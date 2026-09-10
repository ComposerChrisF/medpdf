//! Repro for medpdf bug-0040: `copy_page_with_cache` called twice for the same
//! source page returns the SAME `ObjectId`, and appends it to the destination
//! `/Kids` twice while incrementing `/Count` twice — a page tree that lists one
//! page object in two slots.
//!
//! Run by copying this file to `medpdf/tests/bug_0040_repro.rs` and running
//! `cargo test -p medpdf --test bug_0040_repro -- --nocapture`.
//!
//! Contrast `pdf_operations_tests.rs::test_copied_pages_are_independent`, which
//! already pins the uncached `copy_page` doing this correctly.

mod fixtures;

use medpdf::pdf_copy_page::copy_page_with_cache;
use std::collections::BTreeMap;

#[test]
fn copying_the_same_page_twice_yields_two_distinct_pages() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let first = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let second = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    println!("first  = {first:?}");
    println!("second = {second:?}");
    println!("get_pages().len() = {}", dest_doc.get_pages().len());

    assert_ne!(
        first, second,
        "each copy must be its own page object; the same id twice in /Kids is malformed"
    );
    assert_eq!(dest_doc.get_pages().len(), 2, "two copies must be two pages");
}

/// The concrete harm, independent of any spec argument: because the two "copies"
/// are one object, a per-page edit to the second is an edit to the first. This is
/// what a consumer's per-page loop (a watermark, a rotation, a stamp) does.
#[test]
fn a_per_page_edit_to_the_second_copy_does_not_touch_the_first() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let first = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let second = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    // Rotate only the second copy.
    dest_doc
        .get_object_mut(second)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Rotate", lopdf::Object::Integer(90));

    let first_rotate = dest_doc
        .get_dictionary(first)
        .unwrap()
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok());
    println!("after rotating only the second copy, first page /Rotate = {first_rotate:?}");

    assert_eq!(
        first_rotate, None,
        "editing the second copy must not rotate the first — they are supposed to be two pages"
    );
}
