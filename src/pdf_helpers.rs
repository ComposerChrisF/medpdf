// src/pdf_helpers.rs

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use std::io::{Error, ErrorKind, Result};
use std::collections::BTreeMap;

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


fn deep_copy_object(
    dest_doc: &mut Document,
    source_doc: &Document,
    object_id: &ObjectId,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>,
) -> Result<Object> {
    if let Some(new_id) = copied_objects.get(object_id) {
        return Ok(Object::Reference(*new_id));
    }

    let obj = source_doc.get_object(*object_id).map_err(lopdf_err_to_io)?.clone();

    let new_obj = match obj {
        Object::Dictionary(mut dict) => {
            for (_, value) in dict.iter_mut() {
                if let Object::Reference(id) = value {
                    *value = deep_copy_object(dest_doc, source_doc, id, copied_objects)?;
                }
            }
            let new_id = dest_doc.add_object(dict);
            copied_objects.insert(*object_id, new_id);
            Object::Reference(new_id)
        }
        Object::Array(mut arr) => {
            for item in arr.iter_mut() {
                if let Object::Reference(id) = item {
                    *item = deep_copy_object(dest_doc, source_doc, id, copied_objects)?;
                }
            }
            let new_id = dest_doc.add_object(arr);
            copied_objects.insert(*object_id, new_id);
            Object::Reference(new_id)
        }
        Object::Stream(stream) => {
            let mut dict = stream.dict;
            let content = stream.content;

            for (_, value) in dict.iter_mut() {
                if let Object::Reference(id) = value {
                    *value = deep_copy_object(dest_doc, source_doc, id, copied_objects)?;
                }
            }
            let new_stream = Stream::new(dict, content);
            let new_id = dest_doc.add_object(new_stream);
            copied_objects.insert(*object_id, new_id);
            Object::Reference(new_id)
        }
        _ => {
            let new_id = dest_doc.add_object(obj);
            copied_objects.insert(*object_id, new_id);
            Object::Reference(new_id)
        }
    };

    Ok(new_obj)
}

