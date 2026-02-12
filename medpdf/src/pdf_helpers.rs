use crate::error::{PdfMergeError, Result};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::BTreeMap;

pub const KEY_TYPE: &[u8] = b"Type";
pub const KEY_PARENT: &[u8] = b"Parent";
pub const KEY_PAGES: &[u8] = b"Pages";
pub const KEY_PAGE: &[u8] = b"Page";
pub const KEY_KIDS: &[u8] = b"Kids";
pub const KEY_COUNT: &[u8] = b"Count";
pub const KEY_RESOURCES: &[u8] = b"Resources";
pub const KEY_CONTENTS: &[u8] = b"Contents";
pub const KEY_FONT: &[u8] = b"Font";
pub const KEY_FONT_DESCRIPTOR: &[u8] = b"FontDescriptor";
pub const KEY_MEDIA_BOX: &[u8] = b"MediaBox";
pub const KEY_EXTGSTATE: &[u8] = b"ExtGState";

/// Walk the page tree to find the inherited MediaBox for a page.
/// Returns `[x0, y0, x1, y1]` if found on the page or any ancestor node.
pub fn get_page_media_box(doc: &Document, page_id: ObjectId) -> Option<[f32; 4]> {
    let mut current_id = page_id;
    while let Ok(dict) = doc.get_dictionary(current_id) {
        if let Ok(mb) = dict.get(KEY_MEDIA_BOX) {
            if let Ok(arr) = mb.as_array() {
                if arr.len() >= 4 {
                    let x0 = obj_as_f32(&arr[0])?;
                    let y0 = obj_as_f32(&arr[1])?;
                    let x1 = obj_as_f32(&arr[2])?;
                    let y1 = obj_as_f32(&arr[3])?;
                    return Some([x0, y0, x1, y1]);
                }
            }
        }
        // Follow /Parent up the page tree
        match dict.get(KEY_PARENT) {
            Ok(Object::Reference(parent_id)) => current_id = *parent_id,
            _ => break,
        }
    }
    None
}

/// Extract an f32 from a lopdf Object (Integer or Real).
fn obj_as_f32(obj: &Object) -> Option<f32> {
    obj.as_float()
        .ok()
        .or_else(|| obj.as_i64().ok().map(|i| i as f32))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    In,
    Mm,
} // TODO: Add Pt, Cm, Percent (of page)

impl Unit {
    pub fn to_points(self, value: f32) -> f32 {
        const POINTS_PER_INCH: f32 = 72.0;
        const POINTS_PER_MM: f32 = POINTS_PER_INCH / 25.4;
        match self {
            Unit::In => value * POINTS_PER_INCH,
            Unit::Mm => value * POINTS_PER_MM,
        }
    }
}

/// Gets the object ID of a page from a document.
pub fn get_page_object_id_from_doc(doc: &Document, page_num: u32) -> Result<ObjectId> {
    doc.get_pages().get(&page_num).copied().ok_or_else(|| {
        PdfMergeError::new(format!("Page {} not found in source document", page_num))
    })
}

pub fn deep_copy_object_by_id(
    dest_doc: &mut Document,
    source_doc: &Document,
    source_object_id: ObjectId,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>, // maps source_object_id to dest object_id
) -> Result<ObjectId> {
    if let Some(&new_id) = copied_objects.get(&source_object_id) {
        return Ok(new_id);
    }

    let new_obj = deep_copy_object(
        dest_doc,
        source_doc,
        source_doc.get_object(source_object_id)?,
        copied_objects,
    )?;
    let new_id = dest_doc.add_object(new_obj);
    copied_objects.insert(source_object_id, new_id);
    Ok(new_id)
}

pub fn deep_copy_object(
    dest_doc: &mut Document,
    source_doc: &Document,
    source_object: &Object,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>, // maps source_object_id to dest object_id
) -> Result<Object> {
    let new_obj = match source_object {
        Object::Reference(_) => {
            return Err(PdfMergeError::new(
                "deep_copy_object() called on a Object::Reference!",
            ));
        }
        Object::Dictionary(source_dict) => {
            let mut dest_dict = Dictionary::new();
            for (key, value) in source_dict.iter() {
                if key == KEY_PARENT {
                    continue;
                } // We never want to deep copy *up* the tree, as we'll then copy the whole document!
                if let Object::Reference(id) = value {
                    dest_dict.set(
                        key.clone(),
                        Object::Reference(deep_copy_object_by_id(
                            dest_doc,
                            source_doc,
                            *id,
                            copied_objects,
                        )?),
                    );
                } else {
                    dest_dict.set(
                        key.clone(),
                        deep_copy_object(dest_doc, source_doc, value, copied_objects)?,
                    );
                }
            }
            Object::Dictionary(dest_dict)
        }
        Object::Array(source_arr) => {
            let mut dest_arr = Vec::<Object>::with_capacity(source_arr.len());
            for item in source_arr.iter() {
                if let Object::Reference(id) = item {
                    dest_arr.push(Object::Reference(deep_copy_object_by_id(
                        dest_doc,
                        source_doc,
                        *id,
                        copied_objects,
                    )?));
                } else {
                    dest_arr.push(deep_copy_object(
                        dest_doc,
                        source_doc,
                        item,
                        copied_objects,
                    )?)
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
                    dest_dict.set(
                        key.clone(),
                        Object::Reference(deep_copy_object_by_id(
                            dest_doc,
                            source_doc,
                            *id,
                            copied_objects,
                        )?),
                    );
                } else {
                    dest_dict.set(
                        key.clone(),
                        deep_copy_object(dest_doc, source_doc, value, copied_objects)?,
                    );
                }
            }

            let new_stream = Stream::new(dest_dict, source_content.clone());
            Object::Stream(new_stream)
        }
        _ => source_object.clone(),
    };

    Ok(new_obj)
}
