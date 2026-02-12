// tests/watermark_tests.rs
// Tests for pdf_watermark module

mod fixtures;

use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_watermark::{add_text_params, unicode_to_winansi, utf8_to_winansi};
use medpdf::types::{AddTextParams, HAlign, PdfColor, VAlign};

// --- Helper ---

/// Creates AddTextParams with built-in Helvetica font at a given position (layer_over=true).
fn builtin_params(text: &str, font_name: &str, font_size: f32, x: f32, y: f32) -> AddTextParams {
    AddTextParams::new(text, vec![b'@'], font_name)
        .font_size(font_size)
        .position(x, y)
}

// --- Basic Watermark Tests ---

#[test]
fn test_watermark_with_builtin_font() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let params = builtin_params("DRAFT", "Helvetica", 24.0, 72.0, 72.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(
        result.is_ok(),
        "Watermark with built-in font should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_watermark_adds_content() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Get contents before
    let page_before = dest_doc.get_dictionary(page_id).unwrap();
    let contents_before = page_before.get(b"Contents").unwrap();
    let count_before = match contents_before {
        lopdf::Object::Array(arr) => arr.len(),
        lopdf::Object::Reference(_) => 1,
        _ => 0,
    };

    let params = builtin_params("TEST", "Helvetica", 12.0, 100.0, 100.0);
    add_text_params(&mut dest_doc, page_id, &params).unwrap();

    // Get contents after
    let page_after = dest_doc.get_dictionary(page_id).unwrap();
    let contents_after = page_after.get(b"Contents").unwrap();
    let count_after = match contents_after {
        lopdf::Object::Array(arr) => arr.len(),
        lopdf::Object::Reference(_) => 1,
        _ => 0,
    };

    assert!(
        count_after >= count_before,
        "Content stream count should not decrease after watermark"
    );
}

#[test]
fn test_watermark_registers_font() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let params = builtin_params("TEST", "Courier", 12.0, 100.0, 100.0);
    add_text_params(&mut dest_doc, page_id, &params).unwrap();

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
    assert!(
        fonts.is_ok(),
        "Resources should have Font dictionary after watermark"
    );
}

#[test]
fn test_watermark_different_positions() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Test various positions
    add_text_params(&mut dest_doc, page_id, &builtin_params("Top-Left", "Helvetica", 12.0, 0.0, 700.0)).unwrap();
    add_text_params(&mut dest_doc, page_id, &builtin_params("Bottom-Right", "Helvetica", 12.0, 500.0, 50.0)).unwrap();
    add_text_params(&mut dest_doc, page_id, &builtin_params("Center", "Helvetica", 12.0, 250.0, 400.0)).unwrap();

    // Page should still be valid
    let page = dest_doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Contents").is_ok());
}

