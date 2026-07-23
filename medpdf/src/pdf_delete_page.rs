//! Page deletion from PDF documents.

use crate::{
    error::{MedpdfError, Result},
    pdf_helpers::{self, KEY_KIDS, get_page_object_id_from_doc},
};
use lopdf::{Document, Object, ObjectId};

/// Removes a page from the document by 1-based page number.
///
/// Removes the page from its parent `/Pages` node's `/Kids` array and decrements
/// the `/Count` of that parent and of every ancestor up to the root (PDF 32000-1
/// §7.7.3.2 — `/Count` is the number of leaf pages under each node).
/// The page object and its resources are left in the document's object table
/// (lopdf does not garbage-collect unreferenced objects until save).
///
/// Returns the `ObjectId` of the removed page.
pub fn delete_page(doc: &mut Document, page_num: u32) -> Result<ObjectId> {
    let page_id = get_page_object_id_from_doc(doc, page_num)?;

    // Find the parent Pages node for this page
    let page_dict = doc.get_dictionary(page_id)?;
    let parent_id = page_dict
        .get(b"Parent")
        .map_err(|_| MedpdfError::new("Page has no /Parent reference"))?
        .as_reference()
        .map_err(|_| MedpdfError::new("Page /Parent is not a reference"))?;

    // Remove from the direct parent's /Kids array.
    {
        let parent = doc.get_object_mut(parent_id)?.as_dict_mut()?;
        let kids = parent.get_mut(KEY_KIDS)?.as_array_mut()?;

        let original_len = kids.len();
        kids.retain(|obj| {
            if let Object::Reference(id) = obj {
                *id != page_id
            } else {
                true
            }
        });

        if kids.len() == original_len {
            return Err(MedpdfError::new(format!(
                "Page {page_id:?} not found in parent's /Kids array"
            )));
        }
    }

    // Removing one leaf decrements the leaf count of the direct parent and of
    // every ancestor up to the root by exactly 1. Walk the /Parent chain rather
    // than assigning kids.len(): kids.len() counts children (not leaves) and never
    // touches ancestors above the direct parent, corrupting nested trees (bug-0020).
    pdf_helpers::adjust_ancestor_counts(doc, parent_id, -1)?;

    Ok(page_id)
}
