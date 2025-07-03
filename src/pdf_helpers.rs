// src/pdf_helpers.rs

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use std::io::{Error, ErrorKind, Result};

// Helper to convert lopdf::Error to std::io::Error
fn lopdf_err_to_io(err: lopdf::Error) -> Error {
    Error::new(ErrorKind::Other, err)
}

/// Gets or creates a mutable dictionary from a parent dictionary.
/// If the dictionary does not exist, it is created and inserted into the parent.
/// If the dictionary exists as an indirect reference, it is resolved.
fn get_or_create_dictionary_mut<'a>(
    doc: &'a mut Document,
    parent: &'a mut Dictionary,
    key: &[u8],
) -> Result<&'a mut Dictionary> {
    if parent.has(key) {
        let object = parent.get_mut(key).unwrap(); // Safe to unwrap because we checked.
        match object {
            Object::Reference(id) => doc
                .get_object_mut(*id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .or_else(|_| Err(Error::new(ErrorKind::InvalidData, "Object is not a dictionary"))),
            Object::Dictionary(inline_dict) => {
                let new_dict = inline_dict.clone();
                let new_id = doc.add_object(new_dict);
                *object = Object::Reference(new_id);
                doc.get_object_mut(new_id)
                    .map_err(lopdf_err_to_io)?
                    .as_dict_mut()
                    .or_else(|_| Err(Error::new(ErrorKind::InvalidData, "Newly created object is not a dictionary")))
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "Resources key points to something other than a dictionary or reference.",
            )),
        }
    } else {
        let new_dict = Dictionary::new();
        let new_id = doc.add_object(new_dict);
        parent.set(key.to_vec(), Object::Reference(new_id));
        doc.get_object_mut(new_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Newly created object is not a dictionary"))
    }
}

/// Gets the object ID of a page from a document.
fn get_page_object_id_from_doc(doc: &Document, page_num: u32) -> Result<ObjectId> {
    doc.get_pages()
        .get(&page_num)
        .copied()
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Page {} not found in source document", page_num)))
}


/// Copies a page from a source document to the destination document.
/// It also copies all referenced objects, such as fonts and images.
pub fn copy_page(
    dest_doc: &mut Document,
    source_doc: &Document,
    page_num: u32,
) -> Result<ObjectId> {
    let page_id = get_page_object_id_from_doc(source_doc, page_num)?;

    let new_page_id = dest_doc.copy_object(source_doc, &page_id).map_err(lopdf_err_to_io)?;

    let pages_id = dest_doc
        .catalog()
        .map_err(lopdf_err_to_io)?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| Error::new(ErrorKind::NotFound, "Pages object not found in destination document"))?;

    let pages = dest_doc
        .get_object_mut(pages_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;

    let kids_entry = pages
        .get_mut(b"Kids")
        .map_err(|_| Error::new(ErrorKind::NotFound, "Kids array not found in Pages dictionary"))?;

    match kids_entry {
        Object::Array(kids) => {
            kids.push(Object::Reference(new_page_id));
        }
        Object::Reference(ref_id) => {
            let kids = dest_doc
                .get_object_mut(*ref_id)
                .map_err(lopdf_err_to_io)?
                .as_array_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Kids object is not an array"))?;
            kids.push(Object::Reference(new_page_id));
        }
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Kids object is not an array or a reference",
            ));
        }
    }

    let count = pages
        .get(b"Count")
        .and_then(Object::as_i64)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Page count (`Count`) is missing or not an integer"))?;
    pages.set(b"Count".to_vec(), Object::Integer(count + 1));

    Ok(new_page_id)
}