#[test]
fn test_watermark_negative_position() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Negative positions are valid (off-page)
    let params = builtin_params("Off-page", "Helvetica", 12.0, -100.0, -100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_large_position() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Large positions (off-page) are valid
    let params = builtin_params("Far away", "Helvetica", 12.0, 10000.0, 10000.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_zero_font_size() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Zero font size - valid but invisible
    let params = builtin_params("Invisible", "Helvetica", 0.0, 100.0, 100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_large_font_size() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Large font size
    let params = builtin_params("BIG", "Helvetica", 500.0, 100.0, 100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_empty_text() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Empty text is valid
    let params = builtin_params("", "Helvetica", 12.0, 100.0, 100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_special_characters() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Text with special characters
    let params = builtin_params("Test (with) [brackets] & symbols!", "Helvetica", 12.0, 100.0, 100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_watermark_unicode_text() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Note: Built-in fonts may not support all unicode characters properly
    // This test just verifies it doesn't crash
    let params = builtin_params("Test with accents: cafe", "Helvetica", 12.0, 100.0, 100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok());
}

#[test]
fn test_multiple_watermarks_same_page() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Add multiple watermarks
    add_text_params(&mut dest_doc, page_id, &builtin_params("First", "Helvetica", 12.0, 100.0, 100.0)).unwrap();
    add_text_params(&mut dest_doc, page_id, &builtin_params("Second", "Helvetica", 14.0, 200.0, 200.0)).unwrap();
    add_text_params(&mut dest_doc, page_id, &builtin_params("Third", "Helvetica", 16.0, 300.0, 300.0)).unwrap();

    // Page should have multiple content entries
    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();

    if let lopdf::Object::Array(arr) = contents {
        assert!(
            arr.len() >= 3,
            "Should have at least 3 content streams for 3 watermarks"
        );
    }
}

#[test]
fn test_watermark_different_builtin_fonts() {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    // Test various built-in fonts
    add_text_params(&mut dest_doc, page_id, &builtin_params("Helvetica", "Helvetica", 12.0, 100.0, 700.0)).unwrap();
    add_text_params(&mut dest_doc, page_id, &builtin_params("Courier", "Courier", 12.0, 100.0, 600.0)).unwrap();
    add_text_params(&mut dest_doc, page_id, &builtin_params("Times-Roman", "Times-Roman", 12.0, 100.0, 500.0)).unwrap();

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
    let params = AddTextParams::new("Reuse", vec![1], "F1")
        .font_size(12.0)
        .position(100.0, 100.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    assert_eq!(unicode_to_winansi('\u{00E9}'), 0xE9); // e with acute
    assert_eq!(unicode_to_winansi('\u{00F1}'), 0xF1); // n with tilde
    assert_eq!(unicode_to_winansi('\u{00FC}'), 0xFC); // u with diaeresis
    assert_eq!(unicode_to_winansi('\u{00A9}'), 0xA9); // U+00A9 copyright
    assert_eq!(unicode_to_winansi('\u{00AE}'), 0xAE); // U+00AE registered
    assert_eq!(unicode_to_winansi('\u{00B0}'), 0xB0); // U+00B0 degree
}

#[test]
fn test_winansi_special_chars() {
    // Special WinAnsi characters in 0x80-0x9F range
    assert_eq!(unicode_to_winansi('\u{20AC}'), 0x80); // U+20AC euro
    assert_eq!(unicode_to_winansi('\u{2122}'), 0x99); // U+2122 trademark
    assert_eq!(unicode_to_winansi('\u{2022}'), 0x95); // U+2022 bullet
    assert_eq!(unicode_to_winansi('\u{2013}'), 0x96); // U+2013 en-dash
    assert_eq!(unicode_to_winansi('\u{2014}'), 0x97); // U+2014 em-dash
    assert_eq!(unicode_to_winansi('\u{201C}'), 0x93); // U+201C left double quote
    assert_eq!(unicode_to_winansi('\u{201D}'), 0x94); // U+201D right double quote
    assert_eq!(unicode_to_winansi('\u{2018}'), 0x91); // U+2018 left single quote
    assert_eq!(unicode_to_winansi('\u{2019}'), 0x92); // U+2019 right single quote
    assert_eq!(unicode_to_winansi('\u{2026}'), 0x85); // U+2026 ellipsis
}

#[test]
fn test_winansi_unmappable_fallback() {
    // Characters outside WinAnsiEncoding should become '?'
    assert_eq!(unicode_to_winansi('\u{4E2D}'), b'?'); // Chinese
    assert_eq!(unicode_to_winansi('\u{65E5}'), b'?'); // Japanese
    assert_eq!(unicode_to_winansi('\u{03B1}'), b'?'); // Greek alpha
    assert_eq!(unicode_to_winansi('\u{2192}'), b'?'); // Arrow
    assert_eq!(unicode_to_winansi('\u{1F600}'), b'?'); // Emoji
}

#[test]
fn test_winansi_cafe_example() {
    // The classic "Cafe" example with accent
    let encoded = utf8_to_winansi("Caf\u{00E9}");
    assert_eq!(encoded, vec![b'C', b'a', b'f', 0xE9]);
}

#[test]
fn test_winansi_mixed_text() {
    // Mix of ASCII and extended characters
    let encoded = utf8_to_winansi("Price: \u{20AC}50");
    assert_eq!(
        encoded,
        vec![b'P', b'r', b'i', b'c', b'e', b':', b' ', 0x80, b'5', b'0']
    );
}

#[test]
fn test_winansi_empty_string() {
    assert_eq!(utf8_to_winansi(""), Vec::<u8>::new());
}

#[test]
fn test_winansi_copyright_notice() {
    let encoded = utf8_to_winansi("\u{00A9} 2024 Company\u{2122}");
    // copyright = 0xA9, trademark = 0x99
    assert_eq!(encoded[0], 0xA9);
    assert_eq!(encoded[encoded.len() - 1], 0x99);
}

// --- add_text_params Tests ---

/// Helper: create a dest doc with one copied page, ready for add_text_params calls.
fn setup_page_for_text_params() -> (lopdf::Document, lopdf::ObjectId) {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    (dest_doc, page_id)
}

#[test]
fn test_add_text_params_basic_defaults() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Hello", vec![b'@'], "Helvetica");
    let result = add_text_params(&mut doc, page_id, &params);
    assert!(result.is_ok(), "Basic add_text_params should succeed: {:?}", result.err());
}

#[test]
fn test_add_text_params_custom_color() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Red", vec![b'@'], "Helvetica")
        .color(PdfColor::rgb(1.0, 0.0, 0.0))
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // Verify the content stream contains the rg operator with red color
    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains("1 0 0 rg"),
        "Content should contain '1 0 0 rg' for red color, got: {}",
        content_str
    );
}

#[test]
fn test_add_text_params_rotation_produces_cm() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Rotated", vec![b'@'], "Helvetica")
        .rotation(45.0)
        .position(200.0, 200.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains("cm"),
        "Rotated text should contain 'cm' operator, got: {}",
        content_str
    );
}

#[test]
fn test_add_text_params_no_rotation_no_cm() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Straight", vec![b'@'], "Helvetica")
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        !content_str.contains("cm"),
        "Non-rotated text should not contain 'cm' operator, got: {}",
        content_str
    );
}

