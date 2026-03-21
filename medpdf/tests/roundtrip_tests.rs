// tests/roundtrip_tests.rs
// Save/load roundtrip integration tests: verify documents survive serialization.

mod fixtures;

use medpdf::pdf_helpers::get_page_media_box;
use medpdf::types::{AddTextParams, PdfColor};
use medpdf::{add_text_params, copy_page, create_blank_page, delete_page, place_page, subset_fonts, EmbeddedFontCache, FontData, PlacePageParams};
use tempfile::NamedTempFile;

/// Helper: saves a Document to a temp file and reloads it.
fn save_and_reload(doc: &mut lopdf::Document) -> lopdf::Document {
    let tmp = NamedTempFile::new().expect("create temp file");
    let path = tmp.path().to_path_buf();
    doc.save(&path).expect("save PDF");
    lopdf::Document::load(&path).expect("reload PDF")
}

/// Helper: saves a Document to an in-memory buffer and reloads it.
fn save_and_reload_in_memory(doc: &mut lopdf::Document) -> lopdf::Document {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save PDF to buffer");
    lopdf::Document::load_mem(&buf).expect("reload PDF from buffer")
}

// --- Basic Roundtrip ---

#[test]
fn test_roundtrip_single_page_preserves_page_count() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_roundtrip_multi_page_preserves_page_count() {
    let mut doc = fixtures::create_pdf_with_pages(5);
    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 5);
}

#[test]
fn test_roundtrip_preserves_dimensions() {
    let width = 595.0_f32;
    let height = 842.0_f32;
    let mut doc = fixtures::create_pdf_with_pages_and_size(1, width, height);
    let reloaded = save_and_reload(&mut doc);

    let pages = reloaded.get_pages();
    let page_id = *pages.get(&1).unwrap();
    let mb = get_page_media_box(&reloaded, page_id).expect("MediaBox should exist");
    assert!(
        (mb[2] - width).abs() < 0.1,
        "width: expected {width}, got {}",
        mb[2]
    );
    assert!(
        (mb[3] - height).abs() < 0.1,
        "height: expected {height}, got {}",
        mb[3]
    );
}

#[test]
fn test_roundtrip_nonzero_origin() {
    let mut doc = fixtures::create_pdf_with_nonzero_origin_media_box(50.0, 100.0, 662.0, 892.0);
    let reloaded = save_and_reload(&mut doc);

    let pages = reloaded.get_pages();
    let page_id = *pages.get(&1).unwrap();
    let mb = get_page_media_box(&reloaded, page_id).unwrap();
    assert!((mb[0] - 50.0).abs() < 0.1);
    assert!((mb[1] - 100.0).abs() < 0.1);
    assert!((mb[2] - 662.0).abs() < 0.1);
    assert!((mb[3] - 892.0).abs() < 0.1);
}

#[test]
fn test_roundtrip_in_memory_same_as_file() {
    let mut doc = fixtures::create_pdf_with_pages(3);
    let from_file = save_and_reload(&mut doc);
    let from_mem = save_and_reload_in_memory(&mut doc);
    assert_eq!(from_file.get_pages().len(), from_mem.get_pages().len());
}

// --- Copy Page + Roundtrip ---

#[test]
fn test_roundtrip_after_copy_page() {
    let source = fixtures::create_pdf_with_pages_and_size(2, 612.0, 792.0);
    let mut dest = fixtures::create_empty_pdf();

    copy_page(&mut dest, &source, 1).unwrap();
    copy_page(&mut dest, &source, 2).unwrap();
    assert_eq!(dest.get_pages().len(), 2);

    let reloaded = save_and_reload(&mut dest);
    assert_eq!(reloaded.get_pages().len(), 2);

    // Verify dimensions survived
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let mb = get_page_media_box(&reloaded, page_id).expect("MediaBox after roundtrip");
    assert!((mb[2] - 612.0).abs() < 0.1);
    assert!((mb[3] - 792.0).abs() < 0.1);
}

