//! Page overlay — merging content from one PDF page onto another with resource renaming.

use crate::error::{MedpdfError, Result};
use crate::pdf_helpers::{self, KEY_CONTENTS, KEY_PAGES, KEY_RESOURCES};
use crate::pdf_overlay_helpers::{
    accumulate_dictionary_keys, merge_resources_into_dest_page, modify_content_stream,
    rename_resources_in_dict, resolve_contents_to_ref_array,
};
use log::{debug, trace};
use lopdf::{Document, Object, ObjectId};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Overlays the content of a source page onto a destination page.
pub fn overlay_page(
    dest_doc: &mut Document,
    dest_page_id: ObjectId,
    overlay_doc: &Document,
    overlay_page_num: u32,
) -> Result<()> {
    let overlay_page_id = pdf_helpers::get_page_object_id_from_doc(overlay_doc, overlay_page_num)?;

    let overlay_page = overlay_doc.get_dictionary(overlay_page_id)?;
    let mut copied_objects = BTreeMap::new();

    // Get overlay's Page/Contents, normalizing the structs to be an array of references to
    // Object::Stream(), deep copying the streams from overlay to dest doc.
    debug!("Standardizing and cloning overlay's /Contents");
    let overlay_contents = overlay_page.get(KEY_CONTENTS)?;
    let overlay_contents_arr_new = resolve_contents_to_ref_array(
        dest_doc, Some(overlay_doc), overlay_contents, &mut copied_objects,
        &format!("Page {overlay_page_id:?}"),
    )?;

    // Generate deep copy of overlay's page's /Resources dictionary, normalizing it to be a
    // reference (rather than an embedded resource).
    debug!("Generating deep copy of overlay's /Resources");
    let overlay_page_resources = overlay_page.get(KEY_RESOURCES)?;
    let overlay_resources_dict_id_new = match overlay_page_resources {
        Object::Dictionary(_) => {
            let d_new = pdf_helpers::deep_copy_object(
                dest_doc,
                overlay_doc,
                overlay_page_resources,
                &mut copied_objects,
            )?;
            dest_doc.add_object(d_new)
        }
        Object::Reference(id) => {
            pdf_helpers::deep_copy_object_by_id(dest_doc, overlay_doc, *id, &mut copied_objects)?
        }
        _ => {
            return Err(MedpdfError::Message(format!(
                "Page {overlay_page_id:?} /Resources must be dictionary or reference to dictionary"
            )))
        }
    };

    // Starting at the root of the *destination* document, build a list (HashSet) of all keys in
    // all /Resource dictionaries (recursing the full page tree), so we can later make sure no
    // names we add to /Resources conflict!
    debug!("Accumulating dictionary keys in destination document");
    let mut keys_used = HashSet::<Vec<u8>>::new();
    accumulate_dictionary_keys(
        &mut keys_used,
        dest_doc,
        dest_doc.catalog()?.get(KEY_PAGES)?.as_reference()?,
    )?;

    // Now generate new names for all resources in our copied resources_dict_id_new, mutably updating it.
    debug!("Renaming keys in overlay dictionaries to be unique in destination");
    let mut key_mapping = HashMap::<Vec<u8>, Vec<u8>>::new();
    rename_resources_in_dict(
        &mut key_mapping,
        &mut keys_used,
        dest_doc,
        overlay_resources_dict_id_new,
        b"_o",
    )?;
    if log::log_enabled!(log::Level::Trace) {
        trace!("key_mapping:");
        for (k, v) in key_mapping.iter() {
            trace!(
                "{} => {}",
                String::from_utf8_lossy(k),
                String::from_utf8_lossy(v)
            );
        }
    }

    // Update the Contents streams from the overlay document to use the new dictionary keys
    debug!("Updating overlay Content streams to use new keys");
    modify_content_stream(dest_doc, &overlay_contents_arr_new, Some(&key_mapping))?;
    if log::log_enabled!(log::Level::Trace) {
        trace!("arr_new: {overlay_contents_arr_new:?}");
        for item in overlay_contents_arr_new.iter() {
            let o = dest_doc.get_object(item.as_reference()?)?;
            trace!("o={o:?}");
        }
    }

    // Now add each element of the Contents array to the destination page's /Contents (normalizing
    // the destination /Contents to be an array).
    debug!("Merging overlay's /Contents into the destination page's /Contents array");
    let mut dest_contents_arr_new = match dest_doc
        .get_object(dest_page_id)?
        .as_dict()?
        .get(KEY_CONTENTS)
    {
        Ok(dest_contents) => {
            let dest_contents = dest_contents.clone();
            resolve_contents_to_ref_array(
                dest_doc, None, &dest_contents, &mut copied_objects,
                &format!("Page {dest_page_id:?}"),
            )?
        }
        Err(_) => Vec::new(),
    };
    debug!("Modifying existing Content streams");
    modify_content_stream(dest_doc, &dest_contents_arr_new, None)?;
    for item in overlay_contents_arr_new.iter() {
        let reference = item.as_reference()?;
        dest_contents_arr_new.push(Object::Reference(reference));
    }
    if log::log_enabled!(log::Level::Trace) {
        if let Some(first) = overlay_contents_arr_new.first() {
            let obj = dest_doc.get_object(first.as_reference()?)?;
            if let Ok(stream) = obj.as_stream() {
                let ops = stream.decode_content()?.operations;
                trace!("First overlay stream: {} ops", ops.len());
            }
        }
    }
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(KEY_CONTENTS, Object::Array(dest_contents_arr_new));

    // Merge renamed /Resources into destination page's /Resources
    debug!("Merge overlay's /Resources dictionary (with keys renamed) into destination page's /Resources");
    merge_resources_into_dest_page(dest_doc, dest_page_id, overlay_resources_dict_id_new)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_overlay_helpers::find_unique_name;
    use lopdf::{dictionary, Dictionary, Object, Stream};

    /// Creates a minimal valid PDF document with one page that has `/Font` and `/XObject` resources.
    fn create_test_pdf_with_resources(
        font_names: &[&str],
        xobject_names: &[&str],
        content_ops: &str,
    ) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let mut font_dict = Dictionary::new();
        for name in font_names {
            let font_obj_id = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
            });
            font_dict.set(name.as_bytes(), Object::Reference(font_obj_id));
        }

        let mut xobj_dict = Dictionary::new();
        for name in xobject_names {
            let xobj_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
            xobj_dict.set(name.as_bytes(), Object::Reference(xobj_id));
        }

        let resources = dictionary! {
            "Font" => font_dict,
            "XObject" => xobj_dict,
        };
        let resources_id = doc.add_object(resources);

        let content_stream = Stream::new(dictionary! {}, content_ops.as_bytes().to_vec());
        let content_id = doc.add_object(content_stream);

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
        };
        let page_id = doc.add_object(page);

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => Object::Integer(1),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        doc
    }

    #[test]
    fn test_find_unique_name_no_collision() {
        let keys = HashSet::new();
        let result = find_unique_name(&keys, b"F1", b"_o").unwrap();
        assert_eq!(result, b"F1_o");
    }

    #[test]
    fn test_find_unique_name_with_collision() {
        let mut keys = HashSet::new();
        keys.insert(b"F1_o".to_vec());
        let result = find_unique_name(&keys, b"F1", b"_o").unwrap();
        assert_eq!(result, b"F1_o1");
    }

    #[test]
    fn test_find_unique_name_multiple_collisions() {
        let mut keys = HashSet::new();
        keys.insert(b"F1_o".to_vec());
        keys.insert(b"F1_o1".to_vec());
        keys.insert(b"F1_o2".to_vec());
        let result = find_unique_name(&keys, b"F1", b"_o").unwrap();
        assert_eq!(result, b"F1_o3");
    }

    #[test]
    fn test_find_unique_name_exhaustion() {
        let mut keys = HashSet::new();
        keys.insert(b"X_o".to_vec());
        for i in 1..10_000 {
            keys.insert(format!("X_o{i}").into_bytes());
        }
        let result = find_unique_name(&keys, b"X", b"_o");
        assert!(result.is_err());
    }

    #[test]
    fn test_overlay_page_basic() {
        let mut dest = create_test_pdf_with_resources(&["F1"], &[], "q\nBT /F1 12 Tf ET\nQ\n");
        let overlay = create_test_pdf_with_resources(&["F2"], &[], "q\nBT /F2 10 Tf ET\nQ\n");

        let dest_page_id = pdf_helpers::get_page_object_id_from_doc(&dest, 1).unwrap();
        overlay_page(&mut dest, dest_page_id, &overlay, 1).unwrap();

        // After overlay, the destination page's Contents should be an array with more streams
        let page_dict = dest.get_dictionary(dest_page_id).unwrap();
        let contents = page_dict.get(KEY_CONTENTS).unwrap();
        let arr = contents.as_array().unwrap();
        assert!(arr.len() >= 2, "Should have multiple content streams after overlay, got {}", arr.len());

        // Verify resources were merged
        let resources_ref = page_dict.get(KEY_RESOURCES).unwrap().as_reference().unwrap();
        let resources = dest.get_dictionary(resources_ref).unwrap();
        let fonts = resources.get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.len() >= 2, "Should have merged font resources, got {}", fonts.len());
    }

    #[test]
    fn test_overlay_page_resource_renaming() {
        // Both dest and overlay use the same font name "F1"
        let mut dest = create_test_pdf_with_resources(&["F1"], &[], "q\nBT /F1 12 Tf ET\nQ\n");
        let overlay = create_test_pdf_with_resources(&["F1"], &[], "q\nBT /F1 10 Tf ET\nQ\n");

        let dest_page_id = pdf_helpers::get_page_object_id_from_doc(&dest, 1).unwrap();
        overlay_page(&mut dest, dest_page_id, &overlay, 1).unwrap();

        let page_dict = dest.get_dictionary(dest_page_id).unwrap();
        let resources_ref = page_dict.get(KEY_RESOURCES).unwrap().as_reference().unwrap();
        let resources = dest.get_dictionary(resources_ref).unwrap();
        let fonts = resources.get(b"Font").unwrap().as_dict().unwrap();

        assert!(fonts.has(b"F1"), "Original font F1 should remain");
        assert!(fonts.len() >= 2, "Should have at least 2 font entries after overlay");

        // Verify that at least one key has the "_o" suffix (indicating renaming)
        let has_renamed = fonts.iter().any(|(k, _)| {
            let key_str = String::from_utf8_lossy(k);
            key_str.contains("_o")
        });
        assert!(has_renamed, "Overlay font should have been renamed with _o suffix");
    }

    #[test]
    fn test_overlay_page_preserves_q_balance() {
        // Destination has unbalanced q (extra q without matching Q)
        let mut dest = create_test_pdf_with_resources(&["F1"], &[], "q\nq\nBT /F1 12 Tf ET\nQ\n");
        let overlay = create_test_pdf_with_resources(&["F2"], &[], "q\nBT /F2 10 Tf ET\nQ\n");

        let dest_page_id = pdf_helpers::get_page_object_id_from_doc(&dest, 1).unwrap();
        overlay_page(&mut dest, dest_page_id, &overlay, 1).unwrap();

        // Collect all content stream bytes and count q/Q balance
        let page_dict = dest.get_dictionary(dest_page_id).unwrap();
        let contents = page_dict.get(KEY_CONTENTS).unwrap().as_array().unwrap();
        let mut total_q = 0i32;
        let mut total_big_q = 0i32;
        for item in contents {
            if let Ok(id) = item.as_reference() {
                if let Ok(obj) = dest.get_object(id) {
                    if let Ok(stream) = obj.as_stream() {
                        let bytes = if stream.is_compressed() {
                            stream.decompressed_content().unwrap_or_default()
                        } else {
                            stream.content.clone()
                        };
                        if let Ok(content) = lopdf::content::Content::decode(&bytes) {
                            for op in &content.operations {
                                match op.operator.as_str() {
                                    "q" => total_q += 1,
                                    "Q" => total_big_q += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(total_q, total_big_q, "q/Q should be balanced: q={total_q}, Q={total_big_q}");
    }
}
