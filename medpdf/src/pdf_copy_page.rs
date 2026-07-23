//! Page copying between PDF documents with full object graph duplication.

use crate::{
    error::Result,
    pdf_helpers::{
        self, KEY_CROP_BOX, KEY_KIDS, KEY_MEDIA_BOX, KEY_PAGES, KEY_PARENT, KEY_RESOURCES,
        KEY_ROTATE,
    },
};
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;

/// Copies a page from a source document to the destination document.
/// It also copies all referenced objects, such as fonts and images.
///
/// Note: Each call creates its own reference tracking map, so shared resources
/// are duplicated when copying multiple pages. Use `copy_page_with_cache` to
/// share a cache across multiple calls and avoid duplicating shared resources.
pub fn copy_page(
    dest_doc: &mut Document,
    source_doc: &Document,
    page_num: u32,
) -> Result<ObjectId> {
    let mut copied_objects = BTreeMap::new();
    copy_page_with_cache(dest_doc, source_doc, page_num, &mut copied_objects)
}

/// Copies a page from a source document to the destination document,
/// using a shared cache to avoid duplicating resources.
///
/// The `copied_objects` map tracks which source objects have already been
/// copied and their corresponding destination IDs. Pass the same map to
/// multiple calls when copying pages from the same source document to
/// deduplicate shared resources like fonts and images.
///
/// # Example
/// ```ignore
/// let mut cache = BTreeMap::new();
/// for page_num in 1..=10 {
///     copy_page_with_cache(&mut dest_doc, &source_doc, page_num, &mut cache)?;
/// }
/// ```
pub fn copy_page_with_cache(
    dest_doc: &mut Document,
    source_doc: &Document,
    page_num: u32,
    copied_objects: &mut BTreeMap<ObjectId, ObjectId>,
) -> Result<ObjectId> {
    let source_page_id = pdf_helpers::get_page_object_id_from_doc(source_doc, page_num)?;
    let dest_pages_id = dest_doc.catalog()?.get(KEY_PAGES)?.as_reference()?;

    let new_page_id =
        pdf_helpers::deep_copy_object_by_id(dest_doc, source_doc, source_page_id, copied_objects)?;

    // Materialize inherited page attributes onto the copied page. `Resources`,
    // `MediaBox`, `CropBox`, and `Rotate` are inheritable and may live only on a
    // source `/Pages` ancestor; deep_copy skips `/Parent`, so they do not come
    // along. Without this the copied page can lose its size, its fonts, and its
    // rotation, silently (bug-0008). Flatten the effective value onto the leaf page
    // so it renders identically under its new parent — deep-copying reference
    // values through the shared `copied_objects` map.
    for &key in &[KEY_RESOURCES, KEY_MEDIA_BOX, KEY_CROP_BOX, KEY_ROTATE] {
        // Skip attributes the copied page already carries as its own.
        if dest_doc.get_dictionary(new_page_id)?.get(key).is_ok() {
            continue;
        }
        if let Some(inherited) =
            pdf_helpers::resolve_inherited_attribute(source_doc, source_page_id, key)
        {
            let value = match inherited {
                Object::Reference(id) => Object::Reference(pdf_helpers::deep_copy_object_by_id(
                    dest_doc,
                    source_doc,
                    id,
                    copied_objects,
                )?),
                other => {
                    pdf_helpers::deep_copy_object(dest_doc, source_doc, &other, copied_objects)?
                }
            };
            dest_doc
                .get_object_mut(new_page_id)?
                .as_dict_mut()?
                .set(key.to_vec(), value);
        }
    }

    let page = dest_doc.get_object_mut(new_page_id)?.as_dict_mut()?;
    page.set(KEY_PARENT, Object::Reference(dest_pages_id));

    // Append the new leaf to the destination /Pages node's /Kids.
    {
        let dest_kids = dest_doc
            .get_object_mut(dest_pages_id)?
            .as_dict_mut()?
            .get_mut(KEY_KIDS)?
            .as_array_mut()?;
        dest_kids.push(Object::Reference(new_page_id));
    }

    // The appended kid is always a leaf Page, so every ancestor's leaf count grows
    // by exactly 1. Increment along the /Parent chain rather than assigning
    // kids.len(), which counts children (not leaves) and is wrong under any
    // intermediate /Pages node (bug-0020). The new page attaches to the root Pages
    // node today, so the walk is usually just that node — but writing it as a walk
    // keeps it correct if a future nested attach point is used.
    pdf_helpers::adjust_ancestor_counts(dest_doc, dest_pages_id, 1)?;

    Ok(new_page_id)
}
