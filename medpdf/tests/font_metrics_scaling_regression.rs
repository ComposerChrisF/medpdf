// tests/font_metrics_scaling_regression.rs
//
// Regression test for bugs/bug-0031: the simple-font path emitted /Widths and every
// FontDescriptor metric in the font's raw unitsPerEm, but PDF 32000-1
// §9.2.4/§9.6.2.1/§9.8.1 define those in glyph space (1000 units = 1 em). For any
// embedded font whose upem ≠ 1000 (Arial/Verdana/most macOS TrueType are 2048) every
// glyph advanced upem/1000× too far — poppler rendered "DRAFT" at ~2× letter spacing,
// running off the page. The composite /W path was already correct; the simple path
// and the shared descriptor were not.
//
// The fix scales every emitted advance and descriptor metric by 1000/upem (the same
// formula pdf_font_composite::build_w_array uses). This test is font-independent: it
// computes the expected scaled values from the loaded face and asserts the emitted
// PDF values match, and that they are NOT the raw font-unit values.
//
// To confirm this test pins the fix, make glyph_space_scale return 1.0 when
// MEDPDF_TEMP_BUG0031 is set (reproducing the raw-unit pre-fix output); the test then
// fails for any upem-≠-1000 font.

mod fixtures;

use lopdf::Document;
use medpdf::types::AddTextParams;
use medpdf::{EmbeddedFontCache, FontData, add_text_params};
use ttf_parser::Face;

fn name_is(d: &lopdf::Dictionary, key: &[u8], val: &[u8]) -> bool {
    d.get(key).ok().and_then(|o| o.as_name().ok()) == Some(val)
}

/// Finds the simple (single-byte) embedded font dict — Subtype /TrueType with a
/// /Widths array — added by the WinAnsi fast path.
fn find_simple_truetype_font(doc: &Document) -> lopdf::Dictionary {
    doc.objects
        .values()
        .find_map(|obj| {
            let d = obj.as_dict().ok()?;
            (name_is(d, b"Type", b"Font")
                && name_is(d, b"Subtype", b"TrueType")
                && d.has(b"Widths"))
            .then(|| d.clone())
        })
        .expect("a simple TrueType font dict with /Widths must be present")
}

#[test]
fn simple_font_widths_and_metrics_are_scaled_to_glyph_space() {
    let Some(data) = fixtures::load_system_ttf() else {
        eprintln!("no system TTF available; skipping bug-0031 scaling test");
        return;
    };
    let face = Face::parse(&data, 0).expect("parse system font");
    let upem = face.units_per_em();
    // The bug only manifests for upem != 1000; a 1000-upem fixture cannot pin it.
    if upem == 1000 {
        eprintln!("system font is upem 1000; skipping (does not exercise scaling)");
        return;
    }
    let scale = 1000.0 / upem as f32;

    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    let params = AddTextParams::new("Widths", FontData::Embedded(data.clone()), "SystemFont")
        .font_size(36.0)
        .position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect("embedded ASCII text must succeed");

    let font = find_simple_truetype_font(&doc);
    let first_char = font.get(b"FirstChar").unwrap().as_i64().unwrap();
    let widths = font.get(b"Widths").unwrap().as_array().unwrap();

    // /Widths['A'] must be the raw advance scaled into 1000-unit glyph space, NOT the
    // raw font-unit advance the pre-fix code emitted (bug-0031).
    let a_gid = face.glyph_index('A').expect("font has glyph 'A'");
    let a_raw = face.glyph_hor_advance(a_gid).unwrap() as f32;
    let a_expected = (a_raw * scale).round() as i64;
    let a_index = ('A' as i64 - first_char) as usize;
    let a_emitted = widths[a_index].as_i64().unwrap();
    assert!(
        (a_emitted - a_expected).abs() <= 1,
        "Widths['A'] must be scaled to glyph space: emitted {a_emitted}, expected {a_expected} \
         (raw advance {a_raw}, upem {upem}) — bug-0031"
    );
    assert_ne!(
        a_emitted,
        a_raw.round() as i64,
        "Widths['A'] must not equal the raw font-unit advance for a upem-{upem} font"
    );

    // FontDescriptor /Ascent must likewise be scaled.
    let desc_id = font.get(b"FontDescriptor").unwrap().as_reference().unwrap();
    let desc = doc.get_dictionary(desc_id).unwrap();
    let ascent_emitted = desc.get(b"Ascent").unwrap().as_i64().unwrap();
    let ascent_expected = (face.ascender() as f32 * scale).round() as i64;
    assert!(
        (ascent_emitted - ascent_expected).abs() <= 1,
        "FontDescriptor /Ascent must be scaled to glyph space: emitted {ascent_emitted}, \
         expected {ascent_expected} — bug-0031"
    );
    assert_ne!(
        ascent_emitted,
        face.ascender() as i64,
        "/Ascent must not equal the raw font-unit ascender for a upem-{upem} font"
    );
}
