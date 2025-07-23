use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use crate::error::{Result, PdfMergeError};



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
