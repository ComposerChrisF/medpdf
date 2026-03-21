// tests/overlay_edge_tests.rs
// Edge case and stress tests for pdf_overlay module

mod fixtures;

use lopdf::{dictionary, Document, Object, Stream};
use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_overlay::overlay_page;

// --- Resource naming collision tests ---

/// Creates a PDF with a page that has many named resources under /Font,
/// designed to stress-test the find_unique_name logic.
fn create_pdf_with_named_resources(names: &[&str]) -> Document {
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let mut font_dict = lopdf::Dictionary::new();
    for name in names {
        let font_obj = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(font_obj);
        font_dict.set(name.as_bytes().to_vec(), Object::Reference(font_id));
    }

    let resources = dictionary! {
        "Font" => Object::Dictionary(font_dict),
    };
    let resources_id = doc.add_object(resources);

    let content_id = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf ET".to_vec()));
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    let pages = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    doc
}

#[test]
fn test_overlay_with_same_resource_names() {
    // Both dest and overlay have resource named "F1" - overlay should rename to avoid collision
    let source_doc = create_pdf_with_named_resources(&["F1", "F2"]);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_named_resources(&["F1", "F2"]);
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with same resource names should succeed: {:?}", result.err());
}

#[test]
fn test_overlay_with_many_resources() {
    // Many resources to test renaming with incrementing suffixes
    let names: Vec<String> = (0..50).map(|i| format!("F{i}")).collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let source_doc = create_pdf_with_named_resources(&name_refs);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_named_resources(&name_refs);
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with many resources should succeed: {:?}", result.err());
}

// --- Overlay onto multi-page documents ---

#[test]
fn test_overlay_onto_specific_page_in_multi_page() {
    // Create a 3-page document, overlay onto page 2 only
    let source_doc = fixtures::create_pdf_with_pages(3);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut page_ids = vec![];
    for p in 1..=3 {
        let pid = copy_page(&mut dest_doc, &source_doc, p).unwrap();
        page_ids.push(pid);
    }

    let overlay_doc = fixtures::create_pdf_with_pages(1);
    let result = overlay_page(&mut dest_doc, page_ids[1], &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay onto page 2 should work: {:?}", result.err());
    assert_eq!(dest_doc.get_pages().len(), 3, "Should still have 3 pages");
}

#[test]
fn test_overlay_onto_all_pages() {
    // Overlay the same overlay onto every page
    let source_doc = fixtures::create_pdf_with_pages(5);
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut page_ids = vec![];
    for p in 1..=5 {
        let pid = copy_page(&mut dest_doc, &source_doc, p).unwrap();
        page_ids.push(pid);
    }

    let overlay_doc = fixtures::create_pdf_with_pages(1);
    for pid in &page_ids {
        let result = overlay_page(&mut dest_doc, *pid, &overlay_doc, 1);
        assert!(result.is_ok(), "Overlay should succeed: {:?}", result.err());
    }
    assert_eq!(dest_doc.get_pages().len(), 5);
}

// --- Invalid destination page ---

#[test]
fn test_overlay_invalid_dest_page_id() {
    let overlay_doc = fixtures::create_pdf_with_pages(1);
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let _dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Use a bogus page ID that doesn't exist
    let bogus_id = (9999, 0);
    let result = overlay_page(&mut dest_doc, bogus_id, &overlay_doc, 1);
    assert!(result.is_err(), "Overlay with invalid dest page id should fail");
}

// --- Overlay with inline resources (embedded dict, not reference) ---

fn create_pdf_with_inline_resources() -> Document {
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let font_obj = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    };
    let font_id = doc.add_object(font_obj);

    let content_id = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf ET".to_vec()));
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    // Resources is an inline dictionary, NOT a reference
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(font_id),
            },
        },
    };
    let page_id = doc.add_object(page);

    let pages = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    doc
}

#[test]
fn test_overlay_with_inline_resources_on_dest() {
    // Destination page has inline (embedded dict) Resources instead of a reference
    let source_doc = create_pdf_with_inline_resources();
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with inline dest resources should succeed: {:?}", result.err());
}

#[test]
fn test_overlay_with_inline_resources_on_overlay() {
    // Overlay page has inline Resources
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_inline_resources();
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with inline overlay resources should succeed: {:?}", result.err());
}

// --- Content stream as inline stream (not reference) ---

fn create_pdf_with_inline_content_stream() -> Document {
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let resources_id = doc.add_object(dictionary! {});
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    let content_stream = Stream::new(dictionary! {}, b"q Q".to_vec());

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Stream(content_stream),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    let pages = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    doc
}

#[test]
fn test_overlay_from_page_with_inline_content_stream() {
    // Overlay source page has Contents as an inline Stream object (not a reference)
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_inline_content_stream();
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with inline content stream should succeed: {:?}", result.err());
}

// --- Content stream as array of references ---

fn create_pdf_with_content_array() -> Document {
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let resources_id = doc.add_object(dictionary! {});
    let content1_id = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
    let content2_id = doc.add_object(Stream::new(dictionary! {}, b"0 0 0 rg\n".to_vec()));
    let content3_id = doc.add_object(Stream::new(dictionary! {}, b"Q\n".to_vec()));
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => vec![
            Object::Reference(content1_id),
            Object::Reference(content2_id),
            Object::Reference(content3_id),
        ],
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    let pages = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    doc
}

#[test]
fn test_overlay_from_page_with_content_array() {
    // Overlay source page has Contents as an array of references to streams
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_content_array();
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with content array should succeed: {:?}", result.err());
}

#[test]
fn test_overlay_dest_with_content_array() {
    // Destination page has Contents as an array of references
    let source_doc = create_pdf_with_content_array();
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay on dest with content array should succeed: {:?}", result.err());
}

