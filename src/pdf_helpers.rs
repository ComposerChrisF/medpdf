// src/pdf_helpers.rs

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use std::collections::BTreeMap;
use crate::error::{Result, PdfMergeError};


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

    let new_obj = deep_copy_object(dest_doc, source_doc, source_doc.get_object(source_object_id)?, copied_objects)?;
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
    let dest_pages_id = dest_doc.catalog()?.get(b"Pages")?.as_reference()?;

    let mut copied_objects = BTreeMap::new();
    let new_page_id = deep_copy_object_by_id(dest_doc, source_doc, source_page_id, &mut copied_objects)?;
    let page = dest_doc.get_object_mut(new_page_id)?.as_dict_mut()?;
    page.set(b"Parent", Object::Reference(dest_pages_id));

    let dest_pages_id = dest_doc
        .catalog_mut()?
        .get_mut(b"Pages")
        .map_err(|_| PdfMergeError::new("Pages object not found in destination document"))?
        .as_reference()
        .map_err(|_| PdfMergeError::new("Pages object not a reference"))?;
    let dest_pages = dest_doc
        .get_object_mut(dest_pages_id)?
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
            .get_object_mut(new_xobject_id)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("XObject is not a dictionary"))?;
        xobject_dict.set(b"Type", Object::Name(b"XObject".to_vec()));
        xobject_dict.set(b"Subtype", Object::Name(b"Form".to_vec()));
    }

    let xobject_name = format!("Ov{}", new_xobject_id.0);

    let resources_id = {
        let resources_obj = dest_doc
            .get_object(dest_page_id)?
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
                .get_object_mut(dest_page_id)?
                .as_dict_mut()
                .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;
            page_dict.set(b"Resources", Object::Reference(id));
        }
        id
    };

    let xobjects_id = {
        let xobjects_obj = dest_doc
            .get_object(resources_id)?
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
                .get_object_mut(resources_id)?
                .as_dict_mut()
                .map_err(|_| PdfMergeError::new("Resources object is not a dictionary"))?;
            resources_dict.set(b"XObject", Object::Reference(id));
        }
        id
    };

    {
        let xobjects = dest_doc
            .get_object_mut(xobjects_id)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("XObjects object is not a dictionary"))?;
        xobjects.set(xobject_name.as_bytes().to_vec(), Object::Reference(new_xobject_id));
    }

    {
        let content_op = Operation::new("Do", vec![Object::Name(xobject_name.as_bytes().to_vec())]);
        let content = Content { operations: vec![Operation::new("q", vec![]), content_op, Operation::new("Q", vec![])] };
        let content_stream = Stream::new(dictionary! {}, content.encode()?);
        let content_id = dest_doc.add_object(content_stream);

        let page_dict = dest_doc
            .get_object_mut(dest_page_id)?
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
    font_data: &[u8], // TODO: Embed font data
    font_name: &str,
    font_size: f32,
    x: i32,
    y: i32,
) -> Result<()> {
    let font_key = add_font_info(dest_doc, page_id, font_data, font_name)?;
    println!("Font key = {font_key}");

    let content = Content {
        operations: vec![
            Operation::new("rg", vec![0.into(), 0.0.into(), 0.51.into()]),
            Operation::new("BT", vec![]),
            Operation::new("Tr", vec![0.into()]),
            Operation::new("Tf", vec![
                Object::Name(font_key.as_bytes().to_vec()),
                font_size.into(),
            ]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    println!("Content={content:?}");
    let content_stream = Stream::new(dictionary! {}, content.encode()?);
    let content_id = dest_doc.add_object(content_stream);

    {
        let page_dict = dest_doc
            .get_object_mut(page_id)?
            .as_dict_mut()
            .or_else(|_| Err(PdfMergeError::new("Page object is not a dictionary")))?;

        if let Ok(contents) = page_dict.get_mut(b"Contents") {
            match contents {
                Object::Array(ref mut arr) => { arr.insert(0, Object::Reference(content_id)); println!("Added Contents to Array!"); },
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
            page_dict.set(b"Contents", Object::Array(vec![Object::Reference(content_id)]));
        }
    }

    Ok(())
}

fn add_font_info(dest_doc: &mut Document, page_id: (u32, u16), font_data: &[u8], font_name: &str) -> Result<String> {
    if font_data.len() == 1 && font_data[0] != '@' as u8 {
        return Ok(format!("F{}", font_data[0]));
    }

    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => if font_data[0] == ('@' as u8) { font_name[1..].to_string() } else { font_name.to_string() },
    };
    let font_id = dest_doc.add_object(font_dict);
    let font_key =format!("F{}", font_id.0);
    let fn_add_font_to_fonts_dict = |dict: &mut Dictionary| { dict.set(font_key.as_bytes(), Object::Reference(font_id)); };

    let fn_add_fonts_to_resources_and_add_font = |resources_dict: &mut Dictionary| -> Result<Option<ObjectId>> {
        let fonts_obj = resources_dict.get_mut(b"Font");
        let fonts_id = match fonts_obj {
            Ok(Object::Reference(id_fonts)) => Some(*id_fonts),
            Ok(Object::Dictionary(dict_fonts)) => { fn_add_font_to_fonts_dict(dict_fonts); None }
            Ok(_) => { return Err(PdfMergeError::new("/Font key of Resource not a Reference nor a Dictionary!")); }
            Err(_) => {
                let mut dict_fonts = dictionary! { };
                fn_add_font_to_fonts_dict(&mut dict_fonts);
                resources_dict.set(b"Font",Object::Dictionary(dict_fonts));
                None
            }
        };
        Ok(fonts_id)
    };

    let page_dict = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

    let resources_obj = page_dict.get_mut(b"Resources");
    let (mut fonts_id, resources_dict_id) = match resources_obj {
        Ok(Object::Reference(id_resources)) => (None, Some(*id_resources)),
        Ok(Object::Dictionary(dict_resources)) => (fn_add_fonts_to_resources_and_add_font(dict_resources)?, None),
        Ok(_) => { return Err(PdfMergeError::new("/Resource key of page not a Reference nor a Dictionary!")); }
        Err(_) => {
            let mut dict_resources = dictionary! { };
            let fonts_id = fn_add_fonts_to_resources_and_add_font(&mut dict_resources)?;
            page_dict.set(b"Resources", Object::Dictionary(dict_resources));
            (fonts_id, None)
        }
    };
    assert!(fonts_id.is_none() || resources_dict_id.is_none());  // Only one of these two is ever set, but both can be None

    if let Some(resources_dict_id) = resources_dict_id {
        let resources_dict = dest_doc.get_object_mut(resources_dict_id)?.as_dict_mut()?;
        assert!(fonts_id.is_none());       // If we entered this branch, then fonts_id should not be set yet!
        fonts_id = fn_add_fonts_to_resources_and_add_font(resources_dict)?;
    }

    if let Some(fonts_id) = fonts_id {
        let fonts_dict = dest_doc.get_object_mut(fonts_id)?.as_dict_mut()?;
        fn_add_font_to_fonts_dict(fonts_dict);
    }

    Ok(font_key)
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
        .catalog()?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| PdfMergeError::new("Pages object not found in destination document"))?;

    // Add page to Kids array
    let pages = dest_doc
        .get_object_mut(pages_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Pages object is not a dictionary"))?;
    let kids = pages.get_mut(b"Kids")
        .map_err(|_| PdfMergeError::new("Kids array not found in Pages dictionary"))?
        .as_array_mut()?;
    kids.push(page_id.into());
    // Update page count
    let new_page_count = kids.len();
    pages.set(b"Count", Object::Integer(new_page_count as i64));

    // Set Parent for the new page
    let page_object = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;
    page_object.set(b"Parent", Object::Reference(pages_id));

    Ok(page_id)
}