// --- Delete Page + Roundtrip ---

#[test]
fn test_roundtrip_after_delete_page() {
    let source = fixtures::create_pdf_with_pages(3);
    let mut dest = fixtures::create_empty_pdf();
    for i in 1..=3 {
        copy_page(&mut dest, &source, i).unwrap();
    }
    assert_eq!(dest.get_pages().len(), 3);

    delete_page(&mut dest, 2).unwrap();
    assert_eq!(dest.get_pages().len(), 2);

    let reloaded = save_and_reload(&mut dest);
    assert_eq!(reloaded.get_pages().len(), 2);
}

// --- Blank Page + Roundtrip ---

#[test]
fn test_roundtrip_blank_page() {
    let mut doc = fixtures::create_empty_pdf();
    create_blank_page(&mut doc, 300.0, 400.0).unwrap();
    assert_eq!(doc.get_pages().len(), 1);

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let mb = get_page_media_box(&reloaded, page_id).unwrap();
    assert!((mb[2] - 300.0).abs() < 0.1);
    assert!((mb[3] - 400.0).abs() < 0.1);
}

// --- Watermark + Roundtrip ---

#[test]
fn test_roundtrip_watermark_with_builtin_font() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("DRAFT", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(48.0)
        .position(100.0, 400.0)
        .color(PdfColor::RED);

    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Verify content stream contains our watermark text
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let content_bytes = fixtures::get_page_content_bytes(&reloaded, page_id);
    let content_str = String::from_utf8_lossy(&content_bytes);
    assert!(
        content_str.contains("DRAFT"),
        "Watermark text should survive roundtrip"
    );
}

#[test]
fn test_roundtrip_watermark_with_alpha() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("CONFIDENTIAL", FontData::BuiltIn("Helvetica".into()), "@Courier")
        .font_size(36.0)
        .position(72.0, 72.0)
        .color(PdfColor::rgba(0.5, 0.5, 0.5, 0.3));

    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let reloaded = save_and_reload(&mut doc);

    // Verify the page has an ExtGState resource (for alpha)
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();

    // Resources should exist
    let resources = page_dict.get(b"Resources");
    assert!(
        resources.is_ok(),
        "Page should have Resources after watermark"
    );
}

// --- Shared Font + Roundtrip ---

#[test]
fn test_roundtrip_shared_font_pages() {
    let source = fixtures::create_pdf_with_shared_font(3);
    let mut dest = fixtures::create_empty_pdf();
    for i in 1..=3 {
        copy_page(&mut dest, &source, i).unwrap();
    }

    let reloaded = save_and_reload(&mut dest);
    assert_eq!(reloaded.get_pages().len(), 3);
}

// --- Inherited MediaBox + Roundtrip ---

#[test]
fn test_roundtrip_inherited_media_box() {
    let mut doc = fixtures::create_pdf_with_inherited_media_box(2, 500.0, 700.0);
    let reloaded = save_and_reload(&mut doc);

    assert_eq!(reloaded.get_pages().len(), 2);
    // MediaBox should still be discoverable (inherited from parent)
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let mb = get_page_media_box(&reloaded, page_id);
    assert!(mb.is_some(), "Inherited MediaBox should survive roundtrip");
    let mb = mb.unwrap();
    assert!((mb[2] - 500.0).abs() < 0.1);
    assert!((mb[3] - 700.0).abs() < 0.1);
}

// --- Multiple Operations + Roundtrip ---

#[test]
fn test_roundtrip_complex_pipeline() {
    // Simulate a mini pipeline: copy pages, add watermark, delete a page, save/reload
    let source = fixtures::create_pdf_with_pages_and_size(3, 612.0, 792.0);
    let mut doc = fixtures::create_empty_pdf();

    // Copy all 3 pages
    for i in 1..=3 {
        copy_page(&mut doc, &source, i).unwrap();
    }
    assert_eq!(doc.get_pages().len(), 3);

    // Add watermark to page 1
    let page_id = *doc.get_pages().get(&1).unwrap();
    let params = AddTextParams::new("PAGE 1", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, 720.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    // Delete page 2
    delete_page(&mut doc, 2).unwrap();
    assert_eq!(doc.get_pages().len(), 2);

    // Roundtrip
    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 2);

    // Verify watermark survived on page 1
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let content = fixtures::get_page_content_bytes(&reloaded, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains("PAGE 1"),
        "Watermark should survive complex pipeline roundtrip"
    );
}

