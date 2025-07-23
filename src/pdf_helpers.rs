// src/pdf_helpers.rs

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use std::collections::{BTreeMap, HashMap, HashSet};
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

fn add_resource_keys(
    keys: &mut HashSet<Vec<u8>>, 
    dict_resources: &Dictionary,
) -> Result<()> {
    for (_, value) in dict_resources.iter() {
        if let Object::Dictionary(dict) = value {
            for (key, _) in dict.iter() {
                keys.insert(key.clone());
            }
        }
    }
    Ok(())
}

fn accumulate_dictionary_keys(
    keys: &mut HashSet<Vec<u8>>, 
    doc: &Document, 
    start: ObjectId
) -> Result<()> {
    let o = doc.get_object(start)?;
    if let Object::Dictionary(dict) = o {
        match dict.get(b"Type") {
            Ok(Object::Name(v)) => {
                if v == b"Pages" || v == b"Page" {
                    match dict.get(b"Resources") {
                        Ok(Object::Dictionary(dict_resources)) => { add_resource_keys(keys, dict_resources)?; }
                        Ok(Object::Reference(id_resources)) => {
                            if let Ok(dict_resources) = doc.get_dictionary(*id_resources) {
                                add_resource_keys(keys, dict_resources)?;
                            }
                        }
                        _ => { return Ok(()); } // Nothing to bother with
                    }
                }
            }
            _ => ()
        }
    }
    Ok(())
}

fn find_unique_name(
    keys_used: &HashSet<Vec<u8>>,
    key_old: &Vec<u8>,
    suffix: &Vec<u8>,
) -> Result<Vec<u8>> {
    let mut buffer = Vec::<u8>::with_capacity(16);
    for b in key_old.iter() { buffer.push(*b); }
    for b in suffix.iter() { buffer.push(*b); }
    let start_len = buffer.len();
    for i in 0..10_000 {
        if i > 0 {
            buffer.truncate(start_len);
            for b in format!("{i}").as_bytes().iter() { buffer.push(*b) }
        }
        if !keys_used.contains(&buffer) { return Ok(buffer); }
    }
    Err(PdfMergeError::new("No new unique key could be generated"))
}


