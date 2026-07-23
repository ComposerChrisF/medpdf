// tests/cff_otf_font_structure_regression.rs
//
// Regression test for bugs/bug-0005: CFF-flavored (.otf) fonts were embedded with
// structurally invalid PDF objects, so conforming viewers (poppler: "Unknown font
// type" / "Mismatch between font type and embedded font file") rejected the embedded
// program and substituted a font. Three faults, all for CFF outlines:
//   1. Simple-path Font dict got /Subtype /Type1C — a *stream* subtype, never legal on
//      a Font dict (must be /Type1). (PDF 32000-1 Table 110.)
//   2. The FontFile3 stream had no /Subtype (must be /OpenType for sfnt-wrapped OTF)
//      and a meaningless /Length1 (defined only for FontFile/FontFile2).
//   3. The composite path paired a CIDFontType2 descendant (which requires TrueType)
//      with a CFF program.
//
// The fix classifies by outline flavor: CFF simple fonts emit /Type1 + FontFile3
// /Subtype /OpenType and drop /Length1; composite CFF fails loudly (CIDFontType0 not
// yet implemented). TrueType (glyf) fonts are unchanged.
//
// To confirm this pins the fix, revert font_helpers::classify_font to return the old
// ("FontFile3","Type1C") pair and drop the descriptor stream fields; the simple test
// then sees /Type1C and a Length1'd FontFile3 with no /Subtype, and the composite test
// stops erroring.

mod fixtures;

use std::sync::Arc;

use lopdf::{Document, Object};
use medpdf::types::AddTextParams;
use medpdf::{EmbeddedFontCache, FontData, add_text_params};
use ttf_parser::{Face, Tag};

/// True if the font program is CFF-flavored (has a `CFF ` table and no `glyf`).
fn is_cff(data: &[u8]) -> bool {
    match Face::parse(data, 0) {
        Ok(face) => {
            face.raw_face().table(Tag::from_bytes(b"CFF ")).is_some()
                && face.raw_face().table(Tag::from_bytes(b"glyf")).is_none()
        }
        Err(_) => false,
    }
}

/// Loads the first available CFF `.otf` font on this machine, or None (test skips).
fn load_cff_otf() -> Option<Arc<Vec<u8>>> {
    let candidates = [
        "/Users/chris/Library/Fonts/BellMTStd-Regular.otf",
        "/Library/Fonts/Academico-Regular.otf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path)
            && is_cff(&bytes)
        {
            return Some(Arc::new(bytes));
        }
    }
    None
}

fn a_page() -> (Document, lopdf::ObjectId) {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    (doc, page_id)
}

/// Finds the embedded font dict — a Font dict that has its own /FontDescriptor
/// (built-in fonts have none). Panics if absent.
fn find_embedded_font(doc: &Document) -> lopdf::Dictionary {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"Font")
                && d.has(b"FontDescriptor")
        })
        .expect("an embedded font dict (with /FontDescriptor) must be present")
        .clone()
}

fn name_of<'a>(d: &'a lopdf::Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    d.get(key).ok().and_then(|o| o.as_name().ok())
}

#[test]
fn cff_simple_font_has_valid_structure() {
    let Some(data) = load_cff_otf() else {
        eprintln!("no CFF .otf available; skipping bug-0005 simple-path test");
        return;
    };

    let (mut doc, page_id) = a_page();
    let params = AddTextParams::new("Structure", FontData::Embedded(data), "CffFont")
        .font_size(24.0)
        .position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect("embedding a CFF simple font must succeed");

    let font = find_embedded_font(&doc);

    // Fault 1: the Font dict /Subtype must be /Type1, never /Type1C (a stream subtype).
    assert_eq!(
        name_of(&font, b"Subtype"),
        Some(&b"Type1"[..]),
        "CFF Font dict /Subtype must be Type1, not Type1C (bug-0005)"
    );

    // The embedded program must live under /FontFile3 for CFF.
    let desc_id = font.get(b"FontDescriptor").unwrap().as_reference().unwrap();
    let desc = doc.get_dictionary(desc_id).unwrap();
    assert!(
        desc.has(b"FontFile3"),
        "CFF descriptor must use /FontFile3 (bug-0005)"
    );
    let ff3_id = desc.get(b"FontFile3").unwrap().as_reference().unwrap();
    let ff3 = doc.get_object(ff3_id).unwrap().as_stream().unwrap();

    // Fault 2a: the FontFile3 stream must declare /Subtype /OpenType.
    assert_eq!(
        ff3.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()),
        Some(&b"OpenType"[..]),
        "FontFile3 stream must carry /Subtype /OpenType (bug-0005)"
    );
    // Fault 2b: /Length1 is defined only for FontFile/FontFile2 — must be absent here.
    assert!(
        ff3.dict.get(b"Length1").is_err(),
        "FontFile3 stream must not carry /Length1 (bug-0005)"
    );
}

#[test]
fn cff_composite_font_fails_loud() {
    let Some(data) = load_cff_otf() else {
        eprintln!("no CFF .otf available; skipping bug-0005 composite-path test");
        return;
    };

    let (mut doc, page_id) = a_page();
    // U+0101 (ā) is outside WinAnsiEncoding → forces the Type0 composite path.
    // lossy_text so a font that happens to lack the glyph still *reaches* the embed
    // step (encode emits .notdef instead of erroring first) — the embed is what must
    // fail. A neutral font name ("TestFace") avoids a substring like "cff" leaking
    // into an unrelated error and masking the assertion.
    let params = AddTextParams::new("m\u{0101}", FontData::Embedded(data), "TestFace")
        .font_size(24.0)
        .position(72.0, 400.0)
        .lossy_text(true);

    let err = add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect_err("composite CFF embedding must fail loudly, not emit a CIDFontType2/CFF mismatch (bug-0005)");
    // Assert on the guard's own distinctive token, not a loose "cff"/"composite"
    // substring that a different error could coincidentally contain.
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("cidfonttype0"),
        "the error must be the composite-CFF guard (mentioning CIDFontType0), got: {err}"
    );
}

#[test]
fn truetype_simple_font_structure_unchanged() {
    // Guards that the flavor branch did not disturb TrueType (glyf) embedding.
    let Some(data) = fixtures::load_system_ttf() else {
        eprintln!("no system TTF available; skipping TrueType guard");
        return;
    };
    // Ensure the fixture really is glyf-flavored (Arial etc.).
    if is_cff(&data) {
        eprintln!("system fixture is CFF, not glyf; skipping TrueType guard");
        return;
    }

    let (mut doc, page_id) = a_page();
    let params = AddTextParams::new("Structure", FontData::Embedded(data), "TtfFont")
        .font_size(24.0)
        .position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect("embedding a TrueType simple font must succeed");

    let font = find_embedded_font(&doc);
    assert_eq!(
        name_of(&font, b"Subtype"),
        Some(&b"TrueType"[..]),
        "TrueType Font dict /Subtype must remain TrueType"
    );
    let desc_id = font.get(b"FontDescriptor").unwrap().as_reference().unwrap();
    let desc = doc.get_dictionary(desc_id).unwrap();
    let ff2_id = desc
        .get(b"FontFile2")
        .expect("TrueType descriptor must use /FontFile2")
        .as_reference()
        .unwrap();
    let ff2 = doc.get_object(ff2_id).unwrap().as_stream().unwrap();
    assert!(
        matches!(ff2.dict.get(b"Length1"), Ok(Object::Integer(_))),
        "FontFile2 stream must keep /Length1"
    );
    assert!(
        ff2.dict.get(b"Subtype").is_err(),
        "FontFile2 stream must not carry a /Subtype"
    );
}
