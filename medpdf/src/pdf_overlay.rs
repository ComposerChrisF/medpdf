use crate::error::{PdfMergeError, Result};
use crate::pdf_helpers::{self, KEY_CONTENTS, KEY_KIDS, KEY_PAGE, KEY_PAGES, KEY_RESOURCES, KEY_TYPE};
use log::{debug, trace, warn};
use lopdf::content::Operation;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::{BTreeMap, HashMap, HashSet};

fn add_resource_keys(keys: &mut HashSet<Vec<u8>>, dict_resources: &Dictionary) -> Result<()> {
    for (_, value) in dict_resources.iter() {
        if let Object::Dictionary(dict) = value {
            for (key, _) in dict.iter() {
                keys.insert(key.clone());
            }
        }
    }
    Ok(())
}

/// Collects resource key names from a document's page tree, starting at `start`.
///
/// Recurses into child `/Pages` nodes and individual `/Page` nodes so that
/// per-page `/Resources` dictionaries are also collected, preventing
/// resource-name collisions during overlay.
fn accumulate_dictionary_keys(
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
            add_resource_keys(keys, dict_resources)?;
        }
        Ok(Object::Reference(id_resources)) => {
            if let Ok(dict_resources) = doc.get_dictionary(*id_resources) {
                add_resource_keys(keys, dict_resources)?;
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

fn find_unique_name(
    keys_used: &HashSet<Vec<u8>>,
    key_old: &[u8],
    suffix: &[u8],
) -> Result<Vec<u8>> {
    let mut buffer = Vec::<u8>::with_capacity(key_old.len() + suffix.len() + 5);
    buffer.extend_from_slice(key_old);
    buffer.extend_from_slice(suffix);
    let start_len = buffer.len();
    for i in 0..10_000 {
        if i > 0 {
            buffer.truncate(start_len);
            buffer.extend_from_slice(format!("{i}").as_bytes());
        }
        if !keys_used.contains(&buffer) {
            return Ok(buffer);
        }
    }
    Err(PdfMergeError::new("No new unique key could be generated"))
}

fn rename_resources_in_dict(
    key_mapping: &mut HashMap<Vec<u8>, Vec<u8>>,
    keys_used: &mut HashSet<Vec<u8>>,
    dest_doc: &mut Document,
    resources_dict_id_new: ObjectId,
) -> Result<()> {
    let dict = dest_doc.get_dictionary_mut(resources_dict_id_new)?;
    let new_key_suffix = vec![b'_', b'o'];
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
                let key_new = find_unique_name(keys_used, &key, &new_key_suffix)?;
                key_mapping.insert(key.clone(), key_new.clone());
                if let Some(v) = dict.remove(&key) {
                    dict.set(key_new, v);
                }

                // NO!: keys_used.insert(key);  (See note above of about preserving overlapping keys from source document.)
            }
        }
    }
    Ok(())
}

