// tests/font_data_tests.rs
// Tests for FontData enum and its methods

use medpdf::FontData;
use std::sync::Arc;

// --- FontData::Hack variant ---

#[test]
fn test_font_data_hack_variant() {
    let fd = FontData::Hack(0);
    assert!(matches!(fd, FontData::Hack(0)));
}

#[test]
fn test_font_data_hack_all_values() {
    for i in 0..=9 {
        let fd = FontData::Hack(i);
        assert!(matches!(fd, FontData::Hack(n) if n == i));
    }
}

#[test]
fn test_font_data_hack_embedded_bytes_returns_none() {
    let fd = FontData::Hack(5);
    assert!(fd.embedded_bytes().is_none());
}

// --- FontData::BuiltIn variant ---

#[test]
fn test_font_data_builtin_helvetica() {
    let fd = FontData::BuiltIn("Helvetica".to_string());
    assert!(matches!(fd, FontData::BuiltIn(ref name) if name == "Helvetica"));
}

#[test]
fn test_font_data_builtin_courier() {
    let fd = FontData::BuiltIn("Courier".to_string());
    assert!(matches!(fd, FontData::BuiltIn(ref name) if name == "Courier"));
}

#[test]
fn test_font_data_builtin_empty_name() {
    let fd = FontData::BuiltIn(String::new());
    assert!(matches!(fd, FontData::BuiltIn(ref name) if name.is_empty()));
}

#[test]
fn test_font_data_builtin_embedded_bytes_returns_none() {
    let fd = FontData::BuiltIn("Times-Roman".to_string());
    assert!(fd.embedded_bytes().is_none());
}

// --- FontData::Embedded variant ---

#[test]
fn test_font_data_embedded_with_data() {
    let data = Arc::new(vec![0x00, 0x01, 0x00, 0x00]); // TTF-like header
    let fd = FontData::Embedded(data.clone());
    assert!(matches!(fd, FontData::Embedded(_)));
}

#[test]
fn test_font_data_embedded_bytes_returns_data() {
    let raw = vec![0x00, 0x01, 0x00, 0x00, 0xFF, 0xFE];
    let data = Arc::new(raw.clone());
    let fd = FontData::Embedded(data);
    let bytes = fd.embedded_bytes().expect("should return Some for Embedded");
    assert_eq!(bytes, &raw);
}

#[test]
fn test_font_data_embedded_empty_data() {
    let data = Arc::new(vec![]);
    let fd = FontData::Embedded(data);
    let bytes = fd.embedded_bytes().expect("should return Some even for empty");
    assert!(bytes.is_empty());
}

#[test]
fn test_font_data_embedded_large_data() {
    let raw = vec![0xAB; 1024 * 1024]; // 1 MB
    let data = Arc::new(raw.clone());
    let fd = FontData::Embedded(data);
    let bytes = fd.embedded_bytes().unwrap();
    assert_eq!(bytes.len(), 1024 * 1024);
    assert_eq!(bytes[0], 0xAB);
}

// --- Clone behavior ---

#[test]
fn test_font_data_clone_hack() {
    let fd = FontData::Hack(3);
    let cloned = fd.clone();
    assert!(matches!(cloned, FontData::Hack(3)));
}

#[test]
fn test_font_data_clone_builtin() {
    let fd = FontData::BuiltIn("Symbol".to_string());
    let cloned = fd.clone();
    assert!(matches!(cloned, FontData::BuiltIn(ref name) if name == "Symbol"));
}

#[test]
fn test_font_data_clone_embedded_shares_arc() {
    let data = Arc::new(vec![1, 2, 3]);
    let fd = FontData::Embedded(data.clone());
    let cloned = fd.clone();
    // Both should point to the same Arc
    if let FontData::Embedded(ref arc) = cloned {
        assert_eq!(Arc::strong_count(arc), 3); // data, fd, cloned
    } else {
        panic!("Clone should preserve Embedded variant");
    }
}

// --- Debug trait ---

#[test]
fn test_font_data_debug_hack() {
    let fd = FontData::Hack(7);
    let debug = format!("{:?}", fd);
    assert!(debug.contains("Hack"));
    assert!(debug.contains("7"));
}

#[test]
fn test_font_data_debug_builtin() {
    let fd = FontData::BuiltIn("Helvetica".to_string());
    let debug = format!("{:?}", fd);
    assert!(debug.contains("BuiltIn"));
    assert!(debug.contains("Helvetica"));
}

#[test]
fn test_font_data_debug_embedded() {
    let data = Arc::new(vec![1, 2, 3]);
    let fd = FontData::Embedded(data);
    let debug = format!("{:?}", fd);
    assert!(debug.contains("Embedded"));
}

// --- Discriminant exhaustiveness ---

#[test]
fn test_font_data_embedded_bytes_exhaustive() {
    // Verify embedded_bytes returns None for non-Embedded, Some for Embedded
    let cases: Vec<(FontData, bool)> = vec![
        (FontData::Hack(0), false),
        (FontData::BuiltIn("X".into()), false),
        (FontData::Embedded(Arc::new(vec![42])), true),
    ];
    for (fd, expect_some) in cases {
        assert_eq!(
            fd.embedded_bytes().is_some(),
            expect_some,
            "embedded_bytes mismatch for {:?}",
            fd
        );
    }
}
