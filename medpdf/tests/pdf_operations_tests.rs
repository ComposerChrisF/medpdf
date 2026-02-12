// tests/pdf_operations_tests.rs
// Tests for pdf_copy_page and pdf_blank_page modules

mod fixtures;

use lopdf::Object;
use medpdf::pdf_blank_page::create_blank_page;
use medpdf::pdf_copy_page::{copy_page, copy_page_with_cache};
use std::collections::BTreeMap;

// --- create_blank_page Tests ---

#[test]
fn test_blank_page_created() {
    let mut doc = fixtures::create_empty_pdf();
    let initial_page_count = doc.get_pages().len();

    let result = create_blank_page(&mut doc, 612.0, 792.0);
    assert!(result.is_ok());

    let final_page_count = doc.get_pages().len();
    assert_eq!(final_page_count, initial_page_count + 1);
}

#[test]
fn test_blank_page_dimensions() {
    let mut doc = fixtures::create_empty_pdf();
    let width = 400.0;
    let height = 600.0;

    let page_id = create_blank_page(&mut doc, width, height).unwrap();

    // Verify MediaBox
    let page = doc.get_dictionary(page_id).unwrap();
    let media_box = page.get(b"MediaBox").unwrap().as_array().unwrap();

    assert_eq!(media_box.len(), 4);
    assert_eq!(media_box[0].as_f32().unwrap(), 0.0);
    assert_eq!(media_box[1].as_f32().unwrap(), 0.0);
    assert_eq!(media_box[2].as_f32().unwrap(), width);
    assert_eq!(media_box[3].as_f32().unwrap(), height);
}

#[test]
fn test_blank_page_us_letter() {
    let mut doc = fixtures::create_empty_pdf();

    let page_id = create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    let page = doc.get_dictionary(page_id).unwrap();
    let media_box = page.get(b"MediaBox").unwrap().as_array().unwrap();

    assert_eq!(media_box[2].as_f32().unwrap(), 612.0);
    assert_eq!(media_box[3].as_f32().unwrap(), 792.0);
}

#[test]
fn test_blank_page_a4_approximate() {
    let mut doc = fixtures::create_empty_pdf();
    // A4 dimensions in points (approximately)
    let width = 595.0;
    let height = 842.0;

    let page_id = create_blank_page(&mut doc, width, height).unwrap();

    let page = doc.get_dictionary(page_id).unwrap();
    let media_box = page.get(b"MediaBox").unwrap().as_array().unwrap();

    assert!((media_box[2].as_f32().unwrap() - width).abs() < 1.0);
    assert!((media_box[3].as_f32().unwrap() - height).abs() < 1.0);
}

#[test]
fn test_blank_page_has_resources() {
    let mut doc = fixtures::create_empty_pdf();
    let page_id = create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    let page = doc.get_dictionary(page_id).unwrap();
    let resources = page.get(b"Resources");

    assert!(resources.is_ok(), "Blank page should have Resources");
}

#[test]
fn test_blank_page_has_contents() {
    let mut doc = fixtures::create_empty_pdf();
    let page_id = create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    let page = doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents");

    assert!(contents.is_ok(), "Blank page should have Contents");
}

#[test]
fn test_blank_page_has_parent() {
    let mut doc = fixtures::create_empty_pdf();
    let page_id = create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    let page = doc.get_dictionary(page_id).unwrap();
    let parent = page.get(b"Parent");

    assert!(parent.is_ok(), "Page should have Parent reference");
    assert!(
        parent.unwrap().as_reference().is_ok(),
        "Parent should be a reference"
    );
}

#[test]
fn test_blank_page_empty_content_stream() {
    let mut doc = fixtures::create_empty_pdf();
    let page_id = create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    let page = doc.get_dictionary(page_id).unwrap();
    let contents_ref = page.get(b"Contents").unwrap().as_reference().unwrap();
    let contents = doc.get_object(contents_ref).unwrap().as_stream().unwrap();

    // Content stream should be empty
    assert!(
        contents.content.is_empty(),
        "Blank page content stream should be empty"
    );
}

