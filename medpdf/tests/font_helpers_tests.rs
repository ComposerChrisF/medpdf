// tests/font_helpers_tests.rs
// Tests for the public measure_text_width function

use medpdf::font_helpers;
use medpdf::FontData;
use std::sync::Arc;

// --- measure_text_width() ---

#[test]
fn test_measure_text_width_hack_font_single_char() {
    let font_data = &FontData::Hack(1);
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
    let font_data = &FontData::Hack(1);
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
    let font_data = &FontData::BuiltIn("Helvetica".into());
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
    let font_data = &FontData::Hack(1);
    let font_size = 12.0;
    let width = font_helpers::measure_text_width(font_data, font_size, "").unwrap();
    assert!(
        width.abs() < f32::EPSILON,
        "Empty string should have 0 width, got {width}"
    );
}

#[test]
fn test_measure_text_width_zero_font_size_hack() {
    let font_data = &FontData::Hack(1);
    let width = font_helpers::measure_text_width(font_data, 0.0, "Hello").unwrap();
    assert!(
        width.abs() < f32::EPSILON,
        "Zero font size should produce 0 width, got {width}"
    );
}

#[test]
fn test_measure_text_width_empty_font_data_uses_estimate() {
    let result = font_helpers::measure_text_width(&FontData::Hack(0), 12.0, "Hello");
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
    let result = font_helpers::measure_text_width(&FontData::Embedded(Arc::new(vec![0xFF, 0xFE, 0x00, 0x01])), 12.0, "Hello");
    assert!(result.is_err(), "Invalid font data should fail");
}

#[test]
fn test_measure_text_width_proportional_to_font_size() {
    let font_data = &FontData::Hack(1);
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
    let font_data = &FontData::Hack(1);
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
    let font_data = &FontData::Hack(1);
    let width = font_helpers::measure_text_width(font_data, -12.0, "Hi").unwrap();
    assert!(
        width < 0.0,
        "Negative font size should produce negative width, got {width}"
    );
}