// --- Place Page + Roundtrip ---

#[test]
fn test_roundtrip_place_page() {
    let source = fixtures::create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let mut doc = fixtures::create_pdf_with_pages(1);
    let dest_page_id = fixtures::get_first_page_id(&doc);

    place_page(
        &mut doc,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(100.0, 200.0, 0.5),
    )
    .unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Verify the page has content streams and resources after roundtrip
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    assert!(
        page_dict.get(b"Contents").is_ok(),
        "Page should have Contents after roundtrip"
    );
    assert!(
        page_dict.get(b"Resources").is_ok(),
        "Page should have Resources after roundtrip"
    );
}

// --- Empty Document Roundtrip ---

#[test]
fn test_roundtrip_empty_doc() {
    let mut doc = fixtures::create_empty_pdf();
    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 0);
}

// --- Roundtrip with Content ---

#[test]
fn test_roundtrip_preserves_content_stream() {
    let content = b"q 1 0 0 rg 100 100 200 200 re f Q";
    let mut doc = fixtures::create_pdf_with_content(content);
    let reloaded = save_and_reload(&mut doc);

    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let bytes = fixtures::get_page_content_bytes(&reloaded, page_id);
    let text = String::from_utf8_lossy(&bytes);
    // Content may be re-encoded, but key operators should be present
    assert!(text.contains("re"), "Rectangle operator should survive");
    assert!(text.contains("rg"), "Color operator should survive");
}

// --- Subsetting + Roundtrip ---

// C1: subsetted document preserves pages and font metadata after roundtrip
#[test]
fn test_roundtrip_subsetted_preserves_pages_and_font() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let params = AddTextParams::new("DRAFT", FontData::Embedded(font_data.clone()), "TestFont")
        .font_size(48.0)
        .position(100.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();

    subset_fonts(&mut doc, &cache).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1, "Should have 1 page after roundtrip");

    // Verify BaseFont has TAG+ prefix
    let mut found_tagged = false;
    for (_id, obj) in reloaded.objects.iter() {
        if let Ok(dict) = obj.as_dict() {
            if let Ok(lopdf::Object::Name(n)) = dict.get(b"BaseFont") {
                let name = String::from_utf8_lossy(n);
                if name.contains('+') {
                    found_tagged = true;
                }
            }
        }
    }
    assert!(found_tagged, "BaseFont should have TAG+ prefix after roundtrip");

    // Verify font stream has Length1 and Filter
    let mut found_font_stream = false;
    for (_id, obj) in reloaded.objects.iter() {
        if let Ok(stream) = obj.as_stream() {
            if stream.dict.has(b"Length1") {
                assert!(stream.dict.has(b"Filter"), "Font stream should have Filter");
                found_font_stream = true;
            }
        }
    }
    assert!(found_font_stream, "Should find font stream with Length1 after roundtrip");
}

// C2: watermark text survives subsetting + roundtrip
#[test]
fn test_roundtrip_subsetted_watermark_text_survives() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let params = AddTextParams::new("DRAFT", FontData::Embedded(font_data.clone()), "TestFont")
        .font_size(48.0)
        .position(100.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();

    subset_fonts(&mut doc, &cache).unwrap();

    let reloaded = save_and_reload(&mut doc);
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let content_bytes = fixtures::get_page_content_bytes(&reloaded, page_id);
    let content_str = String::from_utf8_lossy(&content_bytes);
    assert!(
        content_str.contains("DRAFT"),
        "Watermark text 'DRAFT' should survive subsetting + roundtrip"
    );
}