/// Overlays the content of a source page onto a destination page.
pub fn overlay_page(
    dest_doc: &mut Document,
    dest_page_id: ObjectId,
    overlay_doc: &Document,
    overlay_page_num: u32,
) -> Result<()> {
    let overlay_page_id = overlay_doc
        .get_page_object_id(overlay_page_num)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Overlay page not found"))?;

    let new_xobject_id = dest_doc.copy_object(overlay_doc, &overlay_page_id).map_err(lopdf_err_to_io)?;

    let xobject_dict = dest_doc
        .get_object_mut(new_xobject_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "XObject is not a dictionary"))?;
    xobject_dict.set(b"Type", Object::Name(b"XObject".to_vec()));
    xobject_dict.set(b"Subtype", Object::Name(b"Form".to_vec()));

    let page_dict = dest_doc
        .get_object_mut(dest_page_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;

    let xobject_name = format!("Ov{}", new_xobject_id.0);
    {
        let resources = get_or_create_dictionary_mut(dest_doc, page_dict, b"Resources")?;
        let xobjects = get_or_create_dictionary_mut(dest_doc, resources, b"XObject")?;
        xobjects.set(xobject_name.as_bytes().to_vec(), Object::Reference(new_xobject_id));
    }

    let content_op = Operation::new("Do", vec![Object::Name(xobject_name.as_bytes().to_vec())]);
    let content = Content { operations: vec![Operation::new("q", vec![]), content_op, Operation::new("Q", vec![])] };
    let content_stream = Stream::new(dictionary! {}, content.encode().map_err(lopdf_err_to_io)?);
    let content_id = dest_doc.add_object(content_stream);

    if let Ok(contents) = page_dict.get_mut(b"Contents") {
        match contents {
            Object::Array(ref mut arr) => arr.push(Object::Reference(content_id)),
            Object::Reference(id) => {
                let old_id = id;
                *contents = Object::Array(vec![Object::Reference(old_id), Object::Reference(content_id)]);
            }
            _ => return Err(Error::new(ErrorKind::InvalidData, "Unexpected page Contents type")),
        }
    } else {
        page_dict.set(b"Contents", Object::Reference(content_id));
    }

    Ok(())
}

/// Adds text to a page at a specific position.
pub fn add_text(
    dest_doc: &mut Document,
    page_id: ObjectId,
    text: &str,
    x: i32,
    y: i32,
) -> Result<()> {
    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    };
    let font_id = dest_doc.add_object(font_dict);

    let page_dict = dest_doc
        .get_object_mut(page_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .or_else(|_| Err(Error::new(ErrorKind::InvalidData, "Page object is not a dictionary")))?;

    let font_key = format!("F{}", font_id.0);
    {
        let resources = get_or_create_dictionary_mut(dest_doc, page_dict, b"Resources")?;
        let fonts = get_or_create_dictionary_mut(dest_doc, resources, b"Font")?;
        fonts.set(font_key.as_bytes().to_vec(), Object::Reference(font_id));
    }

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(font_key.as_bytes().to_vec()), 12.into()]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_stream = Stream::new(dictionary! {}, content.encode().map_err(lopdf_err_to_io)?);
    let content_id = dest_doc.add_object(content_stream);

    if let Ok(contents) = page_dict.get_mut(b"Contents") {
        match contents {
            Object::Array(ref mut arr) => arr.push(Object::Reference(content_id)),
            Object::Reference(id) => {
                let old_id = id;
                *contents = Object::Array(vec![Object::Reference(*old_id), Object::Reference(content_id)]);
            }
            _ => return Err(Error::new(ErrorKind::InvalidData, "Unexpected page Contents type")),
        }
    } else {
        page_dict.set(b"Contents", Object::Reference(content_id));
    }

    Ok(())
}

/// Creates a new, blank page with the specified dimensions and adds it to the document.
pub fn create_blank_page(dest_doc: &mut Document, width: f32, height: f32) -> Result<ObjectId> {
    let media_box = vec![0.0.into(), 0.0.into(), width.into(), height.into()];
    let resources_id = dest_doc.add_object(dictionary! {});
    let content_id = dest_doc.add_object(Stream::new(dictionary! {}, vec![]));

    let page = dictionary! {
        "Type" => "Page",
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = dest_doc.add_object(page);

    let pages_id = dest_doc
        .catalog()
        .map_err(lopdf_err_to_io)?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| Error::new(ErrorKind::NotFound, "Pages object not found in destination document"))?;

    let pages = dest_doc
        .get_object_mut(pages_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;

    let kids_entry = pages
        .get_mut(b"Kids")
        .map_err(|_| Error::new(ErrorKind::NotFound, "Kids array not found in Pages dictionary"))?;

    match kids_entry {
        Object::Array(kids) => {
            kids.push(page_id.into());
        }
        Object::Reference(ref_id) => {
            let kids = dest_doc
                .get_object_mut(*ref_id)
                .map_err(lopdf_err_to_io)?
                .as_array_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Kids object is not an array"))?;
            kids.push(page_id.into());
        }
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Kids object is not an array or a reference",
            ));
        }
    }

    let page_object = dest_doc
        .get_object_mut(page_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;
    page_object.set(b"Parent".to_vec(), Object::Reference(pages_id));

    let count = pages
        .get(b"Count")
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Page count (`Count`) is missing or not an integer"))?;
    pages.set(b"Count".to_vec(), Object::Integer(count + 1));

    Ok(page_id)
}
