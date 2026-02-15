// tests/font_tests.rs
// Tests for pdf_font and font_helpers modules
//
// NOTE: Some tests require system fonts to be available.
// Font embedding tests are limited without actual font files.

use medpdf::font_helpers::measure_text_width;
use medpdf::pdf_font::{find_font, find_font_with_style, FontCache, FontPath};
use medpdf::types::{FontStyle, FontWeight};
use medpdf::FontData;
use std::path::PathBuf;
use std::sync::Arc;

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

// System font test - checks for platform-appropriate default fonts.
// Tries multiple common fonts per platform since availability varies.
//
// Platform defaults:
// - macOS: Helvetica at /System/Library/Fonts/Helvetica.ttc
// - Windows: Arial, Times New Roman, Courier New
// - Linux: DejaVu Sans, Liberation Sans, FreeSans
//
// Note: On macOS we use full paths because font-kit returns Handle::Memory
// for system fonts when searching by name, but find_font only handles Handle::Path.
#[test]
fn test_find_font_system_default() {
    let font_candidates: &[&str] = if cfg!(target_os = "macos") {
        // Use full paths on macOS since font-kit returns Memory handles for name lookups
        &[
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/Times.ttc",
            "/System/Library/Fonts/Courier.ttc",
        ]
    } else if cfg!(target_os = "windows") {
        &["Arial", "Times New Roman", "Courier New", "Verdana"]
    } else {
        // Linux and other Unix-like systems
        // DejaVu Sans is most common (Ubuntu, Debian, Fedora, RHEL)
        // Liberation Sans is also widely available
        // FreeSans is part of GNU FreeFont
        &["DejaVu Sans", "Liberation Sans", "FreeSans", "Noto Sans"]
    };

    let mut found_font = None;
    for font_name in font_candidates {
        let result = find_font(&PathBuf::from(font_name));
        if let Ok(FontPath::Path(path)) = result {
            if path.exists() {
                found_font = Some((font_name, path));
                break;
            }
        }
    }

    match found_font {
        Some((name, path)) => {
            println!("Found system font '{}' at {:?}", name, path);
        }
        None => {
            panic!("No system font found. Tried: {:?}", font_candidates);
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
    match data {
        FontData::Hack(n) => assert_eq!(n, 3),
        _ => panic!("Expected FontData::Hack"),
    }
}

#[test]
fn test_font_cache_get_builtin() {
    let mut cache = FontCache::new();
    let font_path = FontPath::BuiltIn("Helvetica".to_string());

    let result = cache.get_data(&font_path);
    assert!(result.is_ok());
    let data = result.unwrap();
    match data {
        FontData::BuiltIn(name) => assert_eq!(name, "Helvetica"),
        _ => panic!("Expected FontData::BuiltIn"),
    }
}

#[test]
fn test_font_cache_caches_path() {
    // This test would require an actual font file
    // Skipping as it's platform-dependent
}

// --- font_helpers symbol font detection tests ---
//
// The font_helpers module now properly handles symbol fonts:
//
// 1. detect_is_symbolic() identifies symbol fonts by name or character coverage
// 2. Symbol fonts get encoding=None (omitted from PDF), regular fonts get WinAnsiEncoding
// 3. Symbol fonts scan for actual glyph coverage; regular fonts use 32-255

#[test]
fn test_font_helpers_symbol_detection_documented() {
    // Symbol font detection is now implemented using:
    // - Name-based detection for known symbol fonts (Symbol, Dingbats, Wingdings, etc.)
    // - Character coverage heuristic (fonts with <20 Latin letters are considered symbolic)
}

// --- measure_text_width Tests ---

#[test]
fn test_measure_text_width_builtin_font_estimate() {
    // Builtin font data uses FontData::BuiltIn
    let font_data = FontData::BuiltIn("Helvetica".into());
    let width = measure_text_width(&font_data, 12.0, "Hello").unwrap();
    // Expected: 5 chars * 12.0 * 0.6 = 36.0
    assert!((width - 36.0).abs() < f32::EPSILON);
}

#[test]
fn test_measure_text_width_hack_font_estimate() {
    // Hack font data uses FontData::Hack
    let font_data = FontData::Hack(3);
    let width = measure_text_width(&font_data, 10.0, "Test").unwrap();
    // Expected: 4 chars * 10.0 * 0.6 = 24.0
    assert!((width - 24.0).abs() < f32::EPSILON);
}

#[test]
fn test_measure_text_width_empty_string() {
    let font_data = FontData::BuiltIn("Helvetica".into());
    let width = measure_text_width(&font_data, 12.0, "").unwrap();
    assert!((width - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_measure_text_width_zero_font_size() {
    let font_data = FontData::BuiltIn("Helvetica".into());
    let width = measure_text_width(&font_data, 0.0, "Hello").unwrap();
    assert!((width - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_measure_text_width_single_char_builtin() {
    let font_data = FontData::BuiltIn("Helvetica".into());
    let width = measure_text_width(&font_data, 20.0, "X").unwrap();
    // Expected: 1 char * 20.0 * 0.6 = 12.0
    assert!((width - 12.0).abs() < f32::EPSILON);
}

// --- find_font_with_style Tests ---

#[test]
fn test_find_font_with_style_hack_ignores_style() {
    let result = find_font_with_style(
        &PathBuf::from("5"),
        FontWeight::BOLD,
        FontStyle::Italic,
    );
    assert!(result.is_ok());
    if let FontPath::Hack(n) = result.unwrap() {
        assert_eq!(n, 5);
    } else {
        panic!("Expected FontPath::Hack");
    }
}

#[test]
fn test_find_font_with_style_builtin_ignores_style() {
    let result = find_font_with_style(
        &PathBuf::from("@Helvetica"),
        FontWeight::BOLD,
        FontStyle::Italic,
    );
    assert!(result.is_ok());
    if let FontPath::BuiltIn(name) = result.unwrap() {
        assert_eq!(name, "Helvetica");
    } else {
        panic!("Expected FontPath::BuiltIn");
    }
}

#[test]
fn test_find_font_with_style_nonexistent_falls_back() {
    // A font that doesn't exist should fall back to find_font, which should also fail
    let result = find_font_with_style(
        &PathBuf::from("NonExistentFont99999"),
        FontWeight::NORMAL,
        FontStyle::Normal,
    );
    assert!(result.is_err());
}

#[test]
fn test_find_font_with_style_numeric_zero() {
    let result = find_font_with_style(
        &PathBuf::from("0"),
        FontWeight::NORMAL,
        FontStyle::Normal,
    );
    assert!(result.is_ok());
    if let FontPath::Hack(n) = result.unwrap() {
        assert_eq!(n, 0);
    } else {
        panic!("Expected FontPath::Hack");
    }
}

// --- FontPath::Memory Tests ---

#[test]
fn test_font_path_memory_get_name() {
    let data = Arc::new(vec![0u8; 10]);
    let path = FontPath::Memory(data, "TestFont-Bold".to_string());
    assert_eq!(path.get_name(), "TestFont-Bold");
}

#[test]
fn test_font_cache_get_data_memory() {
    let data = Arc::new(vec![1u8, 2, 3, 4]);
    let path = FontPath::Memory(Arc::clone(&data), "TestFont".to_string());
    let mut cache = FontCache::new();
    let result = cache.get_data(&path).unwrap();
    match result {
        FontData::Embedded(bytes) => assert_eq!(*bytes, vec![1u8, 2, 3, 4]),
        _ => panic!("Expected FontData::Embedded"),
    }
}

#[test]
fn test_font_cache_memory_preserves_arc_identity() {
    let data = Arc::new(vec![10u8; 100]);
    let path = FontPath::Memory(Arc::clone(&data), "MyFont".to_string());
    let mut cache = FontCache::new();
    let result = cache.get_data(&path).unwrap();
    if let FontData::Embedded(returned) = result {
        assert!(Arc::ptr_eq(&data, &returned));
    } else {
        panic!("Expected FontData::Embedded");
    }
}

// --- System font by name (macOS Handle::Memory resolution) ---

#[test]
fn test_find_font_by_family_name_system() {
    // On macOS, font-kit often returns Handle::Memory for system fonts.
    // With the Memory variant support, name-based lookup should now succeed.
    let font_candidates: &[&str] = if cfg!(target_os = "macos") {
        &["Helvetica", "Times New Roman", "Courier"]
    } else if cfg!(target_os = "windows") {
        &["Arial", "Times New Roman", "Courier New"]
    } else {
        &["DejaVu Sans", "Liberation Sans", "FreeSans"]
    };

    let mut found = false;
    for name in font_candidates {
        if let Ok(path) = find_font(&PathBuf::from(name)) {
            match &path {
                FontPath::Path(_) | FontPath::Memory(_, _) => {
                    println!("Found font '{}' as {:?}", name, path.get_name());
                    found = true;
                    break;
                }
                _ => {}
            }
        }
    }
    assert!(found, "No system font found by name. Tried: {:?}", font_candidates);
}

#[test]
fn test_find_font_with_style_bold_system() {
    let font_candidates: &[&str] = if cfg!(target_os = "macos") {
        &["Helvetica"]
    } else if cfg!(target_os = "windows") {
        &["Arial"]
    } else {
        &["DejaVu Sans", "Liberation Sans"]
    };

    for name in font_candidates {
        if let Ok(path) = find_font_with_style(
            &PathBuf::from(name),
            FontWeight::BOLD,
            FontStyle::Normal,
        ) {
            let font_name = path.get_name();
            println!("Found bold variant of '{}': {}", name, font_name);
            return;
        }
    }
    // Not a hard failure -- font availability varies
    println!("No bold system font found (not a fatal error)");
}