/// Copies a page from a source document to the destination document.
/// It also copies all referenced objects, such as fonts and images.
pub fn copy_page(
    dest_doc: &mut Document,
    source_doc: &Document,
    page_num: u32,
) -> Result<ObjectId> {
    let page_id = get_page_object_id_from_doc(source_doc, page_num)?;

    let mut copied_objects = BTreeMap::new();
    let new_page_object = deep_copy_object(dest_doc, source_doc, &page_id, &mut copied_objects)?;
    let new_page_id = match new_page_object {
        Object::Reference(id) => id,
        _ => return Err(Error::new(ErrorKind::Other, "Copied page is not a reference")),
    };

    let pages_id = dest_doc
        .catalog()
        .map_err(lopdf_err_to_io)?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| Error::new(ErrorKind::NotFound, "Pages object not found in destination document"))?;

    let kids_object_clone = {
        let pages = dest_doc
            .get_object(pages_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;
        pages
            .get(b"Kids")
            .map_err(|_| Error::new(ErrorKind::NotFound, "Kids array not found in Pages dictionary"))?
            .clone()
    };

    match kids_object_clone {
        Object::Reference(ref_id) => {
            let kids = dest_doc
                .get_object_mut(ref_id)
                .map_err(lopdf_err_to_io)?
                .as_array_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Kids object is not an array"))?;
            kids.push(Object::Reference(new_page_id));
        }
        Object::Array(_) => {
            let pages = dest_doc
                .get_object_mut(pages_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;
            let kids_entry = pages
                .get_mut(b"Kids")
                .map_err(|_| Error::new(ErrorKind::NotFound, "Kids array not found in Pages dictionary"))?;
            if let Object::Array(kids) = kids_entry {
                kids.push(Object::Reference(new_page_id));
            }
        }
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Kids object is not an array or a reference",
            ));
        }
    }

    let pages = dest_doc
        .get_object_mut(pages_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;

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
    let overlay_page_id = get_page_object_id_from_doc(overlay_doc, overlay_page_num)?;

    let mut copied_objects = BTreeMap::new();
    let new_xobject = deep_copy_object(dest_doc, overlay_doc, &overlay_page_id, &mut copied_objects)?;
    let new_xobject_id = match new_xobject {
        Object::Reference(id) => id,
        _ => return Err(Error::new(ErrorKind::Other, "Copied XObject is not a reference")),
    };

    {
        let xobject_dict = dest_doc
            .get_object_mut(new_xobject_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "XObject is not a dictionary"))?;
        xobject_dict.set(b"Type", Object::Name(b"XObject".to_vec()));
        xobject_dict.set(b"Subtype", Object::Name(b"Form".to_vec()));
    }

    let xobject_name = format!("Ov{}", new_xobject_id.0);

    let resources_id = {
        let resources_obj = dest_doc
            .get_object(dest_page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?
            .get(b"Resources")
            .cloned();

        let (id, needs_update) = match resources_obj {
            Ok(Object::Reference(id)) => (id, false),
            Ok(Object::Dictionary(dict)) => (dest_doc.add_object(dict), true),
            Ok(v) => (dest_doc.add_object(v), true),
            Err(_) => return Err(Error::new(ErrorKind::InvalidData, "Resources is not a dictionary or reference")),
        };

        if needs_update {
            let page_dict = dest_doc
                .get_object_mut(dest_page_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;
            page_dict.set(b"Resources", Object::Reference(id));
        }
        id
    };

    let xobjects_id = {
        let xobjects_obj = dest_doc
            .get_object(resources_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Resources object is not a dictionary"))?
            .get(b"XObject")
            .cloned();

        let (id, needs_update) = match xobjects_obj {
            Ok(Object::Reference(id)) => (id, false),
            Ok(Object::Dictionary(dict)) => (dest_doc.add_object(dict), true),
            Ok(v) => (dest_doc.add_object(v), true),
            Err(_) => return Err(Error::new(ErrorKind::InvalidData, "XObject is not a dictionary or reference")),
        };

        if needs_update {
            let resources_dict = dest_doc
                .get_object_mut(resources_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Resources object is not a dictionary"))?;
            resources_dict.set(b"XObject", Object::Reference(id));
        }
        id
    };

    {
        let xobjects = dest_doc
            .get_object_mut(xobjects_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "XObjects object is not a dictionary"))?;
        xobjects.set(xobject_name.as_bytes().to_vec(), Object::Reference(new_xobject_id));
    }

    {
        let content_op = Operation::new("Do", vec![Object::Name(xobject_name.as_bytes().to_vec())]);
        let content = Content { operations: vec![Operation::new("q", vec![]), content_op, Operation::new("Q", vec![])] };
        let content_stream = Stream::new(dictionary! {}, content.encode().map_err(lopdf_err_to_io)?);
        let content_id = dest_doc.add_object(content_stream);

        let page_dict = dest_doc
            .get_object_mut(dest_page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;

        if let Ok(contents) = page_dict.get_mut(b"Contents") {
            match contents {
                Object::Array(ref mut arr) => arr.push(Object::Reference(content_id)),
                Object::Reference(id) => {
                    let old_id = *id;
                    *contents = Object::Array(vec![Object::Reference(old_id), Object::Reference(content_id)]);
                }
                _ => return Err(Error::new(ErrorKind::InvalidData, "Unexpected page Contents type")),
            }
        } else {
            page_dict.set(b"Contents", Object::Reference(content_id));
        }
    }

    Ok(())
}

/// Adds text to a page at a specific position.
pub fn add_text(
    dest_doc: &mut Document,
    page_id: ObjectId,
    text: &str,
    _font_data: &[u8], // TODO: Embed font data
    font_name: &str,
    font_size: f32,
    x: i32,
    y: i32,
) -> Result<()> {
    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => font_name.to_string(),
    };
    let font_id = dest_doc.add_object(font_dict);
    let font_key = format!("F{}", font_id.0);

    let resources_id = {
        let page_dict = dest_doc
            .get_object(page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;
        let resources_obj = page_dict.get(b"Resources").cloned();

        let (id, needs_update) = match resources_obj {
            Ok(Object::Reference(id)) => (id, false),
            Ok(Object::Dictionary(dict)) => (dest_doc.add_object(dict), true),
            Ok(v) => (dest_doc.add_object(v), true),
            Err(_) => {
                let new_dict_id = dest_doc.add_object(dictionary! {});
                (new_dict_id, true)
            }
        };

        if needs_update {
            let page_dict_mut = dest_doc
                .get_object_mut(page_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;
            page_dict_mut.set(b"Resources", Object::Reference(id));
        }
        id
    };

    {
        let font_dict_id = {
            let resources_dict = dest_doc
                .get_object(resources_id)
                .map_err(lopdf_err_to_io)?
                .as_dict()
                .map_err(|_|
                    Error::new(ErrorKind::InvalidData, "Resources object is not a dictionary")
                )?;
            let font_obj = resources_dict.get(b"Font").cloned();

            let (id, needs_update) = match font_obj {
                Ok(Object::Reference(id)) => (id, false),
                Ok(Object::Dictionary(dict)) => (dest_doc.add_object(dict), true),
                Ok(v) => (dest_doc.add_object(v), true),
                Err(_) => {
                    let new_dict_id = dest_doc.add_object(dictionary! {});
                    (new_dict_id, true)
                }
            };

            if needs_update {
                let resources_dict_mut = dest_doc
                    .get_object_mut(resources_id)
                    .map_err(lopdf_err_to_io)?
                    .as_dict_mut()
                    .map_err(|_|
                        Error::new(ErrorKind::InvalidData, "Resources object is not a dictionary")
                    )?;
                resources_dict_mut.set(b"Font", Object::Reference(id));
            }
            id
        };

        let font_dict_mut = dest_doc
            .get_object_mut(font_dict_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Font dictionary is not a dictionary"))?;
        font_dict_mut.set(font_key.as_bytes().to_vec(), Object::Reference(font_id));
    }

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![
                Object::Name(font_key.as_bytes().to_vec()),
                font_size.into(),
            ]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_stream = Stream::new(dictionary! {}, content.encode().map_err(lopdf_err_to_io)?);
    let content_id = dest_doc.add_object(content_stream);

    {
        let page_dict = dest_doc
            .get_object_mut(page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .or_else(|_| Err(Error::new(ErrorKind::InvalidData, "Page object is not a dictionary")))?;

        if let Ok(contents) = page_dict.get_mut(b"Contents") {
            match contents {
                Object::Array(ref mut arr) => arr.push(Object::Reference(content_id)),
                Object::Reference(id) => {
                    let old_id = *id;
                    *contents =
                        Object::Array(vec![Object::Reference(old_id), Object::Reference(content_id)]);
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Unexpected page Contents type",
                    ))
                }
            }
        } else {
            page_dict.set(b"Contents", Object::Reference(content_id));
        }
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

    // Add page to Kids array
    let kids_obj = {
        let pages = dest_doc
            .get_object(pages_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;
        pages.get(b"Kids").cloned()
            .map_err(|_| Error::new(ErrorKind::NotFound, "Kids array not found in Pages dictionary"))?
    };

    match kids_obj {
        Object::Array(mut kids) => {
            kids.push(page_id.into());
            let pages = dest_doc
                .get_object_mut(pages_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;
            pages.set(b"Kids", Object::Array(kids));
        }
        Object::Reference(kids_id) => {
            let kids = dest_doc
                .get_object_mut(kids_id)
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

    // Set Parent for the new page
    {
        let page_object = dest_doc
            .get_object_mut(page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Page object is not a dictionary"))?;
        page_object.set(b"Parent".to_vec(), Object::Reference(pages_id));
    }

    // Update page count
    {
        let pages = dest_doc
            .get_object_mut(pages_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Pages object is not a dictionary"))?;
        let count = pages
            .get(b"Count")
            .and_then(Object::as_i64)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Page count (`Count`) is missing or not an integer"))?;
        pages.set(b"Count".to_vec(), Object::Integer(count + 1));
    }

    Ok(page_id)
}
