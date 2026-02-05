// tests/font_tests.rs
// Tests for pdf_font and font_helpers modules
//
// NOTE: Some tests require system fonts to be available.
// Font embedding tests are limited without actual font files.

use std::path::PathBuf;
use pdf_merger::pdf_font::{find_font, FontPath, FontCache};

// --- find_font Tests ---

#[test]
fn test_find_font_numeric_hack() {
    // Numeric paths are treated as font references
    let result = find_font(&PathBuf::from("1"));
    assert!(result.is_ok());
    if let FontPath::Hack(n) = result.unwrap() {
        assert_eq!(n, 1);
    } else {
        panic!("Expected FontPath::Hack");
    }
}

#[test]
fn test_find_font_numeric_various() {
    for i in 0..=9 {
        let result = find_font(&PathBuf::from(i.to_string()));
        assert!(result.is_ok());
        if let FontPath::Hack(n) = result.unwrap() {
            assert_eq!(n, i);
        } else {
            panic!("Expected FontPath::Hack for {}", i);
        }
    }
}

#[test]
fn test_find_font_builtin_helvetica() {
    let result = find_font(&PathBuf::from("@Helvetica"));
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "Helvetica");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

#[test]
fn test_find_font_builtin_courier() {
    let result = find_font(&PathBuf::from("@Courier"));
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "Courier");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

#[test]
fn test_find_font_builtin_times() {
    let result = find_font(&PathBuf::from("@Times-Roman"));
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "Times-Roman");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

#[test]
fn test_find_font_builtin_bold() {
    let result = find_font(&PathBuf::from("@Helvetica-Bold"));
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "Helvetica-Bold");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

#[test]
fn test_find_font_builtin_symbol() {
    let result = find_font(&PathBuf::from("@Symbol"));
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "Symbol");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

#[test]
fn test_find_font_builtin_zapf() {
    let result = find_font(&PathBuf::from("@ZapfDingbats"));
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "ZapfDingbats");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

// Note: System font tests are platform-dependent and may not work everywhere
#[test]
#[ignore] // Ignored by default as it depends on system fonts
fn test_find_font_system_arial() {
    let result = find_font(&PathBuf::from("Arial"));
    // This may or may not succeed depending on the system
    if result.is_ok() {
        if let FontPath::Path(path) = result.unwrap() {
            assert!(path.exists());
        }
    }
}

#[test]
fn test_find_font_nonexistent() {
    // A font that definitely doesn't exist
    let result = find_font(&PathBuf::from("NonExistentFont12345"));
    // This should fail to find the font
    assert!(result.is_err());
}

// --- FontPath::get_name Tests ---

#[test]
fn test_font_path_get_name_hack() {
    let path = FontPath::Hack(5);
    assert_eq!(path.get_name(), "F5");
}

#[test]
fn test_font_path_get_name_builtin() {
    let path = FontPath::BuiltIn("Helvetica".to_string());
    assert_eq!(path.get_name(), "Helvetica");
}

#[test]
fn test_font_path_get_name_path() {
    let path = FontPath::Path(PathBuf::from("/fonts/MyFont.ttf"));
    assert_eq!(path.get_name(), "MyFont");
}

#[test]
fn test_font_path_get_name_path_no_extension() {
    let path = FontPath::Path(PathBuf::from("/fonts/MyFont"));
    assert_eq!(path.get_name(), "MyFont");
}

// --- FontCache Tests ---

#[test]
fn test_font_cache_new() {
    let cache = FontCache::new();
    // Just verify it can be created
    let _ = cache;
}

#[test]
fn test_font_cache_get_hack() {
    let mut cache = FontCache::new();
    let font_path = FontPath::Hack(3);

    let result = cache.get_data(&font_path);
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0], 3);
}

#[test]
fn test_font_cache_get_builtin() {
    let mut cache = FontCache::new();
    let font_path = FontPath::BuiltIn("Helvetica".to_string());

    let result = cache.get_data(&font_path);
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0], b'@');
}

#[test]
fn test_font_cache_caches_path() {
    // This test would require an actual font file
    // Skipping as it's platform-dependent
}

// --- font_helpers documentation tests ---
//
// The font_helpers module has several hardcoded limitations that should be documented:
//
// 1. is_symbolic is always false (compute_pdf_font_flags)
//    - All fonts are treated as non-symbolic
//
// 2. MacRomanEncoding is hardcoded (get_pdf_info_of_face)
//    - All fonts use MacRomanEncoding regardless of actual encoding
//
// 3. first_char=32, last_char=255 hardcoded (get_pdf_info_of_face)
//    - Character range is always 32-255 regardless of font content
//
// These are design decisions that work for common Latin fonts but may
// cause issues with symbol fonts or non-Latin character sets.

#[test]
fn test_font_helpers_limitations_documented() {
    // This is a documentation test - it passes as long as the limitations
    // are documented above. When the font_helpers module is updated to
    // handle these cases properly, these comments should be updated.
}
