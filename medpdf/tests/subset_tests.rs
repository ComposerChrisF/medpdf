// tests/subset_tests.rs
// Integration tests for the public subset_fonts() API.

mod fixtures;

use lopdf::{Object, ObjectId};
use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_watermark::add_text_params;
use medpdf::types::AddTextParams;
use medpdf::{subset_fonts, EmbeddedFontCache, FontData};

// --- Helpers ---

/// Finds all font file streams (streams with a `Length1` key) in the document.
fn find_font_streams(doc: &lopdf::Document) -> Vec<(ObjectId, Vec<u8>)> {
    let mut result = Vec::new();
    for (id, obj) in doc.objects.iter() {
        if let Ok(stream) = obj.as_stream()
            && stream.dict.has(b"Length1")
        {
            let bytes = if stream.is_compressed() {
                stream.decompressed_content().unwrap_or_else(|_| stream.content.clone())
            } else {
                stream.content.clone()
            };
            result.push((*id, bytes));
        }
    }
    result
}

/// Total compressed size of all font streams (raw `.content` bytes, before decompression).
fn total_font_stream_compressed_size(doc: &lopdf::Document) -> usize {
    let mut total = 0;
    for (_id, obj) in doc.objects.iter() {
        if let Ok(stream) = obj.as_stream()
            && stream.dict.has(b"Length1")
        {
            total += stream.content.len();
        }
    }
    total
}

/// Finds all BaseFont name values across Font dictionaries in the document.
fn find_base_font_names(doc: &lopdf::Document) -> Vec<String> {
    let mut names = Vec::new();
    for (_id, obj) in doc.objects.iter() {
        if let Ok(dict) = obj.as_dict()
            && let Ok(Object::Name(n)) = dict.get(b"BaseFont")
        {
            names.push(String::from_utf8_lossy(n).to_string());
        }
    }
    names
}

/// Finds all FontName values across FontDescriptor dictionaries.
fn find_font_descriptor_names(doc: &lopdf::Document) -> Vec<String> {
    let mut names = Vec::new();
    for (_id, obj) in doc.objects.iter() {
        if let Ok(dict) = obj.as_dict()
            && dict.get(b"Type").ok().and_then(|v| v.as_name().ok()) == Some(b"FontDescriptor")
            && let Ok(Object::Name(n)) = dict.get(b"FontName")
        {
            names.push(String::from_utf8_lossy(n).to_string());
        }
    }
    names
}

/// Checks if a name matches the subset tag pattern: 6 uppercase letters followed by '+' and more.
fn has_subset_tag(name: &str) -> bool {
    if let Some((tag, rest)) = name.split_once('+') {
        tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()) && !rest.is_empty()
    } else {
        false
    }
}

/// Sets up a 1-page document with an embedded font watermark.
/// Returns (doc, page_id, cache).
fn setup_watermarked_doc(
    text: &str,
    font_data: &std::sync::Arc<Vec<u8>>,
) -> (lopdf::Document, ObjectId, EmbeddedFontCache) {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let params = AddTextParams::new(text, FontData::Embedded(font_data.clone()), "TestFont")
        .font_size(48.0)
        .position(100.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();

    (doc, page_id, cache)
}

// --- Tests ---

// B1: subset reduces font stream size
#[test]
fn test_subset_reduces_font_stream_size() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let (mut doc, _page_id, cache) = setup_watermarked_doc("DRAFT", &font_data);
    let size_before = total_font_stream_compressed_size(&doc);

    subset_fonts(&mut doc, &cache).unwrap();

    let size_after = total_font_stream_compressed_size(&doc);
    assert!(
        size_after < size_before,
        "Subsetted font should be smaller: before={size_before}, after={size_after}"
    );
}

// B2: subset tags BaseFont name with TAG+ prefix
#[test]
fn test_subset_tags_base_font_name() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let (mut doc, _page_id, cache) = setup_watermarked_doc("ABC", &font_data);
    subset_fonts(&mut doc, &cache).unwrap();

    let names = find_base_font_names(&doc);
    let tagged: Vec<_> = names.iter().filter(|n| has_subset_tag(n)).collect();
    assert!(!tagged.is_empty(), "At least one BaseFont should have TAG+ prefix: {names:?}");
}

// B3: subset tags FontName in FontDescriptor
#[test]
fn test_subset_tags_font_name_in_descriptor() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let (mut doc, _page_id, cache) = setup_watermarked_doc("XYZ", &font_data);
    subset_fonts(&mut doc, &cache).unwrap();

    let names = find_font_descriptor_names(&doc);
    let tagged: Vec<_> = names.iter().filter(|n| has_subset_tag(n)).collect();
    assert!(!tagged.is_empty(), "At least one FontName should have TAG+ prefix: {names:?}");
}

// B4: Length1 matches decompressed stream size after subsetting
#[test]
fn test_subset_length1_matches_decompressed_size() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let (mut doc, _page_id, cache) = setup_watermarked_doc("Test", &font_data);
    subset_fonts(&mut doc, &cache).unwrap();

    for (id, obj) in doc.objects.iter() {
        if let Ok(stream) = obj.as_stream()
            && stream.dict.has(b"Length1")
        {
            let length1 = stream.dict.get(b"Length1").unwrap().as_i64().unwrap() as usize;
            let decompressed = if stream.is_compressed() {
                stream.decompressed_content().unwrap()
            } else {
                stream.content.clone()
            };
            assert_eq!(
                decompressed.len(),
                length1,
                "Length1 should match decompressed size for stream {id:?}"
            );
        }
    }
}

