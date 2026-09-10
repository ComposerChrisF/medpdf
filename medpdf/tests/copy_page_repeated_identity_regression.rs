//! Regression tests for bug-0040: `copy_page_with_cache` must deduplicate
//! *resources*, never *pages*.
//!
//! The shared `copied_objects` cache is keyed on source `ObjectId`, and the page
//! object was looked up through it like any other object — so a second call for
//! the same source page returned the first copy's id, appended it to `/Kids`
//! again, and incremented `/Count` again. Two slots, one object. Because they
//! were one object, a per-page edit to "the second page" also edited the first.
//!
//! Reachable from an ordinary page spec once `parse_page_spec` honors duplicates
//! (plan-0006): both consumers feed each element of the expanded list straight to
//! this function.
//!
//! The first two tests are the repro filed with the report (`bugs/bug-0040/`);
//! the rest pin the properties the fix must not break.

mod fixtures;

use lopdf::{Document, Object, Stream, dictionary};
use medpdf::pdf_copy_page::copy_page_with_cache;
use std::collections::BTreeMap;

/// The identity property: one call, one page.
#[test]
fn copying_the_same_page_twice_yields_two_distinct_pages() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let first = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let second = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    assert_ne!(
        first, second,
        "each copy must be its own page object; the same id twice in /Kids is malformed"
    );
    assert_eq!(
        dest_doc.get_pages().len(),
        2,
        "two copies must be two pages"
    );
}

/// The concrete harm, independent of any spec argument: because the two "copies"
/// were one object, a per-page edit to the second was an edit to the first. This
/// is exactly what a consumer's per-page loop does (a watermark, a stamp, a
/// rotation), and it is the assertion that states the property consumers depend
/// on — the page tree already *reported* two pages with the bug fully present, so
/// a test that only counted pages passed.
#[test]
fn a_per_page_edit_to_the_second_copy_does_not_touch_the_first() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let first = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let second = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    dest_doc
        .get_object_mut(second)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Rotate", Object::Integer(90));

    let first_rotate = dest_doc
        .get_dictionary(first)
        .unwrap()
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok());

    assert_eq!(
        first_rotate, None,
        "editing the second copy must not rotate the first — they are two pages"
    );
}

/// The page tree itself must be well formed: two distinct references in `/Kids`,
/// and a `/Count` that matches.
#[test]
fn repeated_copies_produce_two_distinct_kids_and_a_matching_count() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    let pages_id = dest_doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let pages = dest_doc.get_dictionary(pages_id).unwrap();

    let kids: Vec<_> = pages
        .get(b"Kids")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_reference().unwrap())
        .collect();

    assert_eq!(kids.len(), 2, "two copies must be two kids");
    assert_ne!(
        kids[0], kids[1],
        "/Kids must not list the same page object twice"
    );
    assert_eq!(
        pages.get(b"Count").unwrap().as_i64().unwrap(),
        2,
        "/Count must match the number of leaves"
    );
}

/// The property the cache exists for, and the one the fix must not trade away:
/// the two copies of a repeated page share their `/Contents` and `/Resources`.
/// Only the page node's identity is fresh.
#[test]
fn repeated_copies_still_share_their_resources_and_contents() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let first = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let second = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    for key in [b"Contents".as_slice(), b"Resources".as_slice()] {
        let a = dest_doc
            .get_dictionary(first)
            .unwrap()
            .get(key)
            .unwrap()
            .as_reference()
            .unwrap();
        let b = dest_doc
            .get_dictionary(second)
            .unwrap()
            .get(key)
            .unwrap()
            .as_reference()
            .unwrap();
        assert_eq!(
            a,
            b,
            "repeated copies must share /{} — deduplicating resources is what the cache is for",
            String::from_utf8_lossy(key)
        );
    }
}

/// The pre-existing behavior for *distinct* pages must be untouched: a resource
/// object shared by two source pages is still copied exactly once. This is the
/// guard against "fixing" the page-identity bug by disabling the cache.
#[test]
fn distinct_pages_still_deduplicate_a_shared_resource() {
    // Two pages that share one /Resources object.
    let mut source_doc = fixtures::create_empty_pdf();
    let pages_id = source_doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let shared_resources_id = source_doc.add_object(dictionary! {});

    for _ in 0..2 {
        let content_id = source_doc.add_object(Stream::new(dictionary! {}, vec![]));
        let page_id = source_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(shared_resources_id),
        });
        let pages = source_doc
            .get_object_mut(pages_id)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
        kids.push(page_id.into());
        let count = kids.len() as i64;
        pages.set("Count", Object::Integer(count));
    }

    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let first = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let second = copy_page_with_cache(&mut dest_doc, &source_doc, 2, &mut cache).unwrap();

    assert_ne!(first, second, "distinct pages are distinct objects");

    let resources_of = |doc: &Document, id| {
        doc.get_dictionary(id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_reference()
            .unwrap()
    };
    assert_eq!(
        resources_of(&dest_doc, first),
        resources_of(&dest_doc, second),
        "a /Resources object shared by two source pages must still be copied once"
    );
}
