// tests/builtin_symbol_font_encoding_regression.rs
//
// Regression test for bugs/bug-0004: add_known_named_font unconditionally wrote
// /Encoding /WinAnsiEncoding into every built-in (Standard-14) font dict. For the two
// symbolic Standard-14 fonts — Symbol and ZapfDingbats — WinAnsiEncoding remaps their
// byte codes to Latin glyph names (a, b, …) they have no glyphs for, so every character
// renders as nothing: a blank page. PDF 32000-1 Annex D binds WinAnsiEncoding to
// nonsymbolic fonts only; symbolic Standard-14 fonts must keep their built-in encoding.
//
// The fix omits /Encoding for Symbol/ZapfDingbats and keeps it for all other built-ins.
//
// To confirm these pin the fix, set MEDPDF_TEMP_BUG0004 (a temporary guard in
// add_known_named_font that restores the always-WinAnsi behavior): the symbolic-font
// assertion then fails.

mod fixtures;

use lopdf::{Document, Object};
use medpdf::types::AddTextParams;
use medpdf::{EmbeddedFontCache, FontData, add_text_params};

/// The Font dict whose /BaseFont is `base_font` (a built-in Type1 font dict).
fn font_dict_with_base(doc: &Document, base_font: &[u8]) -> lopdf::Dictionary {
    doc.objects
        .values()
        .find_map(|obj| {
            let d = obj.as_dict().ok()?;
            (d.get(b"Type").ok()?.as_name().ok()? == b"Font"
                && d.get(b"BaseFont").ok()?.as_name().ok()? == base_font)
                .then(|| d.clone())
        })
        .unwrap_or_else(|| {
            panic!(
                "a Font dict with /BaseFont /{} must exist",
                String::from_utf8_lossy(base_font)
            )
        })
}

fn draw_builtin(name: &str) -> Document {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    // "abc" is ASCII (WinAnsi-representable) so it stays on the simple path; the point is
    // the /Encoding entry on the built-in font dict, not the glyph coverage.
    let params =
        AddTextParams::new("abc", FontData::BuiltIn(name.to_string()), name).position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();
    doc
}

#[test]
fn symbol_builtin_omits_encoding() {
    let doc = draw_builtin("Symbol");
    let font = font_dict_with_base(&doc, b"Symbol");
    assert!(
        !font.has(b"Encoding"),
        "symbolic Standard-14 font Symbol must NOT carry /Encoding (its built-in encoding \
         applies); got Encoding = {:?} — bug-0004",
        font.get(b"Encoding").ok()
    );
}

#[test]
fn zapfdingbats_builtin_omits_encoding() {
    let doc = draw_builtin("ZapfDingbats");
    let font = font_dict_with_base(&doc, b"ZapfDingbats");
    assert!(
        !font.has(b"Encoding"),
        "symbolic Standard-14 font ZapfDingbats must NOT carry /Encoding — bug-0004"
    );
}

#[test]
fn nonsymbolic_builtin_keeps_winansi_encoding() {
    // The fix must NOT strip /Encoding from ordinary text built-ins: Helvetica still needs
    // /Encoding /WinAnsiEncoding for cross-platform Latin text.
    let doc = draw_builtin("Helvetica");
    let font = font_dict_with_base(&doc, b"Helvetica");
    match font.get(b"Encoding") {
        Ok(Object::Name(n)) => assert_eq!(
            n.as_slice(),
            b"WinAnsiEncoding",
            "nonsymbolic built-in Helvetica must keep /Encoding /WinAnsiEncoding — bug-0004"
        ),
        other => {
            panic!("Helvetica must carry /Encoding /WinAnsiEncoding; got {other:?} — bug-0004")
        }
    }
}