// B5: multiple watermarks with same font+cache → 1 font stream, subsetted
#[test]
fn test_subset_multiple_watermarks_same_font() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let params1 = AddTextParams::new("AB", FontData::Embedded(font_data.clone()), "TestFont")
        .font_size(24.0)
        .position(72.0, 700.0);
    add_text_params(&mut doc, page_id, &params1, &mut cache).unwrap();

    let params2 = AddTextParams::new("CD", FontData::Embedded(font_data.clone()), "TestFont")
        .font_size(24.0)
        .position(72.0, 600.0);
    add_text_params(&mut doc, page_id, &params2, &mut cache).unwrap();

    let streams_before = find_font_streams(&doc);
    assert_eq!(streams_before.len(), 1, "Should have exactly 1 font stream (shared cache)");

    subset_fonts(&mut doc, &cache).unwrap();

    let names = find_base_font_names(&doc);
    let tagged_count = names.iter().filter(|n| n.contains('+')).count();
    assert!(tagged_count >= 1, "Font should be tagged after subsetting");
}

// B6: multiple distinct fonts both subsetted
#[test]
fn test_subset_multiple_distinct_fonts() {
    let font_data1 = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };
    let font_data2 = match fixtures::load_second_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: need 2 different system fonts"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let params1 = AddTextParams::new("Hello", FontData::Embedded(font_data1.clone()), "FontA")
        .font_size(24.0)
        .position(72.0, 700.0);
    add_text_params(&mut doc, page_id, &params1, &mut cache).unwrap();

    let params2 = AddTextParams::new("World", FontData::Embedded(font_data2.clone()), "FontB")
        .font_size(24.0)
        .position(72.0, 600.0);
    add_text_params(&mut doc, page_id, &params2, &mut cache).unwrap();

    let streams_before = find_font_streams(&doc);
    assert_eq!(streams_before.len(), 2, "Should have 2 font streams");

    subset_fonts(&mut doc, &cache).unwrap();

    let names = find_base_font_names(&doc);
    let tagged: Vec<_> = names.iter().filter(|n| n.contains('+')).collect();
    assert!(tagged.len() >= 2, "Both fonts should be tagged: {names:?}");
}

// B7: empty cache → no-op
#[test]
fn test_subset_empty_cache_noop() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let cache = EmbeddedFontCache::new();
    let obj_count_before = doc.objects.len();

    subset_fonts(&mut doc, &cache).unwrap();

    assert_eq!(doc.objects.len(), obj_count_before, "No objects should change");
}

// B8: built-in font unaffected by subsetting
#[test]
fn test_subset_builtin_font_unaffected() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let params = AddTextParams::new("DRAFT", FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(48.0)
        .position(100.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();

    subset_fonts(&mut doc, &cache).unwrap();

    let names = find_base_font_names(&doc);
    for name in &names {
        assert!(
            !name.contains('+'),
            "Built-in font should not have TAG+ prefix: {name}"
        );
    }
}

// B9: multi-page shared font → 1 font stream subsetted, all pages reference same font
#[test]
fn test_subset_multi_page_shared_font() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(3);
    let mut doc = fixtures::create_empty_pdf();
    let page1 = copy_page(&mut doc, &source_doc, 1).unwrap();
    let page2 = copy_page(&mut doc, &source_doc, 2).unwrap();
    let page3 = copy_page(&mut doc, &source_doc, 3).unwrap();
    let mut cache = EmbeddedFontCache::new();

    for (page_id, text) in [(page1, "P1"), (page2, "P2"), (page3, "P3")] {
        let params = AddTextParams::new(text, FontData::Embedded(font_data.clone()), "TestFont")
            .font_size(24.0)
            .position(72.0, 400.0);
        add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();
    }

    assert_eq!(find_font_streams(&doc).len(), 1, "Should share 1 font stream");

    subset_fonts(&mut doc, &cache).unwrap();

    assert_eq!(find_font_streams(&doc).len(), 1, "Should still have 1 font stream after subsetting");

    let names = find_base_font_names(&doc);
    let tagged_count = names.iter().filter(|n| n.contains('+')).count();
    assert!(tagged_count >= 1, "Font should be tagged");
}

// B10: subsetting preserves content streams
#[test]
fn test_subset_preserves_content_streams() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let (mut doc, page_id, cache) = setup_watermarked_doc("DRAFT", &font_data);

    // Record content stream bytes before subsetting
    let content_before = fixtures::get_page_content_bytes(&doc, page_id);

    subset_fonts(&mut doc, &cache).unwrap();

    // Content streams should be identical (subsetting only touches font streams)
    let content_after = fixtures::get_page_content_bytes(&doc, page_id);
    assert_eq!(
        content_before, content_after,
        "Content streams should be unchanged by subsetting"
    );
}

// B11: without calling subset_fonts, font is unmodified
#[test]
fn test_no_subset_leaves_font_unchanged() {
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let (doc, _page_id, _cache) = setup_watermarked_doc("DRAFT", &font_data);

    // BaseFont should NOT have TAG+ prefix
    let names = find_base_font_names(&doc);
    for name in &names {
        assert!(
            !name.contains('+'),
            "Without subsetting, BaseFont should not have TAG+ prefix: {name}"
        );
    }

    // Font stream should contain the full font data (compressed)
    let streams = find_font_streams(&doc);
    assert_eq!(streams.len(), 1, "Should have 1 font stream");
    let (_id, decompressed) = &streams[0];
    assert_eq!(
        decompressed.len(),
        font_data.len(),
        "Full font should be embedded without subsetting"
    );
}
