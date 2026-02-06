// tests/watermark_tests.rs
// Tests for pdf_watermark module

mod fixtures;

use pdf_merger::pdf_watermark::{add_text, utf8_to_winansi, unicode_to_winansi};
use pdf_merger::pdf_copy_page::copy_page;

// --- Basic Watermark Tests ---

#[test]
fn test_watermark_with_builtin_font() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Use built-in font marker (starts with '@')
    let font_data = b"@";
    let font_name = "Helvetica";

    let result = add_text(&mut dest_doc, page_id, "DRAFT", font_data, font_name, 24.0, 72, 72);
    assert!(result.is_ok(), "Watermark with built-in font should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_adds_content() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Get contents before
    let page_before = dest_doc.get_dictionary(page_id).unwrap();
    let contents_before = page_before.get(b"Contents").unwrap();
    let count_before = match contents_before {
        lopdf::Object::Array(arr) => arr.len(),
        lopdf::Object::Reference(_) => 1,
        _ => 0,
    };

    add_text(&mut dest_doc, page_id, "TEST", font_data, font_name, 12.0, 100, 100).unwrap();

    // Get contents after
    let page_after = dest_doc.get_dictionary(page_id).unwrap();
    let contents_after = page_after.get(b"Contents").unwrap();
    let count_after = match contents_after {
        lopdf::Object::Array(arr) => arr.len(),
        lopdf::Object::Reference(_) => 1,
        _ => 0,
    };

    assert!(count_after >= count_before,
            "Content stream count should not decrease after watermark");
}

#[test]
fn test_watermark_registers_font() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Courier";

    add_text(&mut dest_doc, page_id, "TEST", font_data, font_name, 12.0, 100, 100).unwrap();

    // Check that Resources/Font exists
    let page = dest_doc.get_dictionary(page_id).unwrap();
    let resources = page.get(b"Resources").unwrap();

    // Resources might be a reference or inline dictionary
    let resources_dict = match resources {
        lopdf::Object::Reference(id) => dest_doc.get_dictionary(*id).unwrap(),
        lopdf::Object::Dictionary(dict) => dict,
        _ => panic!("Resources should be dictionary or reference"),
    };

    let fonts = resources_dict.get(b"Font");
    assert!(fonts.is_ok(), "Resources should have Font dictionary after watermark");
}

#[test]
fn test_watermark_different_positions() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Test various positions
    add_text(&mut dest_doc, page_id, "Top-Left", font_data, font_name, 12.0, 0, 700).unwrap();
    add_text(&mut dest_doc, page_id, "Bottom-Right", font_data, font_name, 12.0, 500, 50).unwrap();
    add_text(&mut dest_doc, page_id, "Center", font_data, font_name, 12.0, 250, 400).unwrap();

    // Page should still be valid
    let page = dest_doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Contents").is_ok());
}

#[test]
fn test_watermark_negative_position() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Negative positions are valid (off-page)
    let result = add_text(&mut dest_doc, page_id, "Off-page", font_data, font_name, 12.0, -100, -100);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_large_position() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Large positions (off-page) are valid
    let result = add_text(&mut dest_doc, page_id, "Far away", font_data, font_name, 12.0, 10000, 10000);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_zero_font_size() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Zero font size - valid but invisible
    let result = add_text(&mut dest_doc, page_id, "Invisible", font_data, font_name, 0.0, 100, 100);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_large_font_size() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Large font size
    let result = add_text(&mut dest_doc, page_id, "BIG", font_data, font_name, 500.0, 100, 100);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_empty_text() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Empty text is valid
    let result = add_text(&mut dest_doc, page_id, "", font_data, font_name, 12.0, 100, 100);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_special_characters() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Text with special characters
    let result = add_text(&mut dest_doc, page_id, "Test (with) [brackets] & symbols!", font_data, font_name, 12.0, 100, 100);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_unicode_text() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Note: Built-in fonts may not support all unicode characters properly
    // This test just verifies it doesn't crash
    let result = add_text(&mut dest_doc, page_id, "Test with accents: cafe", font_data, font_name, 12.0, 100, 100);
    assert!(result.is_ok());
}

#[test]
fn test_multiple_watermarks_same_page() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";
    let font_name = "Helvetica";

    // Add multiple watermarks
    add_text(&mut dest_doc, page_id, "First", font_data, font_name, 12.0, 100, 100).unwrap();
    add_text(&mut dest_doc, page_id, "Second", font_data, font_name, 14.0, 200, 200).unwrap();
    add_text(&mut dest_doc, page_id, "Third", font_data, font_name, 16.0, 300, 300).unwrap();

    // Page should have multiple content entries
    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();

    if let lopdf::Object::Array(arr) = contents {
        assert!(arr.len() >= 3, "Should have at least 3 content streams for 3 watermarks");
    }
}