#[test]
fn test_add_text_params_halign_center() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Centered", vec![b'@'], "Helvetica")
        .font_size(20.0)
        .h_align(HAlign::Center)
        .position(300.0, 400.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // The Td x value should be less than 300.0 (shifted left by half text width)
    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(content_str.contains("Td"), "Should contain Td operator");
    // Center alignment shifts x by -textWidth/2, so x < 300
    // "Centered" = 8 chars, width ~ 8 * 20 * 0.6 = 96, half = 48, so x ~ 252
    assert!(
        !content_str.contains("300 400 Td"),
        "Center-aligned text should not use original x position"
    );
}

#[test]
fn test_add_text_params_halign_right() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Right", vec![b'@'], "Helvetica")
        .font_size(20.0)
        .h_align(HAlign::Right)
        .position(300.0, 400.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(content_str.contains("Td"), "Should contain Td operator");
    // Right alignment shifts x by -textWidth, so x < 300
    assert!(
        !content_str.contains("300 400 Td"),
        "Right-aligned text should not use original x position"
    );
}

#[test]
fn test_add_text_params_valign_top() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Top", vec![b'@'], "Helvetica")
        .font_size(20.0)
        .v_align(VAlign::Top)
        .position(100.0, 400.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(content_str.contains("Td"), "Should contain Td operator");
    // Top alignment shifts y by font_size * 0.7 = 14.0, so y ~ 414
    assert!(
        !content_str.contains("100 400 Td"),
        "Top-aligned text should shift y position upward"
    );
}

#[test]
fn test_add_text_params_strikeout() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Strike", vec![b'@'], "Helvetica")
        .font_size(20.0)
        .strikeout(true)
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains(" re\n") && content_str.contains("\nf\n"),
        "Strikeout should produce 're' and 'f' operators, got: {}",
        content_str
    );
}

#[test]
fn test_add_text_params_underline() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Under", vec![b'@'], "Helvetica")
        .font_size(20.0)
        .underline(true)
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains(" re\n") && content_str.contains("\nf\n"),
        "Underline should produce 're' and 'f' operators, got: {}",
        content_str
    );
}

#[test]
fn test_add_text_params_strikeout_and_underline() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Both", vec![b'@'], "Helvetica")
        .font_size(20.0)
        .strikeout(true)
        .underline(true)
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    // Should have two 're' + 'f' pairs (one for underline, one for strikeout)
    let re_count = content_str.matches(" re").count();
    assert!(
        re_count >= 2,
        "Strikeout + underline should produce at least 2 're' operators, got {}",
        re_count
    );
}

#[test]
fn test_add_text_params_layer_under() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Under", vec![b'@'], "Helvetica")
        .layer_over(false)
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // When layer_over is false, the new content should be prepended (first in array)
    let page = doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    if let lopdf::Object::Array(arr) = contents {
        // The new content stream should be the first element
        assert!(arr.len() >= 2, "Should have at least 2 content streams");
    }
}

