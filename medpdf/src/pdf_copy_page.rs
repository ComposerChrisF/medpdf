use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use crate::{error::Result, pdf_helpers::{self, KEY_COUNT, KEY_KIDS, KEY_PAGES, KEY_PARENT}};


/// Copies a page from a source document to the destination document.
/// It also copies all referenced objects, such as fonts and images.
pub fn copy_page(
    dest_doc: &mut Document,
    source_doc: &Document,
    page_num: u32,
) -> Result<ObjectId> {
    let source_page_id = pdf_helpers::get_page_object_id_from_doc(source_doc, page_num)?;
    let dest_pages_id = dest_doc.catalog()?.get(KEY_PAGES)?.as_reference()?;

    let mut copied_objects = BTreeMap::new();
    let new_page_id = pdf_helpers::deep_copy_object_by_id(dest_doc, source_doc, source_page_id, &mut copied_objects)?;
    let page = dest_doc.get_object_mut(new_page_id)?.as_dict_mut()?;
    page.set(KEY_PARENT, Object::Reference(dest_pages_id));

    let dest_pages_id = dest_doc
        .catalog()?
        .get(KEY_PAGES)?
        .as_reference()?;
    let dest_pages = dest_doc
        .get_object_mut(dest_pages_id)?
        .as_dict_mut()?;

    let new_page_count = {
        let dest_kids = dest_pages
            .get_mut(KEY_KIDS)?
            .as_array_mut()?;
        dest_kids.push(Object::Reference(new_page_id));
        dest_kids.len()
    };
    dest_pages.set(KEY_COUNT.to_vec(), Object::Integer(new_page_count as i64));

    Ok(new_page_id)
}