#[test]
fn test_watermark_different_builtin_fonts() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let font_data = b"@";

    // Test various built-in fonts
    add_text(&mut dest_doc, page_id, "Helvetica", font_data, "Helvetica", 12.0, 100, 700).unwrap();
    add_text(&mut dest_doc, page_id, "Courier", font_data, "Courier", 12.0, 100, 600).unwrap();
    add_text(&mut dest_doc, page_id, "Times-Roman", font_data, "Times-Roman", 12.0, 100, 500).unwrap();

    assert_eq!(dest_doc.get_pages().len(), 1);
}

// --- Font Hack Mode Tests ---

#[test]
fn test_watermark_font_hack_mode() {
    // When font_data is a single byte (not '@'), it's treated as a reference
    // to an existing font in the document
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Font data as single byte references font F1
    let font_data: &[u8] = &[1];
    let font_name = "F1";

    // This uses an existing font reference - may or may not work depending on document
    let result = add_text(&mut dest_doc, page_id, "Reuse", font_data, font_name, 12.0, 100, 100);
    assert!(result.is_ok());
}

// --- WinAnsiEncoding Conversion Tests ---

#[test]
fn test_winansi_ascii_passthrough() {
    // ASCII characters should pass through unchanged
    assert_eq!(utf8_to_winansi("Hello"), b"Hello".to_vec());
    assert_eq!(utf8_to_winansi("ABC123"), b"ABC123".to_vec());
    assert_eq!(utf8_to_winansi("!@#$%"), b"!@#$%".to_vec());
}

#[test]
fn test_winansi_latin1_supplement() {
    // Latin-1 Supplement (U+00A0-U+00FF) maps directly
    assert_eq!(unicode_to_winansi('é'), 0xE9);  // U+00E9
    assert_eq!(unicode_to_winansi('ñ'), 0xF1);  // U+00F1
    assert_eq!(unicode_to_winansi('ü'), 0xFC);  // U+00FC
    assert_eq!(unicode_to_winansi('©'), 0xA9);  // U+00A9
    assert_eq!(unicode_to_winansi('®'), 0xAE);  // U+00AE
    assert_eq!(unicode_to_winansi('°'), 0xB0);  // U+00B0
}

#[test]
fn test_winansi_special_chars() {
    // Special WinAnsi characters in 0x80-0x9F range
    assert_eq!(unicode_to_winansi('€'), 0x80);  // U+20AC
    assert_eq!(unicode_to_winansi('™'), 0x99);  // U+2122
    assert_eq!(unicode_to_winansi('•'), 0x95);  // U+2022
    assert_eq!(unicode_to_winansi('–'), 0x96);  // U+2013 en-dash
    assert_eq!(unicode_to_winansi('—'), 0x97);  // U+2014 em-dash
    assert_eq!(unicode_to_winansi('\u{201C}'), 0x93);  // U+201C left double quote
    assert_eq!(unicode_to_winansi('\u{201D}'), 0x94);  // U+201D right double quote
    assert_eq!(unicode_to_winansi('\u{2018}'), 0x91);  // U+2018 left single quote
    assert_eq!(unicode_to_winansi('\u{2019}'), 0x92);  // U+2019 right single quote
    assert_eq!(unicode_to_winansi('…'), 0x85);  // U+2026 ellipsis
}

#[test]
fn test_winansi_unmappable_fallback() {
    // Characters outside WinAnsiEncoding should become '?'
    assert_eq!(unicode_to_winansi('中'), b'?');  // Chinese
    assert_eq!(unicode_to_winansi('日'), b'?');  // Japanese
    assert_eq!(unicode_to_winansi('α'), b'?');   // Greek alpha
    assert_eq!(unicode_to_winansi('→'), b'?');   // Arrow
    assert_eq!(unicode_to_winansi('😀'), b'?');  // Emoji
}

#[test]
fn test_winansi_cafe_example() {
    // The classic "Café" example
    let encoded = utf8_to_winansi("Café");
    assert_eq!(encoded, vec![b'C', b'a', b'f', 0xE9]);
}

#[test]
fn test_winansi_mixed_text() {
    // Mix of ASCII and extended characters
    let encoded = utf8_to_winansi("Price: €50");
    assert_eq!(encoded, vec![b'P', b'r', b'i', b'c', b'e', b':', b' ', 0x80, b'5', b'0']);
}

#[test]
fn test_winansi_empty_string() {
    assert_eq!(utf8_to_winansi(""), Vec::<u8>::new());
}

#[test]
fn test_winansi_copyright_notice() {
    let encoded = utf8_to_winansi("© 2024 Company™");
    // © = 0xA9, ™ = 0x99
    assert_eq!(encoded[0], 0xA9);
    assert_eq!(encoded[encoded.len() - 1], 0x99);
}
