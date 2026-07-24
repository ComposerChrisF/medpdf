// tests/embedded_symbol_font_regression.rs
//
// Regression test for bugs/bug-0010: embedded symbol fonts (Wingdings, Webdings)
// produced garbage. ttf_parser::Face::glyph_index reads only Unicode cmap subtables, so
// it never sees the Microsoft (3,0) "symbol" subtable where symbol fonts publish their
// glyphs at 0xF000 + code. Two failures resulted:
//
//   - Wingdings — flagged symbolic (name), but the char-range scan and /Widths saw no
//     glyphs, so /Widths came out all-zero: every glyph advanced 0, piling text up.
//   - Webdings — the name heuristic misses it and the Unicode coverage probe saw zero
//     glyphs, so it was classified NON-symbolic and given /Encoding /WinAnsiEncoding
//     (plus all-zero widths): a silent garbage page.
//
// The fix routes the glyph lookups in detect_is_symbolic, compute_char_range,
// get_font_widths (and the two width-measurement paths, and the bug-0032 glyph check)
// through glyph_index_symbol_aware, which also consults the (3,0) subtable at 0xF000+code.
//
// To confirm these pin the fix, set MEDPDF_TEMP_BUG0010 (a temporary guard in
// glyph_index_symbol_aware that disables the symbol-cmap fallback): the nonzero-widths
// and symbolic-classification assertions then fail.

mod fixtures;

use std::sync::Arc;

use lopdf::Document;
use medpdf::types::AddTextParams;
use medpdf::{EmbeddedFontCache, FontData, add_text_params};

/// Loads a Microsoft symbol TrueType font (Wingdings / Webdings) from the usual macOS
/// locations. Returns None if none is present (keeps the test environment-tolerant).
fn load_symbol_font(basenames: &[&str]) -> Option<Arc<Vec<u8>>> {
    let dirs = [
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/System/Library/Fonts",
    ];
    for dir in dirs {
        for name in basenames {
            if let Ok(data) = std::fs::read(format!("{dir}/{name}")) {
                return Some(Arc::new(data));
            }
        }
    }
    None
}

fn load_wingdings() -> Option<Arc<Vec<u8>>> {
    load_symbol_font(&["Wingdings.ttf"])
}

fn load_webdings() -> Option<Arc<Vec<u8>>> {
    load_symbol_font(&["Webdings.ttf"])
}

/// The embedded simple TrueType font dict (Type /Font, Subtype /TrueType, with /Widths)
/// added by the WinAnsi fast path.
fn simple_truetype_font(doc: &Document) -> lopdf::Dictionary {
    doc.objects
        .values()
        .find_map(|obj| {
            let d = obj.as_dict().ok()?;
            (d.get(b"Type").ok()?.as_name().ok()? == b"Font"
                && d.get(b"Subtype").ok()?.as_name().ok()? == b"TrueType"
                && d.has(b"Widths"))
            .then(|| d.clone())
        })
        .expect("an embedded simple TrueType font dict with /Widths must exist")
}

/// Draws `text` with the embedded font (lossy, so a symbol byte with no glyph never
/// aborts the draw), returning the resulting document.
fn draw_embedded_lossy(data: Arc<Vec<u8>>, text: &str) -> Document {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    let params = AddTextParams::new(text, FontData::Embedded(data), "SymbolFont")
        .position(72.0, 400.0)
        .lossy_text(true);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();
    doc
}

#[test]
fn wingdings_embed_has_nonzero_widths_and_no_encoding() {
    let Some(data) = load_wingdings() else {
        eprintln!("Wingdings not available; skipping bug-0010 Wingdings test");
        return;
    };
    let doc = draw_embedded_lossy(data, "abcABC");
    let font = simple_truetype_font(&doc);

    let widths = font.get(b"Widths").unwrap().as_array().unwrap();
    assert!(
        widths.iter().any(|w| w.as_i64().unwrap_or(0) != 0),
        "Wingdings /Widths must contain nonzero advances (symbol glyphs live at \
         0xF000+code); got all-zero — bug-0010"
    );
    assert!(
        !font.has(b"Encoding"),
        "a symbol font must NOT carry /Encoding /WinAnsiEncoding (its built-in symbol \
         encoding applies) — bug-0010"
    );
}

#[test]
fn webdings_classified_symbolic_no_encoding() {
    // Webdings is the classification case: the name heuristic misses it, so only the
    // symbol-aware coverage probe correctly flags it symbolic and drops /Encoding.
    let Some(data) = load_webdings() else {
        eprintln!("Webdings not available; skipping bug-0010 Webdings test");
        return;
    };
    let doc = draw_embedded_lossy(data, "abcABC");
    let font = simple_truetype_font(&doc);

    assert!(
        !font.has(b"Encoding"),
        "Webdings must be classified symbolic (no /Encoding); the name heuristic misses \
         it, so only the 0xF000 coverage probe catches it — bug-0010"
    );
    let widths = font.get(b"Widths").unwrap().as_array().unwrap();
    assert!(
        widths.iter().any(|w| w.as_i64().unwrap_or(0) != 0),
        "Webdings /Widths must contain nonzero advances — bug-0010"
    );
}

#[test]
fn embedded_symbol_font_draws_without_error_non_lossy() {
    // The bug-0032 glyph check must be symbol-aware: bytes that render a symbol glyph
    // (via 0xF000+byte) are NOT "unrepresentable", so an embedded symbol font must draw
    // without UnrepresentableText even with lossy_text = false.
    let Some(data) = load_wingdings() else {
        eprintln!("Wingdings not available; skipping bug-0010 non-lossy draw test");
        return;
    };
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    let params =
        AddTextParams::new("abcABC", FontData::Embedded(data), "SymbolFont").position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).expect(
        "an embedded symbol font must draw without UnrepresentableText — its bytes render \
         symbol glyphs (bug-0010, guarding the bug-0032 check)",
    );
}

#[test]
fn measure_text_width_nonzero_for_symbol_font() {
    // Width measurement must also be symbol-aware, so a symbol watermark aligns against
    // real advances rather than zero.
    let Some(data) = load_wingdings() else {
        eprintln!("Wingdings not available; skipping bug-0010 measure test");
        return;
    };
    let width = medpdf::measure_text_width(&FontData::Embedded(data), 48.0, "abcABC")
        .expect("measuring symbol text must succeed");
    assert!(
        width > 0.0,
        "a symbol font's measured width must be nonzero (its glyphs live in a (3,0) cmap \
         glyph_index skips) — bug-0010"
    );
}
