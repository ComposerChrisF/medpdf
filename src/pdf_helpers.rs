// src/pdf_helpers.rs

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use std::fmt::Debug;
use std::collections::BTreeMap;

pub enum PdfMergeError {
    Io(std::io::Error),
    LoPdf(lopdf::Error),
    FontKit(font_kit::error::SelectionError),
    Message(String),
}
impl PdfMergeError {
    fn new<T: Into<String>>(msg: T) -> Self {
        PdfMergeError::Message(msg.into())
    }
}
impl From<lopdf::Error> for PdfMergeError {
    fn from(err: lopdf::Error) -> Self {
        PdfMergeError::LoPdf(err)
    }
}
impl From<std::io::Error> for PdfMergeError {
    fn from(err: std::io::Error) -> Self {
        PdfMergeError::Io(err)
    }
}
impl From<&str> for PdfMergeError {
    fn from(err: &str) -> Self {
        PdfMergeError::Message(err.into())
    }
}
impl From<String> for PdfMergeError {
    fn from(err: String) -> Self {
        PdfMergeError::Message(err)
    }
}
impl From<font_kit::error::SelectionError> for PdfMergeError {
    fn from(err: font_kit::error::SelectionError) -> Self {
        PdfMergeError::FontKit(err)
    }
}

impl Debug for PdfMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => f.debug_tuple("Io").field(e).finish(),
            Self::LoPdf(e) => f.debug_tuple("LoPdf").field(e).finish(),
            Self::Message(e) => f.debug_tuple("Message").field(e).finish(),
            Self::FontKit(e) => f.debug_tuple("FontKit").field(e).finish(),
        }
    }
}
type Error = PdfMergeError;
type Result<T> = std::result::Result<T, Error>;

// Helper to convert lopdf::Error to std::io::Error
fn lopdf_err_to_io(err: lopdf::Error) -> Error {
    PdfMergeError::LoPdf(err)
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
                .get_object_mut(*id)?
                .as_dict_mut()
                .or_else(|_| Err(PdfMergeError::new("Object is not a dictionary"))),
            Object::Dictionary(inline_dict) => {
                let new_dict = inline_dict.clone();
                let new_id = doc.add_object(new_dict);
                *object = Object::Reference(new_id);
                doc.get_object_mut(new_id)
                    .map_err(lopdf_err_to_io)?
                    .as_dict_mut()
                    .or_else(|_| Err(PdfMergeError::new("Newly created object is not a dictionary")))
            }
            _ => Err(PdfMergeError::new("Resources key points to something other than a dictionary or reference.")),
        }
    } else {
        let new_dict = Dictionary::new();
        let new_id = doc.add_object(new_dict);
        parent.set(key.to_vec(), Object::Reference(new_id));
        doc.get_object_mut(new_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("Newly created object is not a dictionary"))
    }
}

/// Gets the object ID of a page from a document.
fn get_page_object_id_from_doc(doc: &Document, page_num: u32) -> Result<ObjectId> {
    doc.get_pages()
        .get(&page_num)
        .copied()
        .ok_or_else(|| PdfMergeError::new(format!("Page {} not found in source document", page_num)))
}

fn deep_copy_object_by_id(
    dest_doc: &mut Document,
    source_doc: &Document,
    source_object_id: ObjectId,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>,  // maps source_object_id to dest object_id
) -> Result<ObjectId> {
    if let Some(&new_id) = copied_objects.get(&source_object_id) {
        return Ok(new_id);
    }

    let new_obj = deep_copy_object(dest_doc, source_doc, source_doc.get_object(source_object_id).map_err(lopdf_err_to_io)?, copied_objects)?;
    let new_id = dest_doc.add_object(new_obj);
    copied_objects.insert(source_object_id, new_id);
    Ok(new_id)
}