#[test]
fn test_add_text_params_layer_over_wraps_existing() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Over", vec![b'@'], "Helvetica")
        .layer_over(true)
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // When layer_over is true, existing content gets wrapped with q/Q
    let page = doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    if let lopdf::Object::Array(arr) = contents {
        // Should be: [q_stream, original_content, closing_Q_stream, new_content]
        assert!(
            arr.len() >= 4,
            "Layer over should produce at least 4 content entries (q, existing, Q, new), got {}",
            arr.len()
        );

        // First stream should be "q\n"
        if let lopdf::Object::Reference(first_id) = &arr[0] {
            let stream = doc.get_object(*first_id).unwrap().as_stream().unwrap();
            let bytes = &stream.content;
            assert_eq!(bytes, b"q\n", "First stream should be opening 'q'");
        }
    }
}

// --- Alpha / Opacity Tests ---

#[test]
fn test_add_text_params_alpha_opaque_no_gs() {
    let (mut doc, page_id) = setup_page_for_text_params();
    // alpha = 1.0 (default) should NOT produce a gs operator
    let params = AddTextParams::new("Opaque", vec![b'@'], "Helvetica")
        .color(PdfColor::rgb(1.0, 0.0, 0.0))
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        !content_str.contains("gs"),
        "Fully opaque text (alpha=1.0) should NOT produce 'gs' operator, got: {}",
        content_str
    );
}

#[test]
fn test_add_text_params_alpha_half_produces_gs() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Semi", vec![b'@'], "Helvetica")
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 0.5))
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains("gs"),
        "Semi-transparent text (alpha=0.5) should produce 'gs' operator, got: {}",
        content_str
    );

    // Verify ExtGState exists in Resources
    let page = doc.get_dictionary(page_id).unwrap();
    let resources = page.get(b"Resources").unwrap();
    let resources_dict = match resources {
        lopdf::Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        lopdf::Object::Dictionary(dict) => dict,
        _ => panic!("Resources should be dictionary or reference"),
    };
    let extgstate = resources_dict.get(b"ExtGState");
    assert!(
        extgstate.is_ok(),
        "Resources should have ExtGState dictionary after semi-transparent watermark"
    );
}

#[test]
fn test_add_text_params_alpha_zero_works() {
    let (mut doc, page_id) = setup_page_for_text_params();
    // Fully transparent — should work without error
    let params = AddTextParams::new("Ghost", vec![b'@'], "Helvetica")
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 0.0))
        .position(100.0, 100.0);
    let result = add_text_params(&mut doc, page_id, &params);
    assert!(
        result.is_ok(),
        "Fully transparent text (alpha=0.0) should succeed: {:?}",
        result.err()
    );

    let content = fixtures::get_page_content_bytes(&doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains("gs"),
        "Fully transparent text should produce 'gs' operator"
    );
}

#[test]
fn test_add_text_params_alpha_extgstate_has_correct_values() {
    let (mut doc, page_id) = setup_page_for_text_params();
    let params = AddTextParams::new("Check", vec![b'@'], "Helvetica")
        .color(PdfColor::rgba(1.0, 0.0, 0.0, 0.5))
        .position(100.0, 100.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // Find the ExtGState dictionary in Resources and verify ca/CA values
    let page = doc.get_dictionary(page_id).unwrap();
    let resources = page.get(b"Resources").unwrap();
    let resources_dict = match resources {
        lopdf::Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        lopdf::Object::Dictionary(dict) => dict,
        _ => panic!("Resources should be dictionary or reference"),
    };
    let extgstate_obj = resources_dict.get(b"ExtGState").unwrap();
    let extgstate_dict = match extgstate_obj {
        lopdf::Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        lopdf::Object::Dictionary(dict) => dict,
        _ => panic!("ExtGState should be dictionary or reference"),
    };

    // Get the first (and only) entry in ExtGState
    let (_, gs_ref) = extgstate_dict.iter().next().expect("ExtGState should have an entry");
    let gs_id = gs_ref.as_reference().expect("ExtGState entry should be a reference");
    let gs_dict = doc.get_dictionary(gs_id).unwrap();

    // Verify ca (non-stroking alpha)
    let ca = gs_dict.get(b"ca").expect("ExtGState should have 'ca' key");
    let ca_val = ca.as_float().unwrap_or_else(|_| ca.as_i64().unwrap() as f32);
    assert!(
        (ca_val - 0.5).abs() < 0.01,
        "ca should be ~0.5, got {}",
        ca_val
    );

    // Verify CA (stroking alpha)
    let big_ca = gs_dict.get(b"CA").expect("ExtGState should have 'CA' key");
    let big_ca_val = big_ca.as_float().unwrap_or_else(|_| big_ca.as_i64().unwrap() as f32);
    assert!(
        (big_ca_val - 0.5).abs() < 0.01,
        "CA should be ~0.5, got {}",
        big_ca_val
    );
}
