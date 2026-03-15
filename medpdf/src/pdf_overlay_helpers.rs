//! Shared helpers for overlay and place-page operations:
//! resource key collection, renaming, content stream normalization, and modification.

use crate::error::{MedpdfError, Result};
use crate::pdf_helpers::{self, KEY_KIDS, KEY_PAGE, KEY_PAGES, KEY_RESOURCES, KEY_TYPE};
use log::{trace, warn};
use lopdf::content::Operation;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) fn add_resource_keys(keys: &mut HashSet<Vec<u8>>, dict_resources: &Dictionary) {
    for (_, value) in dict_resources.iter() {
        if let Object::Dictionary(dict) = value {
            for (key, _) in dict.iter() {
                keys.insert(key.clone());
            }
        }
    }
}

/// Collects resource key names from a document's page tree, starting at `start`.
///
/// Recurses into child `/Pages` nodes and individual `/Page` nodes so that
/// per-page `/Resources` dictionaries are also collected, preventing
/// resource-name collisions during overlay.
pub(crate) fn accumulate_dictionary_keys(
    keys: &mut HashSet<Vec<u8>>,
    doc: &Document,
    start: ObjectId,
) -> Result<()> {
    let o = doc.get_object(start)?;
    let dict = match o {
        Object::Dictionary(d) => d,
        _ => return Ok(()),
    };

    let is_page_node = match dict.get(KEY_TYPE) {
        Ok(Object::Name(v)) => v == KEY_PAGES || v == KEY_PAGE,
        _ => false,
    };
    if !is_page_node {
        return Ok(());
    }

    // Collect resource keys from this node's /Resources
    match dict.get(KEY_RESOURCES) {
        Ok(Object::Dictionary(dict_resources)) => {
            add_resource_keys(keys, dict_resources);
        }
        Ok(Object::Reference(id_resources)) => {
            if let Ok(dict_resources) = doc.get_dictionary(*id_resources) {
                add_resource_keys(keys, dict_resources);
            }
        }
        _ => {}
    }

    // Recurse into /Kids for /Pages nodes
    if let Ok(Object::Array(kids)) = dict.get(KEY_KIDS) {
        let child_ids: Vec<ObjectId> = kids
            .iter()
            .filter_map(|obj| obj.as_reference().ok())
            .collect();
        for child_id in child_ids {
            accumulate_dictionary_keys(keys, doc, child_id)?;
        }
    }

    Ok(())
}

pub(crate) fn find_unique_name(
    keys_used: &HashSet<Vec<u8>>,
    key_old: &[u8],
    suffix: &[u8],
) -> Result<Vec<u8>> {
    let mut buffer = Vec::<u8>::with_capacity(key_old.len() + suffix.len() + 5);
    buffer.extend_from_slice(key_old);
    buffer.extend_from_slice(suffix);
    let start_len = buffer.len();
    let mut itoa_buf = itoa::Buffer::new();
    for i in 0..10_000 {
        if i > 0 {
            buffer.truncate(start_len);
            buffer.extend_from_slice(itoa_buf.format(i).as_bytes());
        }
        if !keys_used.contains(&buffer) {
            return Ok(buffer);
        }
    }
    Err(MedpdfError::new("No new unique key could be generated"))
}