#[test]
fn test_multiple_blank_pages() {
    let mut doc = fixtures::create_empty_pdf();

    let page1 = create_blank_page(&mut doc, 612.0, 792.0).unwrap();
    let page2 = create_blank_page(&mut doc, 612.0, 792.0).unwrap();
    let page3 = create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    assert_eq!(doc.get_pages().len(), 3);
    assert_ne!(page1, page2);
    assert_ne!(page2, page3);
}

#[test]
fn test_blank_page_different_sizes() {
    let mut doc = fixtures::create_empty_pdf();

    create_blank_page(&mut doc, 100.0, 200.0).unwrap();
    create_blank_page(&mut doc, 300.0, 400.0).unwrap();
    create_blank_page(&mut doc, 500.0, 600.0).unwrap();

    assert_eq!(doc.get_pages().len(), 3);
}

// --- copy_page Tests ---

#[test]
fn test_copy_page_single() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();

    let result = copy_page(&mut dest_doc, &source_doc, 1);
    assert!(result.is_ok());

    assert_eq!(dest_doc.get_pages().len(), 1);
}

#[test]
fn test_copy_page_preserves_dimensions() {
    let source_doc = fixtures::create_pdf_with_pages_and_size(1, 400.0, 500.0);
    let mut dest_doc = fixtures::create_empty_pdf();

    let new_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let page = dest_doc.get_dictionary(new_page_id).unwrap();
    let media_box = page.get(b"MediaBox").unwrap().as_array().unwrap();

    assert_eq!(media_box[2].as_f32().unwrap(), 400.0);
    assert_eq!(media_box[3].as_f32().unwrap(), 500.0);
}

#[test]
fn test_copy_page_multiple_pages_in_order() {
    let source_doc = fixtures::create_pdf_with_pages(3);
    let mut dest_doc = fixtures::create_empty_pdf();

    copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    copy_page(&mut dest_doc, &source_doc, 2).unwrap();
    copy_page(&mut dest_doc, &source_doc, 3).unwrap();

    assert_eq!(dest_doc.get_pages().len(), 3);
}

#[test]
fn test_copy_page_out_of_order() {
    let source_doc = fixtures::create_pdf_with_pages(3);
    let mut dest_doc = fixtures::create_empty_pdf();

    copy_page(&mut dest_doc, &source_doc, 3).unwrap();
    copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    copy_page(&mut dest_doc, &source_doc, 2).unwrap();

    assert_eq!(dest_doc.get_pages().len(), 3);
}

#[test]
fn test_copy_page_duplicate_pages() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();

    copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Should have 3 copies of page 1
    assert_eq!(dest_doc.get_pages().len(), 3);
}

#[test]
fn test_copy_page_invalid_page_number() {
    let source_doc = fixtures::create_pdf_with_pages(3);
    let mut dest_doc = fixtures::create_empty_pdf();

    let result = copy_page(&mut dest_doc, &source_doc, 4);
    assert!(result.is_err());
}

#[test]
fn test_copy_page_zero() {
    let source_doc = fixtures::create_pdf_with_pages(3);
    let mut dest_doc = fixtures::create_empty_pdf();

    let result = copy_page(&mut dest_doc, &source_doc, 0);
    assert!(result.is_err());
}

#[test]
fn test_copy_page_from_empty_doc() {
    let source_doc = fixtures::create_empty_pdf();
    let mut dest_doc = fixtures::create_empty_pdf();

    let result = copy_page(&mut dest_doc, &source_doc, 1);
    assert!(result.is_err());
}

