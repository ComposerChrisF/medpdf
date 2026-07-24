// tests/fallback_width_char_count_regression.rs
//
// Regression test for bugs/bug-0011: compute_text_metrics estimated the width of text in
// a non-embedded (built-in) font as `text.len() * font_size * 0.6` — a BYTE count, so
// every non-ASCII (multibyte) character over-measured the string by 0.6 em per extra
// byte, skewing center/right alignment and underline/strikeout length. The sibling
// font_helpers::measure_text_width already used chars().count() with a comment saying
// why; this fallback arm was missed.
//
// The fix uses chars().count(). This test draws "Café" (4 chars, 5 bytes) centered and
// asserts the Td x operand reflects the 4-char width, not the 5-byte width.
//
// To confirm it pins the fix, set MEDPDF_TEMP_BUG0011 (a temporary guard restoring the
// byte count): the assertion then fails (82.0 instead of 85.6).

mod fixtures;

use lopdf::Object;
use lopdf::content::Content;
use medpdf::types::{AddTextParams, HAlign};
use medpdf::{EmbeddedFontCache, FontData, add_text_params};

fn obj_to_f32(obj: &Object) -> f32 {
    match obj {
        Object::Real(r) => *r,
        Object::Integer(i) => *i as f32,
        _ => panic!("expected a numeric operand, got {obj:?}"),
    }
}

#[test]
fn builtin_font_center_uses_char_count_not_byte_count() {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();

    // "Café": 4 characters, 5 UTF-8 bytes (é = U+00E9, WinAnsi-representable so it stays
    // on the simple path). Built-in Helvetica has no embedded face, so the width comes
    // from the 0.6-em fallback.
    let text = "Café";
    assert_eq!(text.chars().count(), 4);
    assert_eq!(text.len(), 5);

    let params = AddTextParams::new(text, FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(12.0)
        .position(100.0, 100.0)
        .h_align(HAlign::Center);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let bytes = fixtures::get_page_content_bytes(&doc, page_id);
    let content = Content::decode(&bytes).expect("decode content stream");
    let td = content
        .operations
        .iter()
        .find(|op| op.operator == "Td")
        .expect("a Td text-positioning op must be present");
    let tx = obj_to_f32(&td.operands[0]);

    // Centered: tx = x - (chars * size * 0.6) / 2 = 100 - (4 * 12 * 0.6)/2 = 85.6.
    // The pre-fix byte count gives 100 - (5 * 12 * 0.6)/2 = 82.0.
    let expected = 100.0 - (4.0 * 12.0 * 0.6) / 2.0;
    assert!(
        (tx - expected).abs() < 0.05,
        "centered Td x must use the 4-char width ({expected}), not the 5-byte width (82.0); \
         got {tx} — bug-0011"
    );
}
