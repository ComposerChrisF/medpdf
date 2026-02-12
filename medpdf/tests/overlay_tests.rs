// tests/overlay_tests.rs
// Tests for pdf_overlay module

mod fixtures;

use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_overlay::overlay_page;

// --- Basic Overlay Tests ---

#[test]
fn test_overlay_basic() {
    // Create a dest doc with one page
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Create overlay doc with one page
    let overlay_doc = fixtures::create_pdf_with_pages(1);

    // Apply overlay
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay should succeed: {:?}", result.err());
}

#[test]
fn test_overlay_adds_content_streams() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);

    // Before overlay - count content streams
    let page_before = dest_doc.get_dictionary(dest_page_id).unwrap();
    let contents_before = page_before.get(b"Contents").unwrap();
    let count_before = match contents_before {
        lopdf::Object::Array(arr) => arr.len(),
        lopdf::Object::Reference(_) => 1,
        _ => 0,
    };

    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();

    // After overlay - should have more content streams
    let page_after = dest_doc.get_dictionary(dest_page_id).unwrap();
    let contents_after = page_after.get(b"Contents").unwrap();
    let count_after = match contents_after {
        lopdf::Object::Array(arr) => arr.len(),
        lopdf::Object::Reference(_) => 1,
        _ => 0,
    };

    assert!(
        count_after > count_before,
        "After overlay, content count ({}) should be greater than before ({})",
        count_after,
        count_before
    );
}

#[test]
fn test_overlay_invalid_page() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);

    // Try to overlay from page 2 which doesn't exist
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 2);
    assert!(result.is_err());
}

#[test]
fn test_overlay_page_zero() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);

    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 0);
    assert!(result.is_err());
}

// --- q/Q Balancing Tests ---

#[test]
fn test_overlay_q_balancing_works() {
    // Test that unbalanced q operators from an overlay are corrected.
    // The code adds missing Q operators when count_q > 0.

    // Create base document
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Create overlay with 2 extra 'q' operators (no matching Q's)
    let overlay_doc = fixtures::create_pdf_with_unbalanced_q(2);

    // Apply overlay - this should add Q's to balance
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();

    // Get combined content and count q/Q operators
    let content = fixtures::get_page_content_bytes(&dest_doc, dest_page_id);
    let (q_count, big_q_count) = fixtures::count_q_operators(&content);

    println!("q count: {}, Q count: {}", q_count, big_q_count);

    // With the bug fixed, the balancing code adds extra Q operators
    // The exact counts depend on wrapping, but Q should be >= q
    assert!(
        big_q_count >= q_count,
        "Q count ({}) should be >= q count ({}) after balancing",
        big_q_count,
        q_count
    );
}

#[test]
fn test_overlay_balanced_q_operators() {
    // Test with properly balanced q/Q in overlay - should work correctly

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Create overlay with balanced q/Q (0 extra)
    let overlay_doc = fixtures::create_pdf_with_unbalanced_q(0);

    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok());
}

// --- Resource Renaming Tests ---

#[test]
fn test_overlay_resources_merged() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);

    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();

    // Verify page still has Resources
    let page = dest_doc.get_dictionary(dest_page_id).unwrap();
    let resources = page.get(b"Resources");
    assert!(
        resources.is_ok(),
        "Page should have Resources after overlay"
    );
}

#[test]
fn test_multiple_overlays_on_same_page() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);

    // Apply multiple overlays to same page
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();

    // Should still have valid page structure
    let page = dest_doc.get_dictionary(dest_page_id).unwrap();
    assert!(page.get(b"Contents").is_ok());
    assert!(page.get(b"Resources").is_ok());
}

#[test]
fn test_overlay_different_overlay_pages() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Create overlay with multiple pages
    let overlay_doc = fixtures::create_pdf_with_pages(3);

    // Overlay from different source pages
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 2).unwrap();
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 3).unwrap();

    // Destination should still have one page with merged content
    assert_eq!(dest_doc.get_pages().len(), 1);
}
