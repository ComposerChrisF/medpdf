use lopdf::{Document, Object, ObjectId};
use std::collections::{BTreeMap};
use crate::{error::{PdfMergeError, Result}, pdf_helpers::{self, KEY_COUNT, KEY_KIDS, KEY_PAGES, KEY_PARENT}};


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
        .catalog_mut()?
        .get_mut(KEY_PAGES)
        .map_err(|_| PdfMergeError::new("Pages object not found in destination document"))?
        .as_reference()
        .map_err(|_| PdfMergeError::new("Pages object not a reference"))?;
    let dest_pages = dest_doc
        .get_object_mut(dest_pages_id)?
        .as_dict_mut()
        .map_err(|e| PdfMergeError::new(format!("Pages object is not a dictionary. e={e:?}")))?;

    let new_page_count = {
        let dest_kids = dest_pages
            .get_mut(KEY_KIDS)
            .map_err(|_| PdfMergeError::new("Kids array not found in Pages dictionary"))?
            .as_array_mut()
            .map_err(|_| PdfMergeError::new("Kids object is not an array"))?;
        dest_kids.push(Object::Reference(new_page_id));
        dest_kids.len()
    };
    dest_pages.set(KEY_COUNT.to_vec(), Object::Integer(new_page_count as i64));

    Ok(new_page_id)
}
