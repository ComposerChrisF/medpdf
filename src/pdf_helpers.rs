use lopdf::{Document, Object, ObjectId, Stream, Dictionary};
use std::collections::BTreeMap;
use crate::error::{Result, PdfMergeError};


/// Gets the object ID of a page from a document.
pub fn get_page_object_id_from_doc(doc: &Document, page_num: u32) -> Result<ObjectId> {
    doc.get_pages()
        .get(&page_num)
        .copied()
        .ok_or_else(|| PdfMergeError::new(format!("Page {} not found in source document", page_num)))
}

pub fn deep_copy_object_by_id(
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

pub fn deep_copy_object(
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