fn deep_copy_object(
    dest_doc: &mut Document,
    source_doc: &Document,
    source_object: &Object,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>,  // maps source_object_id to dest object_id
) -> Result<Object> {
    let new_obj = match source_object {
        Object::Reference(_) => {
            return Err(PdfMergeError::new("deep_copy_object() called on a Object::Reference!"));
        }
        Object::Dictionary(source_dict) => {
            let mut dest_dict = Dictionary::new();
            for (key, value) in source_dict.iter() {
                if key == b"Parent" { continue; }
                if let Object::Reference(id) = value {
                    dest_dict.set(key.clone(), Object::Reference(deep_copy_object_by_id(dest_doc, source_doc, *id, copied_objects)?));
                } else {
                    dest_dict.set(key.clone(), deep_copy_object(dest_doc, source_doc, value, copied_objects)?);
                }
            }
            Object::Dictionary(dest_dict)
        }
        Object::Array(source_arr) => {
            let mut dest_arr = Vec::<Object>::with_capacity(source_arr.len());
            for item in source_arr.iter() {
                if let Object::Reference(id) = item {
                    dest_arr.push(Object::Reference(deep_copy_object_by_id(dest_doc, source_doc, *id, copied_objects)?));
                } else {
                    dest_arr.push(deep_copy_object(dest_doc, source_doc, item, copied_objects)?)
                }
            }
            Object::Array(dest_arr)
        }
        Object::Stream(source_stream) => {
            let source_dict = &source_stream.dict;
            let source_content = &source_stream.content;

            let mut dest_dict = Dictionary::new();
            for (key, value) in source_dict.iter() {
                if let Object::Reference(id) = value {
                    dest_dict.set(key.clone(), Object::Reference(deep_copy_object_by_id(dest_doc, source_doc, *id, copied_objects)?));
                } else {
                    dest_dict.set(key.clone(), deep_copy_object(dest_doc, source_doc, value, copied_objects)?);
                }
            }

            let new_stream = Stream::new(dest_dict, source_content.clone());
            Object::Stream(new_stream)
        }
        _ => {
            source_object.clone()
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
    let source_page_id = get_page_object_id_from_doc(source_doc, page_num)?;
    //let source_pages_id = source_doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
    let dest_pages_id = dest_doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();

    let mut copied_objects = BTreeMap::new();
    let new_page_id = deep_copy_object_by_id(dest_doc, source_doc, source_page_id, &mut copied_objects)?;
    let page = dest_doc.get_object_mut(new_page_id).unwrap().as_dict_mut().unwrap();
    page.set(b"Parent", Object::Reference(dest_pages_id));

    let dest_pages_id = dest_doc
        .catalog_mut()
        .map_err(lopdf_err_to_io)?
        .get_mut(b"Pages")
        .map_err(|_| PdfMergeError::new("Pages object not found in destination document"))?
        .as_reference()
        .map_err(|_| PdfMergeError::new("Pages object not a reference"))?;
    let dest_pages = dest_doc
        .get_object_mut(dest_pages_id)
        .map_err(lopdf_err_to_io)?
        .as_dict_mut()
        .map_err(|e| PdfMergeError::new(format!("Pages object is not a dictionary. e={e:?}")))?;

    let new_page_count = {
        let dest_kids = dest_pages
            .get_mut(b"Kids")
            .map_err(|_| PdfMergeError::new("Kids array not found in Pages dictionary"))?
            .as_array_mut()
            .map_err(|_| PdfMergeError::new("Kids object is not an array"))?;
        dest_kids.push(Object::Reference(new_page_id));
        dest_kids.len()
    };
    dest_pages.set(b"Count".to_vec(), Object::Integer(new_page_count as i64));
    println!("NEW PAGE COUNT: {}", new_page_count);

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
    let new_xobject_id = deep_copy_object_by_id(dest_doc, overlay_doc, overlay_page_id, &mut copied_objects)?;

    {
        let xobject_dict = dest_doc
            .get_object_mut(new_xobject_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("XObject is not a dictionary"))?;
        xobject_dict.set(b"Type", Object::Name(b"XObject".to_vec()));
        xobject_dict.set(b"Subtype", Object::Name(b"Form".to_vec()));
    }

    let xobject_name = format!("Ov{}", new_xobject_id.0);

    let resources_id = {
        let resources_obj = dest_doc
            .get_object(dest_page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?
            .get(b"Resources")
            .cloned();

        let (id, needs_update) = match resources_obj {
            Ok(Object::Reference(id)) => (id, false),
            Ok(Object::Dictionary(dict)) => (dest_doc.add_object(dict), true),
            Ok(v) => (dest_doc.add_object(v), true),
            Err(_) => return Err(PdfMergeError::new("Resources is not a dictionary or reference")),
        };

        if needs_update {
            let page_dict = dest_doc
                .get_object_mut(dest_page_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;
            page_dict.set(b"Resources", Object::Reference(id));
        }
        id
    };

    let xobjects_id = {
        let xobjects_obj = dest_doc
            .get_object(resources_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| PdfMergeError::new("Resources object is not a dictionary"))?
            .get(b"XObject")
            .cloned();

        let (id, needs_update) = match xobjects_obj {
            Ok(Object::Reference(id)) => (id, false),
            Ok(Object::Dictionary(dict)) => (dest_doc.add_object(dict), true),
            Ok(v) => (dest_doc.add_object(v), true),
            Err(_) => return Err(PdfMergeError::new("XObject is not a dictionary or reference")),
        };

        if needs_update {
            let resources_dict = dest_doc
                .get_object_mut(resources_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| PdfMergeError::new("Resources object is not a dictionary"))?;
            resources_dict.set(b"XObject", Object::Reference(id));
        }
        id
    };

    {
        let xobjects = dest_doc
            .get_object_mut(xobjects_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("XObjects object is not a dictionary"))?;
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
            .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

        if let Ok(contents) = page_dict.get_mut(b"Contents") {
            match contents {
                Object::Array(ref mut arr) => arr.push(Object::Reference(content_id)),
                Object::Reference(id) => {
                    let old_id = *id;
                    *contents = Object::Array(vec![Object::Reference(old_id), Object::Reference(content_id)]);
                }
                _ => return Err(PdfMergeError::new("Unexpected page Contents type")),
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
            .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;
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
                .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;
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
                    PdfMergeError::new("Resources object is not a dictionary")
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
                        PdfMergeError::new("Resources object is not a dictionary")
                    )?;
                resources_dict_mut.set(b"Font", Object::Reference(id));
            }
            id
        };

        let font_dict_mut = dest_doc
            .get_object_mut(font_dict_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("Font dictionary is not a dictionary"))?;
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
            .or_else(|_| Err(PdfMergeError::new("Page object is not a dictionary")))?;

        if let Ok(contents) = page_dict.get_mut(b"Contents") {
            match contents {
                Object::Array(ref mut arr) => arr.push(Object::Reference(content_id)),
                Object::Reference(id) => {
                    let old_id = *id;
                    *contents =
                        Object::Array(vec![Object::Reference(old_id), Object::Reference(content_id)]);
                }
                _ => {
                    return Err(PdfMergeError::new("Unexpected page Contents type"))
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
        .map_err(|_| PdfMergeError::new("Pages object not found in destination document"))?;

    // Add page to Kids array
    let kids_obj = {
        let pages = dest_doc
            .get_object(pages_id)
            .map_err(lopdf_err_to_io)?
            .as_dict()
            .map_err(|_| PdfMergeError::new("Pages object is not a dictionary"))?;
        pages.get(b"Kids").cloned()
            .map_err(|_| PdfMergeError::new("Kids array not found in Pages dictionary"))?
    };

    match kids_obj {
        Object::Array(mut kids) => {
            kids.push(page_id.into());
            let pages = dest_doc
                .get_object_mut(pages_id)
                .map_err(lopdf_err_to_io)?
                .as_dict_mut()
                .map_err(|_| PdfMergeError::new("Pages object is not a dictionary"))?;
            pages.set(b"Kids", Object::Array(kids));
        }
        Object::Reference(kids_id) => {
            let kids = dest_doc
                .get_object_mut(kids_id)
                .map_err(lopdf_err_to_io)?
                .as_array_mut()
                .map_err(|_| PdfMergeError::new("Kids object is not an array"))?;
            kids.push(page_id.into());
        }
        _ => {
            return Err(PdfMergeError::new("Kids object is not an array or a reference"));
        }
    }

    // Set Parent for the new page
    {
        let page_object = dest_doc
            .get_object_mut(page_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;
        page_object.set(b"Parent".to_vec(), Object::Reference(pages_id));
    }

    // Update page count
    {
        let pages = dest_doc
            .get_object_mut(pages_id)
            .map_err(lopdf_err_to_io)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("Pages object is not a dictionary"))?;
        let count = pages
            .get(b"Count")
            .and_then(Object::as_i64)
            .map_err(|_| PdfMergeError::new("Page count (`Count`) is missing or not an integer"))?;
        pages.set(b"Count".to_vec(), Object::Integer(count + 1));
    }

    Ok(page_id)
}
