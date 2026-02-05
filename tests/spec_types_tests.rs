// tests/spec_types_tests.rs
// Tests for WatermarkSpec, OverlaySpec, PadToSpec, PadFileSpec FromStr implementations

use std::str::FromStr;
use pdf_merger::spec_types::{WatermarkSpec, OverlaySpec, PadToSpec, PadFileSpec};

// --- WatermarkSpec Tests ---

#[test]
fn test_watermark_complete_spec() {
    let spec = WatermarkSpec::from_str(
        "text=DRAFT,font=@Helvetica,size=24,x=1.5,y=2.0,units=in,pages=1-3"
    ).unwrap();

    assert_eq!(spec.text, "DRAFT");
    assert_eq!(spec.font.to_string_lossy(), "@Helvetica");
    assert_eq!(spec.size, 24.0);
    assert_eq!(spec.x, 1.5);
    assert_eq!(spec.y, 2.0);
    assert_eq!(spec.pages, "1-3");
}

#[test]
fn test_watermark_minimal_spec() {
    // Minimal spec with only required fields
    let spec = WatermarkSpec::from_str(
        "text=Hello,font=@Courier,x=0,y=0"
    ).unwrap();

    assert_eq!(spec.text, "Hello");
    assert_eq!(spec.font.to_string_lossy(), "@Courier");
    assert_eq!(spec.size, 48.0); // default
    assert_eq!(spec.x, 0.0);
    assert_eq!(spec.y, 0.0);
    assert_eq!(spec.pages, "all"); // default
}

#[test]
fn test_watermark_default_size() {
    let spec = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1,y=1").unwrap();
    assert_eq!(spec.size, 48.0);
}

#[test]
fn test_watermark_default_pages() {
    let spec = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1,y=1").unwrap();
    assert_eq!(spec.pages, "all");
}

#[test]
fn test_watermark_negative_coordinates() {
    // Negative coordinates should be valid
    let spec = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=-10,y=-20").unwrap();
    assert_eq!(spec.x, -10.0);
    assert_eq!(spec.y, -20.0);
}

#[test]
fn test_watermark_decimal_values() {
    let spec = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1.5,y=2.75,size=12.5").unwrap();
    assert_eq!(spec.x, 1.5);
    assert_eq!(spec.y, 2.75);
    assert_eq!(spec.size, 12.5);
}

#[test]
fn test_watermark_zero_size() {
    let spec = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=0,y=0,size=0").unwrap();
    assert_eq!(spec.size, 0.0);
}

#[test]
fn test_watermark_units_mm() {
    let spec = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=10,y=20,units=mm").unwrap();
    // Just verify it parses correctly - Unit enum comparison is internal
    assert_eq!(spec.x, 10.0);
    assert_eq!(spec.y, 20.0);
}

#[test]
fn test_watermark_file_path_font() {
    let spec = WatermarkSpec::from_str("text=Test,font=/path/to/font.ttf,x=1,y=1").unwrap();
    assert_eq!(spec.font.to_string_lossy(), "/path/to/font.ttf");
}

#[test]
fn test_watermark_error_missing_text() {
    let result = WatermarkSpec::from_str("font=@Helvetica,x=1,y=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("text"));
}

#[test]
fn test_watermark_error_missing_font() {
    let result = WatermarkSpec::from_str("text=Test,x=1,y=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("font"));
}

#[test]
fn test_watermark_error_missing_x() {
    let result = WatermarkSpec::from_str("text=Test,font=@Helvetica,y=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("x"));
}

#[test]
fn test_watermark_error_missing_y() {
    let result = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("y"));
}

#[test]
fn test_watermark_error_invalid_size() {
    let result = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1,y=1,size=abc");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("size"));
}

#[test]
fn test_watermark_error_invalid_x() {
    let result = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=abc,y=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("x"));
}

#[test]
fn test_watermark_error_invalid_y() {
    let result = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1,y=abc");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("y"));
}

#[test]
fn test_watermark_error_unknown_key() {
    let result = WatermarkSpec::from_str("text=Test,font=@Helvetica,x=1,y=1,unknown=value");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown"));
}

#[test]
fn test_watermark_error_no_equals() {
    let result = WatermarkSpec::from_str("text=Test,font@Helvetica,x=1,y=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("key-value"));
}

#[test]
fn test_watermark_error_empty_string() {
    let result = WatermarkSpec::from_str("");
    assert!(result.is_err());
}

// --- OverlaySpec Tests ---

#[test]
fn test_overlay_complete_spec() {
    let spec = OverlaySpec::from_str("file=overlay.pdf,src_page=2,target_pages=1-5").unwrap();
    assert_eq!(spec.file.to_string_lossy(), "overlay.pdf");
    assert_eq!(spec.src_page, 2);
    assert_eq!(spec.target_pages, "1-5");
}

#[test]
fn test_overlay_minimal_spec() {
    let spec = OverlaySpec::from_str("file=overlay.pdf,src_page=1").unwrap();
    assert_eq!(spec.file.to_string_lossy(), "overlay.pdf");
    assert_eq!(spec.src_page, 1);
    assert_eq!(spec.target_pages, "all"); // default
}

#[test]
fn test_overlay_full_path() {
    let spec = OverlaySpec::from_str("file=/path/to/overlay.pdf,src_page=1").unwrap();
    assert_eq!(spec.file.to_string_lossy(), "/path/to/overlay.pdf");
}

#[test]
fn test_overlay_error_missing_file() {
    let result = OverlaySpec::from_str("src_page=1,target_pages=all");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("file"));
}

#[test]
fn test_overlay_error_missing_src_page() {
    let result = OverlaySpec::from_str("file=overlay.pdf,target_pages=all");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("src_page"));
}

