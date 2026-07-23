// tests/simple_font_missing_glyph_regression.rs
//
// Regression test for bugs/bug-0032: the WinAnsi (simple) encoding arm of
// `add_text_params` never checked glyph presence. A character inside CP1252 but absent
// from the embedded font (e.g. U+00AD soft hyphen in a font that lacks it) sailed
// through as a zero-width byte and vanished silently — contradicting the function doc,
// the `UnrepresentableText` error doc, and CLAUDE.md, all of which promise a loud
// failure. The composite (Type0) arm already honored that contract; the simple arm did
// not. The fix mirrors the composite arm: fail loudly on a missing glyph, or substitute
// `?` with a warning under `lossy_text`.
//
// Every system TrueType font covers the full WinAnsi set, so a glyph gap has to be
// manufactured. `make_gapped_font` runs medpdf's OWN subsetter (subset_fonts, allsorts)
// down to just "AB", producing a valid font whose cmap maps only 'A', 'B', and .notdef —
// so 'C' (WinAnsi ASCII, hence on the simple path) is guaranteed absent. Subsetting is
// not the code under test, so it is a safe way to build the gap.
//
// To confirm these pin the fix, set MEDPDF_TEMP_BUG0032 (a temporary guard in
// encode_text_winansi_checked that restores the pre-fix no-check behavior): the
// fail-loud and `?`-substitution assertions then fail.

mod fixtures;

use std::sync::Arc;

use lopdf::{Document, ObjectId};
use medpdf::types::AddTextParams;
use medpdf::{EmbeddedFontCache, FontData, MedpdfError, add_text_params, subset_fonts};
use ttf_parser::Face;

/// The single decompressed font-file stream (a stream carrying `/Length1`).
fn one_font_stream(doc: &Document) -> Option<Vec<u8>> {
    doc.objects.values().find_map(|obj| {
        let stream = obj.as_stream().ok()?;
        if !stream.dict.has(b"Length1") {
            return None;
        }
        Some(
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone()),
        )
    })
}

/// Builds an embedded font with a guaranteed WinAnsi glyph gap, returning
/// `(font_bytes, present_char, missing_char)`. Subsets a system font down to "AB" via
/// medpdf's own `subset_fonts`; the result maps 'A'/'B'/.notdef only, so 'C' is absent
/// while remaining a WinAnsi-representable ASCII character (simple path). Returns None if
/// no system font is available or the subset did not yield the expected gap (skip, never
/// mislead).
fn make_gapped_font() -> Option<(Arc<Vec<u8>>, char, char)> {
    let base = fixtures::load_system_ttf()?;
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).ok()?;
    let mut cache = EmbeddedFontCache::new();
    let params =
        AddTextParams::new("AB", FontData::Embedded(base), "GapFont").position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut cache).ok()?;
    subset_fonts(&mut doc, &cache).ok()?;

    let bytes = one_font_stream(&doc)?;
    let face = Face::parse(&bytes, 0).ok()?;
    if face.glyph_index('A').is_none() || face.glyph_index('C').is_some() {
        eprintln!("subset did not produce the expected A-present/C-absent gap; skipping");
        return None;
    }
    Some((Arc::new(bytes), 'A', 'C'))
}

fn setup() -> (Document, ObjectId) {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    (doc, page_id)
}

#[test]
fn simple_path_missing_glyph_fails_loudly() {
    let Some((gapped, present, missing)) = make_gapped_font() else {
        eprintln!("no gapped font available; skipping bug-0032 fail-loud test");
        return;
    };
    let (mut doc, page_id) = setup();

    // present + missing + present: the string stays entirely on the simple path (all
    // WinAnsi-representable), so only the glyph gap is at issue.
    let text = format!("{present}{missing}{present}");
    let params =
        AddTextParams::new(&text, FontData::Embedded(gapped), "GapFont").position(72.0, 400.0);
    let result = add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new());

    match result {
        Err(MedpdfError::UnrepresentableText { chars, .. }) => {
            assert!(
                chars.contains(&missing),
                "UnrepresentableText must name the missing char '{missing}'; got {chars:?} — bug-0032"
            );
        }
        other => panic!(
            "simple path must fail loudly on a missing glyph ('{missing}'); got {other:?} — bug-0032"
        ),
    }
}

#[test]
fn simple_path_missing_glyph_lossy_substitutes_question_mark() {
    let Some((gapped, present, missing)) = make_gapped_font() else {
        eprintln!("no gapped font available; skipping bug-0032 lossy test");
        return;
    };
    let (mut doc, page_id) = setup();

    let text = format!("{present}{missing}{present}");
    let params = AddTextParams::new(&text, FontData::Embedded(gapped), "GapFont")
        .position(72.0, 400.0)
        .lossy_text(true);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect("lossy mode must succeed with a substitution");

    // The drawn literal must read "A?B": the missing char became '?' (0x3F), NOT the raw
    // WinAnsi byte 'C' (0x43) the pre-fix code emitted (which rendered as nothing).
    let want = [present as u8, b'?', present as u8];
    let raw = [present as u8, missing as u8, present as u8];
    let content = fixtures::get_page_content_bytes(&doc, page_id);
    assert!(
        content.windows(3).any(|w| w == want),
        "lossy substitution must emit '{present}?{present}' in the content stream — bug-0032"
    );
    assert!(
        !content.windows(3).any(|w| w == raw),
        "lossy substitution must not emit the raw missing char '{missing}' — bug-0032"
    );
}

#[test]
fn simple_path_control_char_does_not_error() {
    // The fix must NOT turn control characters into hard errors: they have no glyph but
    // are exempt from the glyph check (Chris deferred rejecting them; add_text_params
    // warns instead). A tab on the simple path keeps today's behavior — encode and
    // succeed — rather than tripping UnrepresentableText.
    let Some(data) = fixtures::load_system_ttf() else {
        eprintln!("no system TTF available; skipping bug-0032 control-char test");
        return;
    };
    let (mut doc, page_id) = setup();

    let params =
        AddTextParams::new("A\tB", FontData::Embedded(data), "SystemFont").position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect("a control char on the simple path must not fail loudly (deferred) — bug-0032");
}
