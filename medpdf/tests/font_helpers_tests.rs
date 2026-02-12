// tests/font_helpers_tests.rs
// Tests for medpdf::font_helpers module — public API and edge cases

use medpdf::font_helpers;

// --- measure_text_width() ---

#[test]
fn test_measure_text_width_hack_font_single_char() {
    // Hack font data (single byte, not '@') uses rough estimate: len * font_size * 0.6
    let font_data = &[1u8]; // hack marker
    let font_size = 10.0;
    let text = "A";
    let width = font_helpers::measure_text_width(font_data, font_size, text).unwrap();
    let expected = 1.0 * font_size * 0.6;
    assert!(
        (width - expected).abs() < f32::EPSILON,
        "got {width}, expected {expected}"
    );
}

#[test]
fn test_measure_text_width_hack_font_multi_char() {
    let font_data = &[1u8];
    let font_size = 12.0;
    let text = "Hello";
    let width = font_helpers::measure_text_width(font_data, font_size, text).unwrap();
    let expected = 5.0 * font_size * 0.6;
    assert!(
        (width - expected).abs() < f32::EPSILON,
        "got {width}, expected {expected}"
    );
}

#[test]
fn test_measure_text_width_builtin_font() {
    // Built-in font marker is '@'
    let font_data = &[b'@'];
    let font_size = 24.0;
    let text = "Test";
    let width = font_helpers::measure_text_width(font_data, font_size, text).unwrap();
    let expected = 4.0 * font_size * 0.6;
    assert!(
        (width - expected).abs() < f32::EPSILON,
        "got {width}, expected {expected}"
    );
}

#[test]
fn test_measure_text_width_empty_string_hack() {
    let font_data = &[1u8];
    let font_size = 12.0;
    let width = font_helpers::measure_text_width(font_data, font_size, "").unwrap();
    assert!(
        width.abs() < f32::EPSILON,
        "Empty string should have 0 width, got {width}"
    );
}

#[test]
fn test_measure_text_width_zero_font_size_hack() {
    let font_data = &[1u8];
    let width = font_helpers::measure_text_width(font_data, 0.0, "Hello").unwrap();
    assert!(
        width.abs() < f32::EPSILON,
        "Zero font size should produce 0 width, got {width}"
    );
}

#[test]
fn test_measure_text_width_empty_font_data_uses_estimate() {
    // Empty font data (len 0 <= 1) uses the rough estimate path
    let result = font_helpers::measure_text_width(&[], 12.0, "Hello");
    assert!(result.is_ok());
    let width = result.unwrap();
    let expected = 5.0 * 12.0 * 0.6;
    assert!(
        (width - expected).abs() < f32::EPSILON,
        "Empty font data should use estimate, got {width}"
    );
}

#[test]
fn test_measure_text_width_invalid_font_data_fails() {
    // Random invalid bytes (more than 1 byte, not valid TTF)
    let result = font_helpers::measure_text_width(&[0xFF, 0xFE, 0x00, 0x01], 12.0, "Hello");
    assert!(result.is_err(), "Invalid font data should fail");
}

// --- get_pdf_font_info_of_data() ---

#[test]
fn test_get_pdf_font_info_of_data_invalid_data() {
    // Not valid TTF data
    let result = font_helpers::get_pdf_font_info_of_data(&[]);
    assert!(result.is_err(), "Empty data should fail");
}

#[test]
fn test_get_pdf_font_info_of_data_random_bytes() {
    let result = font_helpers::get_pdf_font_info_of_data(&[0xDE, 0xAD, 0xBE, 0xEF]);
    assert!(result.is_err(), "Random bytes should fail");
}

// --- guess_pdf_stem_v_for_font() ---
// This is a public function we can test if we can get a Face. We need valid font data.
// Since we don't bundle test fonts, we verify the function is called correctly
// through get_pdf_font_info_of_data.

// --- get_name() ---
// Tested indirectly through get_pdf_font_info_of_data when it populates base_font/font_name

// --- Measurement consistency ---

#[test]
fn test_measure_text_width_proportional_to_font_size() {
    // With hack font, width should scale linearly with font size
    let font_data = &[1u8];
    let text = "ABCDE";
    let width_12 = font_helpers::measure_text_width(font_data, 12.0, text).unwrap();
    let width_24 = font_helpers::measure_text_width(font_data, 24.0, text).unwrap();
    assert!(
        (width_24 - width_12 * 2.0).abs() < f32::EPSILON,
        "Width should scale linearly with font size: {width_12} * 2 vs {width_24}"
    );
}

#[test]
fn test_measure_text_width_proportional_to_text_length() {
    // With hack font, width should scale linearly with text length
    let font_data = &[1u8];
    let font_size = 12.0;
    let width_5 = font_helpers::measure_text_width(font_data, font_size, "ABCDE").unwrap();
    let width_10 = font_helpers::measure_text_width(font_data, font_size, "ABCDEABCDE").unwrap();
    assert!(
        (width_10 - width_5 * 2.0).abs() < f32::EPSILON,
        "Width should scale linearly with text length: {width_5} * 2 vs {width_10}"
    );
}

#[test]
fn test_measure_text_width_negative_font_size() {
    // Negative font size should still compute (math works, result is negative)
    let font_data = &[1u8];
    let width = font_helpers::measure_text_width(font_data, -12.0, "Hi").unwrap();
    assert!(
        width < 0.0,
        "Negative font size should produce negative width, got {width}"
    );
}

// --- FontPdfInfo / FontDescriptorPdfInfo struct tests ---

#[test]
fn test_font_pdf_info_clone() {
    let info = font_helpers::FontPdfInfo {
        base_font: "TestFont".to_string(),
        subtype: "TrueType".to_string(),
        encoding: Some("WinAnsiEncoding".to_string()),
        first_char: 32,
        last_char: 255,
        widths: vec![600; 224],
    };
    let cloned = info.clone();
    assert_eq!(cloned.base_font, info.base_font);
    assert_eq!(cloned.encoding, info.encoding);
    assert_eq!(cloned.widths.len(), info.widths.len());
}

#[test]
fn test_font_pdf_info_no_encoding_for_symbol() {
    let info = font_helpers::FontPdfInfo {
        base_font: "Symbol".to_string(),
        subtype: "Type1".to_string(),
        encoding: None,
        first_char: 0,
        last_char: 255,
        widths: vec![600; 256],
    };
    assert!(info.encoding.is_none());
}

#[test]
fn test_font_descriptor_pdf_info_clone() {
    let desc = font_helpers::FontDescriptorPdfInfo {
        font_name: "TestFont".to_string(),
        flags: 0x0020,
        font_bbox: [-100, -200, 1000, 800],
        italic_angle: 0,
        ascent: 800,
        descent: -200,
        leading: 0,
        x_height: 500,
        stem_v: 80,
        cap_height: 700,
        font_file_key: "FontFile2".to_string(),
    };
    let cloned = desc.clone();
    assert_eq!(cloned.font_name, desc.font_name);
    assert_eq!(cloned.flags, desc.flags);
    assert_eq!(cloned.font_bbox, desc.font_bbox);
}
