// tests/unicode_text_tests.rs
// Unicode text beyond WinAnsi: Type0/CIDFontType2 composite-font embedding, fail-loud
// behavior for unrepresentable text, and the lossy_text escape hatch.

mod fixtures;

use lopdf::{Document, Object, ObjectId};
use medpdf::pdf_watermark::add_text_params;
use medpdf::types::{AddTextParams, HAlign};
use medpdf::{EmbeddedFontCache, Error, FontData};
use tempfile::NamedTempFile;

/// A Hawaiian string mixing the ‘okina (U+2018, WinAnsi-representable) with kahakō
/// vowels (Latin Extended-A, NOT WinAnsi) — so it forces the composite path.
const HAWAIIAN: &str = "La\u{2018}i \u{0101}\u{0113}\u{012B}\u{014D}\u{016B}";

fn save_and_reload(doc: &mut Document) -> Document {
    let tmp = NamedTempFile::new().expect("temp file");
    let path = tmp.path().to_path_buf();
    doc.save(&path).expect("save PDF");
    Document::load(&path).expect("reload PDF")
}

/// Finds the first `/Subtype /Type0` font dictionary in the document.
fn find_type0_font(doc: &Document) -> Option<(ObjectId, &lopdf::Dictionary)> {
    doc.objects.iter().find_map(|(id, obj)| {
        let dict = obj.as_dict().ok()?;
        match dict.get(b"Subtype").ok()?.as_name().ok()? {
            b"Type0" => Some((*id, dict)),
            _ => None,
        }
    })
}

/// Decompressed ToUnicode CMap text of a Type0 font.
fn tounicode_text(doc: &Document, type0: &lopdf::Dictionary) -> String {
    let tu_id = type0.get(b"ToUnicode").unwrap().as_reference().unwrap();
    let stream = doc.get_object(tu_id).unwrap().as_stream().unwrap();
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes).into_owned()
}

fn embedded_hawaiian_font() -> Option<FontData> {
    fixtures::load_system_ttf().map(FontData::Embedded)
}

// --- Composite path ---

#[test]
fn hawaiian_text_embeds_type0_composite_font() {
    let Some(font) = embedded_hawaiian_font() else {
        return;
    };
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    let params = AddTextParams::new(HAWAIIAN, font, "Arial")
        .font_size(24.0)
        .position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect("Hawaiian text with an embedded font should succeed");

    let reloaded = save_and_reload(&mut doc);
    let (_, type0) = find_type0_font(&reloaded).expect("a Type0 font should be embedded");

    // Identity-H encoding on the parent.
    assert_eq!(
        type0.get(b"Encoding").unwrap().as_name().unwrap(),
        b"Identity-H"
    );

    // Descendant is a CIDFontType2 with Identity CIDToGIDMap and a non-empty /W.
    let desc_arr = type0.get(b"DescendantFonts").unwrap();
    let cid_ref = match desc_arr {
        Object::Array(a) => a[0].as_reference().unwrap(),
        Object::Reference(r) => {
            // DescendantFonts may itself be an indirect array
            let inner = reloaded.get_object(*r).unwrap().as_array().unwrap();
            inner[0].as_reference().unwrap()
        }
        _ => panic!("unexpected DescendantFonts type"),
    };
    let cidfont = reloaded.get_object(cid_ref).unwrap().as_dict().unwrap();
    assert_eq!(
        cidfont.get(b"Subtype").unwrap().as_name().unwrap(),
        b"CIDFontType2"
    );
    assert_eq!(
        cidfont.get(b"CIDToGIDMap").unwrap().as_name().unwrap(),
        b"Identity"
    );
    let w = cidfont.get(b"W").unwrap().as_array().unwrap();
    assert!(!w.is_empty(), "/W widths array should be populated");
}

#[test]
fn tounicode_maps_every_non_ascii_char() {
    let Some(font) = embedded_hawaiian_font() else {
        return;
    };
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    let params = AddTextParams::new(HAWAIIAN, font, "Arial").position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let reloaded = save_and_reload(&mut doc);
    let (_, type0) = find_type0_font(&reloaded).unwrap();
    let cmap = tounicode_text(&reloaded, type0);

    assert!(cmap.contains("beginbfchar"), "ToUnicode should have bfchar");
    // Each non-ASCII scalar's UTF-16BE hex must appear as a destination.
    for ch in HAWAIIAN.chars().filter(|c| !c.is_ascii()) {
        let hex = format!("{:04X}", ch as u32);
        assert!(
            cmap.contains(&hex),
            "ToUnicode CMap missing mapping for U+{hex} ('{ch}')"
        );
    }
}

#[test]
fn ascii_text_keeps_simple_winansi_fast_path() {
    let Some(font) = embedded_hawaiian_font() else {
        return;
    };
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    // Pure-CP1252 text must NOT trigger the composite path.
    let params = AddTextParams::new("PERUSAL COPY", font, "Arial").position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert!(
        find_type0_font(&reloaded).is_none(),
        "ASCII text should use a simple WinAnsi font, not Type0"
    );
}

#[test]
fn mixed_page_embeds_both_simple_and_composite() {
    let Some(font) = embedded_hawaiian_font() else {
        return;
    };
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();

    let ascii = AddTextParams::new("DRAFT", font.clone(), "Arial").position(72.0, 700.0);
    add_text_params(&mut doc, page_id, &ascii, &mut cache).unwrap();
    let uni = AddTextParams::new(HAWAIIAN, font, "Arial").position(72.0, 400.0);
    add_text_params(&mut doc, page_id, &uni, &mut cache).unwrap();

    // Same face, two encodings → two cache entries.
    assert_eq!(cache.embedded_entries().count(), 2);
}

// --- Fail-loud behavior ---

#[test]
fn builtin_font_rejects_non_winansi_text() {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    let params = AddTextParams::new(HAWAIIAN, FontData::BuiltIn("Helvetica".into()), "Helvetica");
    let err = add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect_err("built-in font must fail loudly on non-WinAnsi text");
    match err {
        Error::UnrepresentableText { chars, font } => {
            assert!(chars.contains(&'\u{0101}'), "should name the kahakō ā");
            assert_eq!(font, "Helvetica");
        }
        other => panic!("expected UnrepresentableText, got {other:?}"),
    }
}

#[test]
fn builtin_font_accepts_winansi_text() {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    // ‘okina U+2018 and é are inside WinAnsi — the built-in font handles them.
    let params = AddTextParams::new(
        "Caf\u{00E9} \u{2018}okina",
        FontData::BuiltIn("Helvetica".into()),
        "Helvetica",
    );
    assert!(add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).is_ok());
}

#[test]
fn lossy_text_substitutes_for_builtin_font() {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    let params = AddTextParams::new(HAWAIIAN, FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .lossy_text(true);
    assert!(
        add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).is_ok(),
        "lossy_text should restore best-effort substitution"
    );
}

#[test]
fn embedded_font_missing_glyph_fails_loudly() {
    let Some(font) = embedded_hawaiian_font() else {
        return;
    };
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    // A CJK ideograph is absent from a Latin text font.
    let params = AddTextParams::new("Test \u{4E2D}", font, "Arial").h_align(HAlign::Center);
    let err = add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new())
        .expect_err("missing glyph must fail loudly");
    assert!(matches!(err, Error::UnrepresentableText { .. }));
}