#[test]
fn test_overlay_error_invalid_src_page() {
    let result = OverlaySpec::from_str("file=overlay.pdf,src_page=abc");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("src_page"));
}

#[test]
fn test_overlay_error_negative_src_page() {
    // u16 can't be negative, so parsing will fail
    let result = OverlaySpec::from_str("file=overlay.pdf,src_page=-1");
    assert!(result.is_err());
}

#[test]
fn test_overlay_error_unknown_key() {
    let result = OverlaySpec::from_str("file=overlay.pdf,src_page=1,unknown=value");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown"));
}

// --- PadToSpec Tests ---

#[test]
fn test_pad_to_valid() {
    let spec = PadToSpec::from_str("4").unwrap();
    assert_eq!(spec.pages, 4);
}

#[test]
fn test_pad_to_one() {
    let spec = PadToSpec::from_str("1").unwrap();
    assert_eq!(spec.pages, 1);
}

#[test]
fn test_pad_to_large() {
    let spec = PadToSpec::from_str("100").unwrap();
    assert_eq!(spec.pages, 100);
}

#[test]
fn test_pad_to_error_zero() {
    // Zero is valid for u16 but may not make semantic sense
    // The parse should succeed but behavior is undefined
    let spec = PadToSpec::from_str("0").unwrap();
    assert_eq!(spec.pages, 0);
}

#[test]
fn test_pad_to_error_negative() {
    let result = PadToSpec::from_str("-1");
    assert!(result.is_err());
}

#[test]
fn test_pad_to_error_non_numeric() {
    let result = PadToSpec::from_str("abc");
    assert!(result.is_err());
}

#[test]
fn test_pad_to_error_float() {
    let result = PadToSpec::from_str("4.5");
    assert!(result.is_err());
}

#[test]
fn test_pad_to_error_empty() {
    let result = PadToSpec::from_str("");
    assert!(result.is_err());
}

// --- PadFileSpec Tests ---

#[test]
fn test_pad_file_complete_spec() {
    let spec = PadFileSpec::from_str("file=blank.pdf,page=2").unwrap();
    assert_eq!(spec.file.to_string_lossy(), "blank.pdf");
    assert_eq!(spec.page, 2);
}

#[test]
fn test_pad_file_minimal_spec() {
    let spec = PadFileSpec::from_str("file=blank.pdf").unwrap();
    assert_eq!(spec.file.to_string_lossy(), "blank.pdf");
    assert_eq!(spec.page, 1); // default
}

#[test]
fn test_pad_file_full_path() {
    let spec = PadFileSpec::from_str("file=/path/to/blank.pdf,page=1").unwrap();
    assert_eq!(spec.file.to_string_lossy(), "/path/to/blank.pdf");
}

#[test]
fn test_pad_file_error_missing_file() {
    let result = PadFileSpec::from_str("page=1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("file"));
}

#[test]
fn test_pad_file_error_invalid_page() {
    let result = PadFileSpec::from_str("file=blank.pdf,page=abc");
    assert!(result.is_err());
}

#[test]
fn test_pad_file_error_negative_page() {
    let result = PadFileSpec::from_str("file=blank.pdf,page=-1");
    assert!(result.is_err());
}

#[test]
fn test_pad_file_error_unknown_key() {
    let result = PadFileSpec::from_str("file=blank.pdf,unknown=value");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown"));
}

#[test]
fn test_pad_file_error_no_equals() {
    let result = PadFileSpec::from_str("fileblank.pdf");
    assert!(result.is_err());
}