#[test]
fn test_copy_page_has_parent_in_dest() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();

    let new_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let page = dest_doc.get_dictionary(new_page_id).unwrap();
    let parent = page.get(b"Parent").unwrap();

    assert!(parent.as_reference().is_ok());

    // Parent should point to dest_doc's Pages object
    let parent_id = parent.as_reference().unwrap();
    let parent_dict = dest_doc.get_dictionary(parent_id).unwrap();
    assert_eq!(
        parent_dict.get(b"Type").unwrap().as_name().unwrap(),
        b"Pages"
    );
}

#[test]
fn test_copy_page_updates_page_count() {
    let source_doc = fixtures::create_pdf_with_pages(5);
    let mut dest_doc = fixtures::create_empty_pdf();

    copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    copy_page(&mut dest_doc, &source_doc, 2).unwrap();

    let pages_id = dest_doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let pages = dest_doc.get_dictionary(pages_id).unwrap();
    let count = pages.get(b"Count").unwrap();

    assert_eq!(count, &Object::Integer(2));
}

#[test]
fn test_copied_pages_are_independent() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();

    let page_id1 = copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    let page_id2 = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // The two copied pages should have different ObjectIds
    assert_ne!(page_id1, page_id2);
}

// --- copy_page_with_cache Tests ---

#[test]
fn test_copy_page_with_cache_deduplicates_shared_resources() {
    let source_doc = fixtures::create_pdf_with_shared_font(3);

    // Copy pages WITHOUT cache (using copy_page) - resources will be duplicated
    let mut dest_without_cache = fixtures::create_empty_pdf();
    let initial_objects_without_cache = dest_without_cache.objects.len();
    for page_num in 1..=3 {
        copy_page(&mut dest_without_cache, &source_doc, page_num).unwrap();
    }
    let objects_added_without_cache =
        dest_without_cache.objects.len() - initial_objects_without_cache;

    // Copy pages WITH cache - shared resources should be deduplicated
    let mut dest_with_cache = fixtures::create_empty_pdf();
    let initial_objects_with_cache = dest_with_cache.objects.len();
    let mut cache = BTreeMap::new();
    for page_num in 1..=3 {
        copy_page_with_cache(&mut dest_with_cache, &source_doc, page_num, &mut cache).unwrap();
    }
    let objects_added_with_cache = dest_with_cache.objects.len() - initial_objects_with_cache;

    // Both should have the same number of pages
    assert_eq!(dest_without_cache.get_pages().len(), 3);
    assert_eq!(dest_with_cache.get_pages().len(), 3);

    // With cache should have fewer objects due to deduplication of shared font/resources
    assert!(
        objects_added_with_cache < objects_added_without_cache,
        "With cache: {} objects, without cache: {} objects. Cache should result in fewer objects.",
        objects_added_with_cache,
        objects_added_without_cache
    );
}

#[test]
fn test_copy_page_with_cache_basic() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    let page1_id = copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();
    let page2_id = copy_page_with_cache(&mut dest_doc, &source_doc, 2, &mut cache).unwrap();

    assert_eq!(dest_doc.get_pages().len(), 2);
    assert_ne!(page1_id, page2_id);
}

#[test]
fn test_copy_page_with_cache_tracks_objects() {
    let source_doc = fixtures::create_pdf_with_shared_font(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut cache = BTreeMap::new();

    // Cache should be empty initially
    assert!(cache.is_empty());

    copy_page_with_cache(&mut dest_doc, &source_doc, 1, &mut cache).unwrap();

    // Cache should now contain mappings for copied objects
    assert!(
        !cache.is_empty(),
        "Cache should contain object mappings after first copy"
    );

    let cache_size_after_first = cache.len();
    copy_page_with_cache(&mut dest_doc, &source_doc, 2, &mut cache).unwrap();

    // Cache may grow for page-specific objects, but shared objects shouldn't be re-added
    // (the cache prevents re-copying the same source object)
    assert!(
        cache.len() >= cache_size_after_first,
        "Cache should retain mappings from previous copies"
    );
}