pub(crate) fn rename_resources_in_dict(
    key_mapping: &mut HashMap<Vec<u8>, Vec<u8>>,
    keys_used: &mut HashSet<Vec<u8>>,
    dest_doc: &mut Document,
    resources_dict_id_new: ObjectId,
    suffix: &[u8],
) -> Result<()> {
    let dict = dest_doc.get_dictionary_mut(resources_dict_id_new)?;
    for (_, value) in dict.iter_mut() {
        // The unused "key" here is /Font, /XObject, etc.  We don't need to know what key it is
        // When value is a dictionary, it contains key->value pairs for resources.  We can ignore non-dictionary values
        if let Object::Dictionary(dict) = value {
            let list_of_keys = dict
                .iter()
                .map(|(k, _v)| k.clone())
                .collect::<Vec<Vec<u8>>>();
            for key in list_of_keys {
                // If we've already mapped this key, skip regenerating new key as we need to
                // preserve the mapping from old keys (that may be shadowing across multiple
                // dictionaries from the source overlay document).
                if key_mapping.contains_key(&key) {
                    continue;
                }

                // These key/value pairs are a resource_name/resource_value pair.  We need to rename the name.
                let key_new = find_unique_name(keys_used, &key, suffix)?;
                key_mapping.insert(key.clone(), key_new.clone());
                if let Some(v) = dict.remove(&key) {
                    dict.set(key_new, v);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn modify_content_stream(
    dest_doc: &mut Document,
    contents_arr: &[Object],
    key_mapping: Option<&HashMap<Vec<u8>, Vec<u8>>>,
) -> Result<()> {
    let len = contents_arr.len();
    let mut cumulative_q_balance = 0_isize;

    for (idx, content_ref_obj) in contents_arr.iter().enumerate() {
        let content_stream = dest_doc
            .get_object_mut(content_ref_obj.as_reference()?)?
            .as_stream_mut()?;
        if content_stream.is_compressed() {
            content_stream.decompress()?;
        }
        let mut content = content_stream.decode_content()?;
        for operation in content.operations.iter_mut() {
            match &operation.operator[..] {
                "q" => cumulative_q_balance += 1,
                "Q" => cumulative_q_balance -= 1,
                _ => {}
            }
            if let Some(key_mapping) = key_mapping {
                for operand in operation.operands.iter_mut() {
                    // Process only operands that are names...
                    if let Ok(name) = operand.as_name() {
                        // ...that also have a mapping to a new name
                        if let Some(name_new) = key_mapping.get(name) {
                            *operand = Object::Name(name_new.clone());
                        }
                    }
                }
            }
        }
        // Wrap the entire sequence of content streams in a single q/Q pair,
        // rather than wrapping each stream individually. Multiple content streams
        // for one page are concatenated — graphics state must carry across them.
        if idx == 0 {
            content.operations.insert(0, Operation::new("q", vec![]));
        }
        if idx == len - 1 {
            content.operations.push(Operation::new("Q", vec![]));
            // Balance any unmatched q operators across all streams
            if cumulative_q_balance < 0 {
                warn!("Content streams have {} more Q than q operators (negative balance)", -cumulative_q_balance);
            }
            trace!("cumulative_q_balance = {cumulative_q_balance}");
            for _ in 0..cumulative_q_balance {
                warn!("Unbalanced q/Q pairs, adding 'Q'");
                content.operations.push(Operation::new("Q", vec![]));
            }
        }

        if log::log_enabled!(log::Level::Trace) {
            for (i, op) in content.operations.iter().enumerate() {
                if i > 20 {
                    break;
                }
                trace!("op {op:?}");
            }
        }
        content_stream.content = content.encode()?;
        content_stream.compress()?;
    }
    Ok(())
}

/// Normalizes a contents array from a source document, deep-copying each stream/reference into dest_doc.
pub(crate) fn normalize_contents_array(
    dest_doc: &mut Document,
    source_doc: &Document,
    items: &[Object],
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>,
) -> Result<Vec<Object>> {
    let mut refs = Vec::with_capacity(items.len());
    for item in items {
        let id = match item {
            Object::Stream(s) => dest_doc.add_object(s.clone()),
            Object::Reference(id) => {
                pdf_helpers::deep_copy_object_by_id(dest_doc, source_doc, *id, copied_objects)?
            }
            _ => {
                return Err(MedpdfError::new(
                    "Page/Contents array must contain Streams or References",
                ))
            }
        };
        refs.push(Object::Reference(id));
    }
    Ok(refs)
}

/// Merges renamed source resources into a destination page's `/Resources`.
///
/// Normalizes the destination's `/Resources` to be a reference (promoting an inline dictionary
/// if needed), then moves each resource-type sub-dictionary entry from `source_resources_dict_id`
/// into the destination, removing the temporary source dict when done.
pub(crate) fn merge_resources_into_dest_page(
    dest_doc: &mut Document,
    dest_page_id: ObjectId,
    source_resources_dict_id: ObjectId,
) -> Result<()> {
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    let (dict_to_make_object, dict_ref) = match dest_page_dict.get(KEY_RESOURCES) {
        Ok(Object::Dictionary(dict)) => (Some(dict.clone()), None),
        Ok(Object::Reference(reference)) => (None, Some(*reference)),
        Ok(_) => {
            return Err(MedpdfError::new(
                "Destination page /Resources was not a Dictionary nor Reference",
            ))
        }
        Err(_) => (Some(Dictionary::new()), None),
    };
    let dict_ref = match (dict_to_make_object, dict_ref) {
        (Some(dict_to_make_object), None) => {
            dest_doc.add_object(Object::Dictionary(dict_to_make_object))
        }
        (None, Some(dict_ref)) => dict_ref,
        _ => {
            return Err(MedpdfError::new(
                "Internal error: unexpected state in resources normalization",
            ))
        }
    };
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(KEY_RESOURCES, Object::Reference(dict_ref));

    let source_resources_dict = dest_doc
        .get_object(source_resources_dict_id)?
        .as_dict()?
        .clone();
    dest_doc.objects.remove(&source_resources_dict_id);
    let dest_resources = dest_doc.get_object_mut(dict_ref)?.as_dict_mut()?;
    for (resource_type, dict) in source_resources_dict.iter() {
        if dest_resources.get(resource_type).is_err() {
            dest_resources.set(resource_type.clone(), dict.clone());
        } else if let Ok(dict) = dict.as_dict() {
            let dest_resource = dest_resources.get_mut(resource_type)?.as_dict_mut()?;
            for (key, value) in dict.iter() {
                dest_resource.set(key.clone(), value.clone());
            }
        }
    }

    Ok(())
}

/// Resolves a page's `/Contents` value into a normalized `Vec<Object::Reference>`.
///
/// Handles all valid `/Contents` forms: inline Stream, Reference (to Stream or Array), or Array.
/// When `source_doc` is `Some`, streams/references are deep-copied from the source document.
/// When `source_doc` is `None`, the contents are assumed to already reside in `dest_doc`.
pub(crate) fn resolve_contents_to_ref_array(
    dest_doc: &mut Document,
    source_doc: Option<&Document>,
    contents: &Object,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>,
    context: &str,
) -> Result<Vec<Object>> {
    match contents {
        Object::Stream(stream) => {
            let id = dest_doc.add_object(stream.clone());
            Ok(vec![Object::Reference(id)])
        }
        Object::Reference(reference) => {
            // Clone the resolved object to release the borrow before mutating dest_doc.
            let resolved = if let Some(src) = source_doc {
                src.get_object(*reference)?.clone()
            } else {
                dest_doc.get_object(*reference)?.clone()
            };
            match resolved {
                Object::Stream(stream) => {
                    let id = dest_doc.add_object(stream);
                    Ok(vec![Object::Reference(id)])
                }
                Object::Array(a) => {
                    if let Some(src) = source_doc {
                        normalize_contents_array(dest_doc, src, &a, copied_objects)
                    } else {
                        Ok(a)
                    }
                }
                _ => Err(MedpdfError::Message(format!(
                    "{context} /Contents references a non-stream / non-array"
                ))),
            }
        }
        Object::Array(a) => {
            if let Some(src) = source_doc {
                normalize_contents_array(dest_doc, src, a, copied_objects)
            } else {
                Ok(a.clone())
            }
        }
        _ => Err(MedpdfError::Message(format!(
            "{context} /Contents must be stream or array or reference to stream or array"
        ))),
    }
}