fn rename_resources_in_dict(
    key_mapping: &mut HashMap<Vec<u8>, Vec<u8>>,
    keys_used: &mut HashSet<Vec<u8>>,
    dest_doc: &mut Document,
    resources_dict_id_new: ObjectId
) -> Result<()> {
    let dict = dest_doc.get_dictionary_mut(resources_dict_id_new)?;
    let new_key_suffix = vec![b'_', b'o'];
    for (_, value) in dict.iter_mut() {
        // The unused "key" here is /Font, /XObject, etc.  We don't need to know what key it is
        // When value is a dictionary, it contains key->value pairs for resources.  We can ignore non-dictionary values
        if let Object::Dictionary(dict) = value {
            let list_of_keys = dict.iter().map(|(k,_v)| k.clone()).collect::<Vec<Vec<u8>>>();
            for key in list_of_keys {
                // If we've already mapped this key, skip regenerating new key as we need to
                // preserve the mapping from old keys (that may be shadowing across multiple
                // dictionaries from the source overlay document).
                if key_mapping.contains_key(&key) { continue; }

                // These key/value pairs are a resource_name/resource_value pair.  We need to rename the name.
                let key_new = find_unique_name(&keys_used, &key, &new_key_suffix)?;
                key_mapping.insert(key.clone(), key_new.clone());
                match dict.remove(&key) {
                    Some(v) => dict.set(key_new, v),
                    None => {}
                }
                
                // NO!: keys_used.insert(key);  (See note above of abour preserving overlapping keys from source document.)
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn debug_dump_stream_by_id(doc: &Document, id_stream: ObjectId) -> Result<()> {
    #[cfg(debug_assertions)] {
        debug_dump_stream_object(doc.get_object(id_stream)?)?;
    }
    Ok(())
}
#[allow(dead_code)]
fn debug_dump_stream_object(stream: &Object) -> Result<()> {
    #[cfg(debug_assertions)] {
        let s = stream.as_stream()?;
        debug_dump_stream(&s)?;
    }
    Ok(())
}
#[allow(dead_code)]
fn debug_dump_stream(stream: &Stream) -> Result<()> {
    #[cfg(debug_assertions)] {
        print!("Dumping stream: ");
        let ops = stream.decode_content()?.operations;
        println!("    # ops = {}", ops.len());
        for (i, op) in ops.iter().enumerate() {
            println!("  op: {op:?}");
            if i > 20 { break; }
        }
        println!("    Raw dump:");
        println!("{}\n", String::from_utf8_lossy(&stream.content[..100]));
        if stream.is_compressed() {
            let x = stream.decompressed_content()?;
            println!("{}\n", String::from_utf8_lossy(&x[..100]));
        }
    }
    Ok(())
}

fn modify_content_stream(
    dest_doc: &mut Document, 
    contents_arr: &Vec<Object>,
    key_mapping: Option<&HashMap<Vec<u8>, Vec<u8>>>
) -> Result<()> {
    for content_ref_obj in contents_arr.iter() {
        let content_stream = dest_doc.get_object_mut(content_ref_obj.as_reference()?)?.as_stream_mut()?;
        if content_stream.is_compressed() { content_stream.decompress()?; }
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
        println!("count_q = {count_q}");
        for _ in count_q..0 {
            println!("Unbalanced q/Q pairs, so adding 'Q'.");
            content.operations.push(Operation::new("Q", vec![]));
        }
        // TODO: Compress content stream!!!
        content_stream.content = content.encode()?;
        for (i, op) in content_stream.decode_content()?.operations.iter().enumerate() {
            if i > 20 { break; }
            println!("op {op:?}");
        }
    }
    Ok(())
}


/// Overlays the content of a source page onto a destination page.
pub fn overlay_page(
    dest_doc: &mut Document,
    dest_page_id: ObjectId,
    overlay_doc: &Document,
    overlay_page_num: u32,
) -> Result<()> {
    let overlay_page_id = get_page_object_id_from_doc(overlay_doc, overlay_page_num)?;

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
    println!("Standarding and cloning overlay's /Contents");
    let overlay_contents = overlay_page.get(b"Contents")?;
    let overlay_contents_arr_new = match overlay_contents {
        Object::Stream(stream) => {
            let dest_stream_id = dest_doc.add_object(stream.clone());
            vec![Object::Reference(dest_stream_id)]
        },
        Object::Reference(reference) => {
            let o = overlay_doc.get_object(*reference)?;
            match o {
                Object::Stream(stream) => {
                    let dest_stream_id = dest_doc.add_object(stream.clone());
                    vec![Object::Reference(dest_stream_id)]
                }
                Object::Array(a) => {
                    let mut a_new = Vec::<Object>::with_capacity(a.len());
                    for item in a {
                        let id_item_new = match item {
                            Object::Stream(s) => dest_doc.add_object(s.clone()),
                            Object::Reference(id) => {
                                deep_copy_object_by_id(dest_doc, overlay_doc, *id, &mut copied_objects)?
                            }
                            _ => return Err(PdfMergeError::new("Page/Contents array must contain Streams or References!")),
                        };
                        a_new.push(Object::Reference(id_item_new));
                    }
                    a_new
                }
                _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Contents references a non-stream / non-array"))),
            }
        }
        Object::Array(a) => {
            let mut a_new = Vec::<Object>::with_capacity(a.len());
            for item in a {
                let id_item_new = match item {
                    Object::Stream(s) => dest_doc.add_object(s.clone()),
                    Object::Reference(id) => {
                        deep_copy_object_by_id(dest_doc, overlay_doc, *id, &mut copied_objects)?
                    }
                    _ => return Err(PdfMergeError::new("Page/Contents array must contain Streams or References!")),
                };
                a_new.push(Object::Reference(id_item_new));
            }
            a_new
        }
        _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Contents must be stream or array or reference to stream or array"))),
    };

    // Generate deep copy of overlay's page's /Resources dictionary, normalizing it to be a 
    // reference (rather than an embedded resource).  FUTURE: Also need to copy/merge any parent /Resources...
    println!("Generating deep copy of overlay's /Resources...");
    let overlay_page_resources = overlay_page.get(b"Resources")?;
    let overlay_resources_dict_id_new = match overlay_page_resources {
        Object::Dictionary(_) => {
            let d_new = deep_copy_object(dest_doc, overlay_doc, overlay_page_resources, &mut copied_objects)?;
            dest_doc.add_object(d_new)
        }
        Object::Reference(id) => {
            deep_copy_object_by_id(dest_doc, overlay_doc, *id, &mut copied_objects)?
        }
        _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Resources must be dictionary or referece to dictionary"))),
    };
    // NOTE: As a side-effect of the deep copy of the overlay's /Resources, we've added an unnecessary
    // Object::Dicitonary() to the dest_doc... we'll remove this later to tidy up.

    // Starting at the root of the *destination* document, build a list (HashSet) of all keys in 
    // all /Resource dictionaries, so we can later make sure no names we add to /Resources conflict!
    println!("Accumulating dictionary keys in destination document");
    let mut keys_used = HashSet::<Vec<u8>>::new();
    accumulate_dictionary_keys(&mut keys_used, &*dest_doc, dest_doc.catalog()?.get(b"Pages")?.as_reference()?)?;

    // Now generate new names for all resources in our copied resources_dict_id_new, mutably updating it.
    // Make sure the new names are not present in keys.
    println!("Renaming keys in overlay dictionaries to be unique in destination");
    let mut key_mapping = HashMap::<Vec<u8>, Vec<u8>>::new();
    rename_resources_in_dict(&mut key_mapping, &mut keys_used, dest_doc, overlay_resources_dict_id_new)?;
    // We've now renamed the keys in the Resources dict from the overlay (resources_dict_id_new).
    println!("key_mapping:");
    for (k, v) in key_mapping.iter() {
        println!("{} => {}", String::from_utf8_lossy(&k), String::from_utf8_lossy(&v));
    }

    // Update the Contents streams from the overlay document to use the new dictionary keys
    // Unobvious, but changing decoded operations does not modify the Content!!!!  It looks
    // like we need to build a *new* Content, modifying the ops as we copy them.
    println!("Updating overlay Content streams to use new keys");
    assert!(overlay_contents_arr_new.iter().all(|obj| obj.as_reference().is_ok()));
    assert!(overlay_contents_arr_new.iter().all(|obj| { let o = dest_doc.get_object(obj.as_reference().unwrap()).unwrap(); o.as_stream().is_ok() }));
    modify_content_stream(dest_doc, &overlay_contents_arr_new, Some(&key_mapping))?;
    //for content_ref_obj in overlay_contents_arr_new.iter() {
    //    let content_stream = dest_doc.get_object_mut(content_ref_obj.as_reference()?)?.as_stream_mut()?;
    //    if content_stream.is_compressed() { content_stream.decompress()?; }
    //    let mut content = content_stream.decode_content()?;
    //    let mut count_q = 0_isize;
    //    for operation in content.operations.iter_mut() {
    //        match &operation.operator[..] {
    //            "q" => count_q += 1,
    //            "Q" => count_q -= 1,
    //            _ => {}
    //        }
    //        for operand in operation.operands.iter_mut() {
    //            // Process only operands that are names...
    //            if let Ok(name) = operand.as_name() {
    //                // ...that also have a mapping to a new name
    //                if let Some(name_new) = key_mapping.get(name) {
    //                    *operand = Object::Name(name_new.clone());
    //                }
    //            }
    //        }
    //    }
    //    // Add bracketing q/Q pair to contain graphics state changes
    //    content.operations.insert(0, Operation::new("q", vec![]));
    //    content.operations.push(Operation::new("Q", vec![]));
    //    // We count q/Q pairs to make sure they are balanced, so that we can add extra "Q" if necessary.
    //    println!("count_q = {count_q}");
    //    for _ in count_q..0 {
    //        println!("Unbalanced q/Q pairs, so adding 'Q'.");
    //        content.operations.push(Operation::new("Q", vec![]));
    //    }
    //    // TODO: Compress content stream!!!
    //    content_stream.content = content.encode()?;
    //    for (i, op) in content_stream.decode_content()?.operations.iter().enumerate() {
    //        if i > 20 { break; }
    //        println!("op {op:?}");
    //    }
    //}
    #[cfg(debug_assertions)] {
        println!("arr_new: {overlay_contents_arr_new:?}");
        for item in overlay_contents_arr_new.iter() {
            let o = dest_doc.get_object(item.as_reference()?)?;
            println!("o={o:?}");
            debug_dump_stream_object(o)?;
        }
    }




    // Now add each element of the Contents array to the destination pages's /Contents (normalizing
    // the destination /Contents to be an array).
    println!("Merging overlay's /Contents into the destination page's /Contents array");
    // a. We start by getting a copy of dest page's Contents, converting it to an array of 
    //    references, if necessary.
    let dest_contents = dest_doc.get_object(dest_page_id)?.as_dict()?.get(b"Contents")?;
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
                _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Contents reference must point to stream or array: {dest_contents:?}"))),
            }
        }
        _ => return Err(PdfMergeError::Message(format!("Page {overlay_page_id:?} /Contents must be stream or array or reference to one: {dest_contents:?}"))),
    };
    // b. For the original Content, we need to make sure everything is both q/Q balanced, *and*
    //     add a starting q and ending Q to all Content streams!  Otherwise our overlay might be 
    //     affected by stray scaling and rotations!
    println!("Modifying existing Content streams");
    modify_content_stream(dest_doc, &dest_contents_arr_new, None)?;
    // c. We then copy the references from the overlay_contents_arr_new to the end of 
    //    dest_content_arr_new.
    println!("overlay_contents_arr_new: {overlay_contents_arr_new:?}");
    print!("  [0]: {:?} => ", overlay_contents_arr_new[0]);
    println!("{:?}", dest_doc.get_object(overlay_contents_arr_new[0].as_reference()?));
    for item in overlay_contents_arr_new.iter() {
        let reference = item.as_reference()?;
        dest_contents_arr_new.push(Object::Reference(reference));   // For "underlay": .insert(i, Object::Reference(reference)); where i starts at 0 and increments for each content stream from the underlay.
    }
    let x = dest_doc.get_object(overlay_contents_arr_new[0].as_reference()?)?;
    let y = x.as_stream()?;
    debug_dump_stream(y)?;
    println!("dest_contents_arr_new: {dest_contents_arr_new:?}");
    // c. Finally, we replace the dest page's Content value with our new array.
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(b"Contents", Object::Array(dest_contents_arr_new));

    // On target output page, merge our renamed /Resources with the target page's /Resources dicts
    println!("Merge overlay's /Resources dictionary (with keys renamed) into destination page's /Resources");
    // a. First, we must ensure the dest page's Resources exists, and normalize it to be a reference
    //    to a separate object.
    //    i. We first determine which scenario we're in: embedded Dictionary or Reference to dict obj:
    println!("a. i.");
    let (dict_to_make_object, dict_ref) = match dest_page_dict.get(b"Resources") {
        Ok(Object::Dictionary(dict)) => (Some(dict.clone()), None),
        Ok(Object::Reference(reference)) => (None, Some(*reference)),
        Ok(_) => return Err(PdfMergeError::new("Destination page's /Resource was not a Dictionary nor Reference")),
        Err(_) => (Some(Dictionary::new()), None),
    };
    assert!(dict_to_make_object.is_none() ^ dict_ref.is_none());  // Exactly one of these is Some() and one is None
    //    ii. Now we add a new Dictionary object to dest_doc if needed, or use the one that's already there!
    println!("a. ii.");
    let dict_ref = match (dict_to_make_object, dict_ref) {
        (Some(dict_to_make_object), None) => dest_doc.add_object(Object::Dictionary(dict_to_make_object)),
        (None, Some(dict_ref)) => dict_ref,
        _ => panic!("unexpected"),
    };
    //    iii. Finally, we update dest page's /Resources to be dict_ref
    println!("a. iii.");
    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(b"Resources", Object::Reference(dict_ref));
    // b. Now, we can modify the object pointed to by dict_ref to contain the needed newly renamed
    //    entries from the overlay page's Resources.
    //    i. As a side-effect of deep copying the overlay's Resources, we have added 
    //       overlay_resources_dict_id_new, which we won't need in the final document.
    //       We also need to get this "outside" the dest_doc to avoid borrow checker 
    //       issues, so we remove this object now, keeping a copy of the underlying
    //       Dictionary, though.
    println!("b. i.");
    let source_resources_dict = dest_doc.get_object(overlay_resources_dict_id_new)?.as_dict()?.clone();
    dest_doc.objects.remove(&overlay_resources_dict_id_new);
    //    ii. Now merge the source_resources_dict into the dict_ref
    println!("b. ii.");
    println!("overlay resources: {source_resources_dict:?}");
    let dest_resources = dest_doc.get_object_mut(dict_ref)?.as_dict_mut()?;
    println!("dest resources: {dest_resources:?}");
    // At the root level /Resources dict, each entry is (usually) a dictionary (e.g. /Font, or /XObject)
    // that actually contain the key->value mappings.  We skip /Resources kinds that are not dictionaries
    // (only /ProcSet; see page 83 of https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)
    for (resource_type, dict) in source_resources_dict.iter() {
        if dest_resources.get(resource_type).is_err() {
            println!("Adding full dict: /Resource/{}, dict={dict:?}", String::from_utf8_lossy(resource_type));
            // Key does not exist, so just add entire resource type in one go:
            dest_resources.set(resource_type.clone(), dict.clone());
        } else {
            println!("Mapping: /Resource/{}", String::from_utf8_lossy(resource_type));
            // Only handle values that are actually dictionaries... there is one /Resource that can be
            // an Array, but we'll skip merging those for now.  (FUTURE!)
            if let Ok(dict) = dict.as_dict() {
                let dest_resource = dest_resources.get_mut(resource_type)?.as_dict_mut()?;
                for (key, value) in dict.iter() {
                    println!("   /{} => {value:?}", String::from_utf8_lossy(key));
                    dest_resource.set(key.clone(), value.clone());
                }
            }
        }
    }
    println!("final /Resources={dest_resources:?}");

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
