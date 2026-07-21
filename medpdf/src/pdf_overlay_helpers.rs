//! Shared helpers for overlay and place-page operations:
//! resource key collection, renaming, content stream normalization, and modification.

use crate::error::{MedpdfError, Result};
use crate::pdf_helpers::{self, KEY_KIDS, KEY_PAGE, KEY_PAGES, KEY_RESOURCES, KEY_TYPE};
use log::warn;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
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

/// Renames a single name operand in place if it maps to a post-collision name.
fn rename_if_mapped(operand: &mut Object, key_mapping: &HashMap<Vec<u8>, Vec<u8>>) {
    if let Ok(name) = operand.as_name()
        && let Some(name_new) = key_mapping.get(name)
    {
        *operand = Object::Name(name_new.clone());
    }
}

/// Renames the resource-name operands of a single operation to their
/// post-collision names — but only for operators that actually consume a *named
/// resource*, so a `/Foo` that is a marked-content tag, an inline colorspace
/// abbreviation, or any other coincidental name is never rewritten just because a
/// source resource happened to share its spelling (bug-0018 "Related" note).
fn rename_resource_operands(operation: &mut Operation, key_mapping: &HashMap<Vec<u8>, Vec<u8>>) {
    match operation.operator.as_str() {
        // First operand names the resource:
        //   /F Tf (font)   /Name Do (xobject)   /GS gs (extgstate)
        //   /Sh sh (shading)   /CS cs | /CS CS (colorspace resource)
        "Tf" | "Do" | "gs" | "sh" | "cs" | "CS" => {
            if let Some(first) = operation.operands.first_mut() {
                rename_if_mapped(first, key_mapping);
            }
        }
        // scn/SCN may carry a trailing pattern-name operand (or only numbers).
        "scn" | "SCN" => {
            if let Some(last) = operation.operands.last_mut() {
                rename_if_mapped(last, key_mapping);
            }
        }
        // BDC/DP: `/Tag /Properties` — only the properties operand (index 1) names
        // a /Properties resource; the tag lives in a separate namespace.
        "BDC" | "DP" => {
            if let Some(props) = operation.operands.get_mut(1) {
                rename_if_mapped(props, key_mapping);
            }
        }
        _ => {}
    }
}

/// Rewrites *source* content streams for overlay/place: renames their resource
/// references to the post-collision names and wraps the whole thing in a single
/// `q`/`Q` pair so the placed graphics state cannot leak. Returns the new
/// `/Contents` reference array — always a single combined stream (or empty, if the
/// source had no content).
///
/// The fragments are concatenated (newline-separated) and decoded **once**, not
/// per fragment. PDF 32000-1 §7.8.2 defines a page's content as the concatenation
/// of its `/Contents` streams, and the split between fragments may fall at any
/// token boundary — an operator and its operands can straddle two fragments.
/// Decoding fragments independently silently drops any operation split across the
/// boundary, and truncates everything after any token lopdf cannot parse
/// (bug-0019). Parsing the concatenation is exactly what a renderer sees, so no
/// operation can straddle a parse boundary.
///
/// Renaming inherently requires re-encoding, so this path also detects inline
/// images (`BI … ID … EI`) and fails loudly: lopdf 0.42's content encoder cannot
/// round-trip an inline image without corrupting or deleting it (see
/// `LOPDF_INLINE_IMAGE_BUG.md` at the repo root). A diagnosable error beats silent
/// image loss. (Destination streams, which need no renaming, are never sent here —
/// see `isolate_dest_content_streams`.)
pub(crate) fn rename_source_content_streams(
    dest_doc: &mut Document,
    contents_arr: &[Object],
    key_mapping: &HashMap<Vec<u8>, Vec<u8>>,
) -> Result<Vec<Object>> {
    let mut ids = Vec::with_capacity(contents_arr.len());
    for content_ref_obj in contents_arr {
        ids.push(content_ref_obj.as_reference()?);
    }
    let Some((&target_id, rest_ids)) = ids.split_first() else {
        return Ok(Vec::new()); // source page had no content
    };

    // Concatenate every fragment into one buffer, newline-separated so tokens
    // cannot fuse across a fragment boundary, then decode the whole thing once.
    let mut combined: Vec<u8> = Vec::new();
    for &id in &ids {
        let stream = dest_doc.get_object_mut(id)?.as_stream_mut()?;
        if stream.is_compressed() {
            stream.decompress()?;
        }
        if !combined.is_empty() {
            combined.push(b'\n');
        }
        combined.extend_from_slice(&stream.content);
    }

    let mut content = Content::decode(&combined)?;
    let mut cumulative_q_balance = 0_isize;
    for operation in content.operations.iter_mut() {
        if operation.operator == "BI" {
            return Err(MedpdfError::new(
                "overlay/place source page contains an inline image (BI ... ID ... EI), \
                 which is unsupported: lopdf 0.42's content-stream encoder cannot round-trip \
                 an inline image without corrupting it (see LOPDF_INLINE_IMAGE_BUG.md at the \
                 repo root). Convert the inline image to an image XObject in the source PDF \
                 and retry.",
            ));
        }
        match operation.operator.as_str() {
            "q" => cumulative_q_balance += 1,
            "Q" => cumulative_q_balance -= 1,
            _ => {}
        }
        rename_resource_operands(operation, key_mapping);
    }

    // Wrap the combined stream in a single q/Q pair.
    content.operations.insert(0, Operation::new("q", vec![]));
    content.operations.push(Operation::new("Q", vec![]));
    if cumulative_q_balance < 0 {
        warn!(
            "Source content streams have {} more Q than q operators (negative balance)",
            -cumulative_q_balance
        );
    }
    for _ in 0..cumulative_q_balance {
        content.operations.push(Operation::new("Q", vec![]));
    }

    // Write the combined, renamed content back into the first fragment's stream,
    // and drop the now-defunct extra fragments (freshly deep-copied here, so they
    // are referenced only by this array). set_content (never a raw
    // `content_stream.content = ...`) keeps /Length in sync with the new body — a
    // stale /Length makes lopdf drop the stream body on reload.
    let target = dest_doc.get_object_mut(target_id)?.as_stream_mut()?;
    target.set_content(content.encode()?);
    target.compress()?;
    for &id in rest_ids {
        dest_doc.objects.remove(&id);
    }

    Ok(vec![Object::Reference(target_id)])
}