// --- Multiple overlays accumulate correctly ---

#[test]
fn test_three_overlays_accumulate_content_streams() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);

    // Apply 3 overlays
    for _ in 0..3 {
        overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();
    }

    // Verify we have an array of content streams
    let page = dest_doc.get_dictionary(dest_page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    match contents {
        Object::Array(arr) => {
            // Original + 3 overlays, each producing multiple references
            assert!(
                arr.len() >= 4,
                "Should have at least 4 content stream references, got {}",
                arr.len()
            );
        }
        _ => panic!("After multiple overlays, Contents should be an Array"),
    }
}

// --- Overlay with XObject resources ---

fn create_pdf_with_xobject_resources() -> Document {
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let xobject_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        },
        b"0 0 100 100 re f".to_vec(),
    );
    let xobject_id = doc.add_object(xobject_stream);

    let resources = dictionary! {
        "XObject" => dictionary! {
            "Im1" => Object::Reference(xobject_id),
        },
    };
    let resources_id = doc.add_object(resources);

    let content_id = doc.add_object(Stream::new(dictionary! {}, b"/Im1 Do".to_vec()));
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    let pages = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    doc
}

#[test]
fn test_overlay_with_xobject_resources() {
    // Overlay with XObject resources should be merged into dest
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_xobject_resources();
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with XObject resources should succeed: {:?}", result.err());

    // Verify XObject resources were merged into dest page
    let page = dest_doc.get_dictionary(dest_page_id).unwrap();
    let resources_ref = page.get(b"Resources").unwrap().as_reference().unwrap();
    let resources = dest_doc.get_dictionary(resources_ref).unwrap();
    let xobject = resources.get(b"XObject");
    assert!(xobject.is_ok(), "Dest should have XObject resources after overlay");
}

// --- Overlay preserves page count ---

#[test]
fn test_overlay_does_not_change_page_count() {
    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page1 = copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    let _page2 = copy_page(&mut dest_doc, &source_doc, 2).unwrap();
    assert_eq!(dest_doc.get_pages().len(), 2);

    let overlay_doc = fixtures::create_pdf_with_pages(1);
    overlay_page(&mut dest_doc, page1, &overlay_doc, 1).unwrap();

    assert_eq!(dest_doc.get_pages().len(), 2, "Overlay should not change page count");
}

// --- Overlay with unbalanced q/Q from content array ---

#[test]
fn test_overlay_q_balance_with_content_array_source() {
    // Dest page has unbalanced q in content array
    let source_doc = fixtures::create_pdf_with_unbalanced_q(3);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_pages(1);
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay should handle unbalanced q from dest: {:?}", result.err());

    // Check q/Q balance in final output
    let content = fixtures::get_page_content_bytes(&dest_doc, dest_page_id);
    let (q_count, big_q_count) = fixtures::count_q_operators(&content);
    assert!(
        big_q_count >= q_count,
        "Q count ({big_q_count}) should be >= q count ({q_count}) after balancing"
    );
}

// --- Empty overlay content ---

#[test]
fn test_overlay_with_empty_content_stream() {
    // Overlay page with empty content stream
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = fixtures::create_pdf_with_content(b"");
    let result = overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1);
    assert!(result.is_ok(), "Overlay with empty content should succeed: {:?}", result.err());
}

// --- Overlay resource _o suffix verification ---

#[test]
fn test_overlay_resource_names_use_o_suffix() {
    // Both dest and overlay have font "F1" — overlay should rename with _o suffix
    let source_doc = create_pdf_with_named_resources(&["F1"]);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let overlay_doc = create_pdf_with_named_resources(&["F1"]);
    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).unwrap();

    // Check that the destination page's font resources contain a key with _o suffix
    let page = dest_doc.get_dictionary(dest_page_id).unwrap();
    let resources_ref = page.get(b"Resources").unwrap().as_reference().unwrap();
    let resources = dest_doc.get_dictionary(resources_ref).unwrap();
    let fonts = resources.get(b"Font").unwrap().as_dict().unwrap();

    assert!(fonts.has(b"F1"), "Original F1 should remain");
    let has_o_suffix = fonts.iter().any(|(k, _)| {
        let key_str = String::from_utf8_lossy(k);
        key_str.contains("_o")
    });
    assert!(
        has_o_suffix,
        "Overlay font should be renamed with _o suffix, got keys: {:?}",
        fonts.iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).collect::<Vec<_>>()
    );
}

// --- Overlay with inherited resources from parent Pages node ---

#[test]
fn test_overlay_with_inherited_font_resources() {
    // Create a dest doc where font resources are inherited from the /Pages node
    let mut dest_doc = Document::with_version("1.7");
    let pages_id = dest_doc.new_object_id();

    // Shared font on the Pages node (inherited by all pages)
    let font_obj_id = dest_doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let content_id = dest_doc.add_object(lopdf::Stream::new(dictionary! {}, b"BT /F1 12 Tf ET".to_vec()));
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    // Page has NO Resources — inherits from parent
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
    };
    let page_id = dest_doc.add_object(page);

    // Pages node has the font resources
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => Object::Integer(1),
        "Resources" => dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(font_obj_id),
            },
        },
    };
    dest_doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = dest_doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    dest_doc.trailer.set("Root", catalog_id);

    // Create overlay with a conflicting "F1" font
    let overlay_doc = create_pdf_with_named_resources(&["F1"]);

    // Apply overlay — should succeed even though dest's resources are inherited
    let result = overlay_page(&mut dest_doc, page_id, &overlay_doc, 1);
    assert!(
        result.is_ok(),
        "Overlay with inherited resources should succeed: {:?}",
        result.err()
    );
}