fn modify_content_stream(
    dest_doc: &mut Document,
    contents_arr: &[Object],
    key_mapping: Option<&HashMap<Vec<u8>, Vec<u8>>>,
) -> Result<()> {
    for content_ref_obj in contents_arr.iter() {
        let content_stream = dest_doc
            .get_object_mut(content_ref_obj.as_reference()?)?
            .as_stream_mut()?;
        if content_stream.is_compressed() {
            content_stream.decompress()?;
        }
        let mut content = content_stream.decode_content()?;
        let mut count_q = 0_isize;
        for operation in content.operations.iter_mut() {
            match &operation.operator[..] {
                "q" => count_q += 1,
                "Q" => count_q -= 1,
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
        // Add bracketing q/Q pair to contain graphics state changes
        content.operations.insert(0, Operation::new("q", vec![]));
        content.operations.push(Operation::new("Q", vec![]));
        // We count q/Q pairs to make sure they are balanced, so that we can add extra "Q" if necessary.
        if count_q < 0 {
            warn!("Content stream has {} more Q than q operators (negative balance)", -count_q);
        }
        trace!("count_q = {count_q}");
        for _ in 0..count_q {
            warn!("Unbalanced q/Q pairs, adding 'Q'");
            content.operations.push(Operation::new("Q", vec![]));
        }

        content_stream.content = content.encode()?;
        content_stream.compress()?;
        if log::log_enabled!(log::Level::Trace) {
            for (i, op) in content_stream
                .decode_content()?
                .operations
                .iter()
                .enumerate()
            {
                if i > 20 {
                    break;
                }
                trace!("op {op:?}");
            }
        }
    }
    Ok(())
}

/// Normalizes a contents array from a source document, deep-copying each stream/reference into dest_doc.
fn normalize_contents_array(
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
                return Err(PdfMergeError::new(
                    "Page/Contents array must contain Streams or References",
                ))
            }
        };
        refs.push(Object::Reference(id));
    }
    Ok(refs)
}

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
    // FUTURE: We should just normalize *all* Documents up-front (i.e. all source/input/overlay
    //         documents as well as the destination document).  That would simplify a lot of logic
    //         throughout the code!  (I.e. we could always assume /Resources is a reference and
    //         is nevery anything else, such as an embedded dictionary.  Similar for /Contents.
    //         We could then also handle nested Pages, flattening their /Resources before our
    //         code even sees any of it.  Also, we could centralize removal of stuff we don't or
    //         can't support when merging.  We could also pre-compute hashes for resources, etc.,
    //         to avoid unneeded object duplication (but only of objects we don't modify!).)
    //         Also, normalizing q/Q pairing (enforcing both proper pairing, and that all content
    //         starts with a q and ends with a Q).
    debug!("Standardizing and cloning overlay's /Contents");
    let overlay_contents = overlay_page.get(KEY_CONTENTS)?;
    let overlay_contents_arr_new = match overlay_contents {
        Object::Stream(stream) => {
            let dest_stream_id = dest_doc.add_object(stream.clone());
            vec![Object::Reference(dest_stream_id)]
        }
        Object::Reference(reference) => {
            let o = overlay_doc.get_object(*reference)?;
            match o {
                Object::Stream(stream) => {
                    let dest_stream_id = dest_doc.add_object(stream.clone());
                    vec![Object::Reference(dest_stream_id)]
                }
                Object::Array(a) => {
                    normalize_contents_array(dest_doc, overlay_doc, a, &mut copied_objects)?
                }
                _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Contents references a non-stream / non-array"))),
            }
        }
        Object::Array(a) => {
            normalize_contents_array(dest_doc, overlay_doc, a, &mut copied_objects)?
        }
        _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Contents must be stream or array or reference to stream or array"))),
    };

    // Generate deep copy of overlay's page's /Resources dictionary, normalizing it to be a
    // reference (rather than an embedded resource).  FUTURE: Also need to copy/merge any parent /Resources...
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
            return Err(PdfMergeError::Message(format!(
                "Page {overlay_page_id:?} /Resources must be dictionary or reference to dictionary"
            )))
        }
    };
    // NOTE: As a side-effect of the deep copy of the overlay's /Resources, we've added an unnecessary
    // Object::Dictionary() to the dest_doc... we'll remove this later to tidy up.

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
    // Make sure the new names are not present in keys.
    debug!("Renaming keys in overlay dictionaries to be unique in destination");
    let mut key_mapping = HashMap::<Vec<u8>, Vec<u8>>::new();
    rename_resources_in_dict(
        &mut key_mapping,
        &mut keys_used,
        dest_doc,
        overlay_resources_dict_id_new,
    )?;
    // We've now renamed the keys in the Resources dict from the overlay (resources_dict_id_new).
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
    // Unobvious, but changing decoded operations does not modify the Content!!!!  It looks
    // like we need to build a *new* Content, modifying the ops as we copy them.
    debug!("Updating overlay Content streams to use new keys");
    if !overlay_contents_arr_new
        .iter()
        .all(|obj| obj.as_reference().is_ok())
    {
        return Err(PdfMergeError::new(
            "Overlay contents array must contain only references",
        ));
    }
    for obj in overlay_contents_arr_new.iter() {
        let o = dest_doc.get_object(obj.as_reference()?)?;
        if o.as_stream().is_err() {
            return Err(PdfMergeError::new(
                "Overlay contents references must point to streams",
            ));
        }
    }
    modify_content_stream(dest_doc, &overlay_contents_arr_new, Some(&key_mapping))?;
    if log::log_enabled!(log::Level::Trace) {
        trace!("arr_new: {overlay_contents_arr_new:?}");
        for item in overlay_contents_arr_new.iter() {
            let o = dest_doc.get_object(item.as_reference()?)?;
            trace!("o={o:?}");
        }
    }

    // Now add each element of the Contents array to the destination pages's /Contents (normalizing
    // the destination /Contents to be an array).
    debug!("Merging overlay's /Contents into the destination page's /Contents array");
    // a. We start by getting a copy of dest page's Contents, converting it to an array of
    //    references, if necessary.
    let dest_contents = dest_doc
        .get_object(dest_page_id)?
        .as_dict()?
        .get(KEY_CONTENTS)?;
    let mut dest_contents_arr_new = match dest_contents {
        Object::Stream(s) => {
            let dest_stream_id = dest_doc.add_object(s.clone());
            vec![Object::Reference(dest_stream_id)]
        },
        Object::Array(a) => a.clone(),
        Object::Reference(reference) => {
            let dest_obj = dest_doc.get_object(*reference)?;
            match dest_obj {
                Object::Stream(s) => {
                    let dest_stream_id = dest_doc.add_object(s.clone());
                    vec![Object::Reference(dest_stream_id)]
                }
                Object::Array(a) => a.clone(),
                _ => return Err(PdfMergeError::Message(format!("Page {dest_page_id:?} /Contents reference must point to stream or array: {dest_contents:?}"))),
            }
        }
        _ => return Err(PdfMergeError::Message(format!("Page {dest_page_id:?} /Contents must be stream or array or reference to one: {dest_contents:?}"))),
    };
    // b. For the original Content, we need to make sure everything is both q/Q balanced, *and*
    //     add a starting q and ending Q to all Content streams!  Otherwise our overlay might be
    //     affected by stray scaling and rotations!
    debug!("Modifying existing Content streams");
    modify_content_stream(dest_doc, &dest_contents_arr_new, None)?;
    // c. We then copy the references from the overlay_contents_arr_new to the end of
    //    dest_content_arr_new.
    for item in overlay_contents_arr_new.iter() {
        let reference = item.as_reference()?;
        dest_contents_arr_new.push(Object::Reference(reference)); // For "underlay": .insert(i, Object::Reference(reference)); where i starts at 0 and increments for each content stream from the underlay.
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
    // c. Finally, we replace the dest page's Content value with our new array.
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(KEY_CONTENTS, Object::Array(dest_contents_arr_new));

    // On target output page, merge our renamed /Resources with the target page's /Resources dicts
    debug!("Merge overlay's /Resources dictionary (with keys renamed) into destination page's /Resources");
    // a. First, we must ensure the dest page's Resources exists, and normalize it to be a reference
    //    to a separate object.
    //    i. We first determine which scenario we're in: embedded Dictionary or Reference to dict obj:
    let (dict_to_make_object, dict_ref) = match dest_page_dict.get(KEY_RESOURCES) {
        Ok(Object::Dictionary(dict)) => (Some(dict.clone()), None),
        Ok(Object::Reference(reference)) => (None, Some(*reference)),
        Ok(_) => {
            return Err(PdfMergeError::new(
                "Destination page's /Resource was not a Dictionary nor Reference",
            ))
        }
        Err(_) => (Some(Dictionary::new()), None),
    };
    //    ii. Now we add a new Dictionary object to dest_doc if needed, or use the one that's already there!
    let dict_ref = match (dict_to_make_object, dict_ref) {
        (Some(dict_to_make_object), None) => {
            dest_doc.add_object(Object::Dictionary(dict_to_make_object))
        }
        (None, Some(dict_ref)) => dict_ref,
        _ => {
            return Err(PdfMergeError::new(
                "Internal error: unexpected state in resources normalization",
            ))
        }
    };
    //    iii. Finally, we update dest page's /Resources to be dict_ref
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(KEY_RESOURCES, Object::Reference(dict_ref));
    // b. Now, we can modify the object pointed to by dict_ref to contain the needed newly renamed
    //    entries from the overlay page's Resources.
    //    i. As a side-effect of deep copying the overlay's Resources, we have added
    //       overlay_resources_dict_id_new, which we won't need in the final document.
    //       We also need to get this "outside" the dest_doc to avoid borrow checker
    //       issues, so we remove this object now, keeping a copy of the underlying
    //       Dictionary, though.
    let source_resources_dict = dest_doc
        .get_object(overlay_resources_dict_id_new)?
        .as_dict()?
        .clone();
    dest_doc.objects.remove(&overlay_resources_dict_id_new);
    //    ii. Now merge the source_resources_dict into the dict_ref
    let dest_resources = dest_doc.get_object_mut(dict_ref)?.as_dict_mut()?;
    // At the root level /Resources dict, each entry is (usually) a dictionary (e.g. /Font, or /XObject)
    // that actually contain the key->value mappings.  We skip /Resources kinds that are not dictionaries
    // (only /ProcSet; see page 83 of https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)
    for (resource_type, dict) in source_resources_dict.iter() {
        if dest_resources.get(resource_type).is_err() {
            // Key does not exist, so just add entire resource type in one go:
            dest_resources.set(resource_type.clone(), dict.clone());
        } else {
            // Only handle values that are actually dictionaries... there is one /Resource that can be
            // an Array, but we'll skip merging those for now.  (FUTURE!)
            if let Ok(dict) = dict.as_dict() {
                let dest_resource = dest_resources.get_mut(resource_type)?.as_dict_mut()?;
                for (key, value) in dict.iter() {
                    dest_resource.set(key.clone(), value.clone());
                }
            }
        }
    }

    Ok(())
}