/// Isolates a destination page's existing content from content that will be
/// concatenated after it, WITHOUT re-encoding the destination streams.
///
/// The old approach decoded and re-encoded every destination stream just to add a
/// `q`/`Q` wrapper. That round trip corrupts inline images under lopdf 0.42, and —
/// because a page's `/Contents` streams can be shared by reference — it also
/// mutated any *other* page sharing them (bug-0018). Instead, this prepends a
/// standalone `q` stream and appends a standalone stream of `Q`s: one to match the
/// `q`, plus one per unclosed `q` the destination content left open. The
/// destination's own streams are never touched, so their bytes — inline images
/// included — survive verbatim. This mirrors the watermark `insert_content_stream`
/// mechanism, and is the isolation primitive reused by later fixes.
///
/// Returns a new `/Contents` reference array: `[q] ++ contents ++ [Q…]`. An empty
/// input (page had no content) is returned unchanged.
pub(crate) fn isolate_dest_content_streams(
    dest_doc: &mut Document,
    contents_arr: Vec<Object>,
) -> Result<Vec<Object>> {
    if contents_arr.is_empty() {
        return Ok(contents_arr);
    }

    // Read-only: count the q/Q balance of the untouched destination streams.
    let mut content_ids = Vec::with_capacity(contents_arr.len());
    for obj in &contents_arr {
        content_ids.push(obj.as_reference()?);
    }
    let q_balance = pdf_helpers::count_q_balance(dest_doc, &content_ids)?;
    if q_balance < 0 {
        warn!(
            "Destination content streams have {} more Q than q operators (negative balance)",
            -q_balance
        );
    }

    // Standalone wrappers as raw bytes — no decode/encode, so nothing to corrupt.
    let open_id = dest_doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let num_closing_qs = 1 + q_balance.max(0) as usize;
    let close_id = dest_doc.add_object(Stream::new(
        Dictionary::new(),
        "Q\n".repeat(num_closing_qs).into_bytes(),
    ));

    let mut wrapped = Vec::with_capacity(contents_arr.len() + 2);
    wrapped.push(Object::Reference(open_id));
    wrapped.extend(contents_arr);
    wrapped.push(Object::Reference(close_id));
    Ok(wrapped)
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
                ));
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
            ));
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
            ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> HashMap<Vec<u8>, Vec<u8>> {
        let mut m = HashMap::new();
        m.insert(b"F1".to_vec(), b"F1_o".to_vec());
        m.insert(b"P1".to_vec(), b"P1_o".to_vec());
        m.insert(b"MC0".to_vec(), b"MC0_o".to_vec());
        m
    }

    fn name(bytes: &[u8]) -> Object {
        Object::Name(bytes.to_vec())
    }

    #[test]
    fn renames_font_operand_of_tf() {
        let mut op = Operation::new("Tf", vec![name(b"F1"), Object::Integer(12)]);
        rename_resource_operands(&mut op, &mapping());
        assert_eq!(op.operands[0], name(b"F1_o"));
        assert_eq!(
            op.operands[1],
            Object::Integer(12),
            "the size operand is untouched"
        );
    }

    #[test]
    fn renames_trailing_pattern_name_of_scn() {
        let mut op = Operation::new("scn", vec![name(b"P1")]);
        rename_resource_operands(&mut op, &mapping());
        assert_eq!(op.operands[0], name(b"P1_o"));
    }

    #[test]
    fn bdc_renames_properties_but_not_the_tag() {
        // `BDC /F1 /MC0`: /F1 is the marked-content TAG (separate namespace — must
        // NOT be renamed even though it maps); /MC0 is the /Properties resource.
        let mut op = Operation::new("BDC", vec![name(b"F1"), name(b"MC0")]);
        rename_resource_operands(&mut op, &mapping());
        assert_eq!(op.operands[0], name(b"F1"), "the tag must be left alone");
        assert_eq!(
            op.operands[1],
            name(b"MC0_o"),
            "the properties resource is renamed"
        );
    }

    #[test]
    fn leaves_operands_of_non_resource_operators_alone() {
        // A coincidental resource-shaped name on an operator that consumes no named
        // resource (here a marked-content point tag) must never be rewritten.
        let mut op = Operation::new("MP", vec![name(b"F1")]);
        rename_resource_operands(&mut op, &mapping());
        assert_eq!(op.operands[0], name(b"F1"));
    }
}
