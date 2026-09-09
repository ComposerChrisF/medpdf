//! plan-0002 Tier 1: `add_text_params` renders `\n`-separated text as multiple
//! lines, with leading taken from the face's own vertical metrics, horizontal
//! alignment applied per line, and vertical alignment addressing the whole block.
//!
//! Before this, medpdf drew one line and never interpreted a newline: on the
//! WinAnsi path the byte was emitted into the literal string and rendered as
//! nothing, and on the composite path it had no glyph and raised
//! `UnrepresentableText`. pdf-maker's documented `\n` watermark escape therefore
//! decoded correctly and then did nothing (pdf-maker bug-0016).
//!
//! The single-line case must stay byte-for-byte what it was, which is what the
//! first test here pins: for one line every vertical-alignment arm reduces to its
//! pre-Tier-1 value, and no extra operators are emitted.

mod fixtures;

use lopdf::Document;
use lopdf::content::{Content, Operation};
use medpdf::types::{AddTextParams, HAlign, VAlign};
use medpdf::{EmbeddedFontCache, FontData, add_text_params};
use std::sync::Arc;

fn ops_of_page(doc: &Document) -> Vec<Operation> {
    let page_id = fixtures::get_first_page_id(doc);
    let bytes = fixtures::get_page_content_bytes(doc, page_id);
    Content::decode(&bytes).expect("content decodes").operations
}

fn obj_to_f32(obj: &lopdf::Object) -> f32 {
    match obj {
        lopdf::Object::Real(r) => *r,
        lopdf::Object::Integer(i) => *i as f32,
        other => panic!("expected numeric operand, got {other:?}"),
    }
}

/// Draws `params` onto a fresh one-page document and returns its operators.
fn draw(params: &AddTextParams) -> Vec<Operation> {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);
    add_text_params(&mut doc, page_id, params, &mut EmbeddedFontCache::new()).unwrap();
    ops_of_page(&doc)
}

fn td_ops(ops: &[Operation]) -> Vec<&Operation> {
    ops.iter().filter(|op| op.operator == "Td").collect()
}

fn tj_count(ops: &[Operation]) -> usize {
    ops.iter().filter(|op| op.operator == "Tj").count()
}

fn builtin(text: &str) -> AddTextParams {
    AddTextParams::new(text, FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(20.0)
        .position(100.0, 500.0)
}

// --- the single-line contract, unchanged ---

#[test]
fn single_line_emits_exactly_one_td_and_one_tj() {
    for v_align in [
        VAlign::Baseline,
        VAlign::Top,
        VAlign::CapTop,
        VAlign::Center,
        VAlign::Bottom,
        VAlign::DescentBottom,
    ] {
        let ops = draw(&builtin("Hello").v_align(v_align));
        assert_eq!(tj_count(&ops), 1, "one line must draw one Tj ({v_align:?})");
        assert_eq!(
            td_ops(&ops).len(),
            1,
            "one line must emit one Td, no per-line delta ({v_align:?})"
        );
    }
}

#[test]
fn single_line_baseline_position_is_the_requested_point() {
    let ops = draw(&builtin("Hello"));
    let td = td_ops(&ops);
    assert!((obj_to_f32(&td[0].operands[0]) - 100.0).abs() < 0.01);
    assert!((obj_to_f32(&td[0].operands[1]) - 500.0).abs() < 0.01);
}

// --- splitting ---

#[test]
fn newline_splits_text_into_one_tj_per_line() {
    let ops = draw(&builtin("one\ntwo\nthree"));
    assert_eq!(tj_count(&ops), 3, "three lines must draw three Tj");
    assert_eq!(td_ops(&ops).len(), 3, "one Td per line");
}

#[test]
fn crlf_and_lone_cr_are_line_breaks_too() {
    for text in ["a\r\nb", "a\rb", "a\nb"] {
        let ops = draw(&builtin(text));
        assert_eq!(tj_count(&ops), 2, "{text:?} must split into two lines");
    }
}

#[test]
fn a_trailing_newline_makes_a_trailing_empty_line() {
    // Deliberate: "a\n" is two lines, the second empty, exactly as a text editor
    // would show it. Silently swallowing it would make the block height depend on
    // trailing whitespace in a way the caller cannot see.
    let ops = draw(&builtin("a\n"));
    assert_eq!(tj_count(&ops), 2);
}

// --- leading ---

#[test]
fn builtin_font_leading_is_one_point_two_em() {
    // No face to ask, so the conventional 1.2 em fallback: 20pt → 24pt.
    let ops = draw(&builtin("a\nb"));
    let td = td_ops(&ops);
    let ty = obj_to_f32(&td[1].operands[1]);
    assert!(
        (ty - -24.0).abs() < 0.01,
        "second line must sit 24pt below the first, got {ty}"
    );
}

#[test]
fn embedded_font_leading_comes_from_the_face_metrics() {
    let Some(font) = fixtures::load_system_ttf() else {
        eprintln!("no system TTF available; skipping");
        return;
    };
    let face = ttf_parser::Face::parse(&font, 0).unwrap();
    let upem = face.units_per_em() as f32;
    let expected =
        (face.ascender() as f32 - face.descender() as f32 + face.line_gap() as f32) * (20.0 / upem);

    let params = AddTextParams::new("a\nb", FontData::Embedded(Arc::clone(&font)), "Test")
        .font_size(20.0)
        .position(100.0, 500.0);
    let ops = draw(&params);
    let td = td_ops(&ops);
    let ty = obj_to_f32(&td[1].operands[1]);
    assert!(
        (ty - -expected).abs() < 0.01,
        "leading must be the face's ascender − descender + line_gap ({expected}), got {}",
        -ty
    );
}

// --- alignment ---

#[test]
fn horizontal_alignment_is_applied_per_line() {
    // Centered lines of different widths need different offsets; the relative Td
    // carries the difference between the two lines' centering offsets.
    let ops = draw(&builtin("iiii\nWWWWWWWW").h_align(HAlign::Center));
    let td = td_ops(&ops);
    let delta_x = obj_to_f32(&td[1].operands[0]);
    assert!(
        delta_x.abs() > 1.0,
        "a centered wide line must shift relative to a narrow one, got {delta_x}"
    );
}

#[test]
fn left_aligned_lines_share_one_x() {
    let ops = draw(&builtin("iiii\nWWWWWWWW"));
    let td = td_ops(&ops);
    assert!(
        obj_to_f32(&td[1].operands[0]).abs() < 0.01,
        "left alignment must not shift x between lines"
    );
}

#[test]
fn vertical_alignment_addresses_the_block_not_the_first_line() {
    // Top-anchored arms are unaffected by line count; bottom-anchored arms shift by
    // the block's full extra height; Center shifts by half. Leading is 24pt here.
    let first_baseline = |text: &str, v: VAlign| -> f32 {
        let ops = draw(&builtin(text).v_align(v));
        obj_to_f32(&td_ops(&ops)[0].operands[1])
    };

    for anchored_at_top in [VAlign::Baseline, VAlign::Top, VAlign::CapTop] {
        let one = first_baseline("a", anchored_at_top);
        let three = first_baseline("a\nb\nc", anchored_at_top);
        assert!(
            (one - three).abs() < 0.01,
            "{anchored_at_top:?} anchors the first line, so it must not move ({one} vs {three})"
        );
    }

    for anchored_at_bottom in [VAlign::Bottom, VAlign::DescentBottom] {
        let one = first_baseline("a", anchored_at_bottom);
        let three = first_baseline("a\nb\nc", anchored_at_bottom);
        assert!(
            (three - one - 48.0).abs() < 0.01,
            "{anchored_at_bottom:?} anchors the last line, so the first must rise by the \
             block's extra height (2 × 24pt); got {}",
            three - one
        );
    }

    let one = first_baseline("a", VAlign::Center);
    let three = first_baseline("a\nb\nc", VAlign::Center);
    assert!(
        (three - one - 24.0).abs() < 0.01,
        "Center must rise by half the extra height (24pt), got {}",
        three - one
    );
}

// --- decorations ---

#[test]
fn underline_draws_one_rule_per_line() {
    let ops = draw(&builtin("a\nb\nc").underline(true));
    let rects = ops.iter().filter(|op| op.operator == "re").count();
    assert_eq!(rects, 3, "one underline rule per line");
}

#[test]
fn underline_rules_step_down_by_the_leading() {
    let ops = draw(&builtin("a\nb").underline(true));
    let rects: Vec<&Operation> = ops.iter().filter(|op| op.operator == "re").collect();
    let y0 = obj_to_f32(&rects[0].operands[1]);
    let y1 = obj_to_f32(&rects[1].operands[1]);
    assert!(
        (y0 - y1 - 24.0).abs() < 0.01,
        "the second rule must sit one leading lower, got {}",
        y0 - y1
    );
}

// --- the composite path: a newline used to be a hard error ---

#[test]
fn a_newline_no_longer_breaks_the_composite_path() {
    // Text with a character outside CP1252 takes the Type0 path, where a newline had
    // no glyph and raised UnrepresentableText — so a Hawaiian or accented multi-line
    // watermark failed outright rather than merely rendering as one line.
    let Some(font) = fixtures::load_system_ttf() else {
        eprintln!("no system TTF available; skipping");
        return;
    };
    // Pick a character the loaded face actually has, so a missing glyph can never be
    // mistaken for the newline failure this test is about.
    let face = ttf_parser::Face::parse(&font, 0).unwrap();
    let Some(outside_cp1252) = ['\u{02bb}', '\u{0101}', '\u{014d}', '\u{2013}']
        .into_iter()
        .find(|c| face.glyph_index(*c).is_some())
    else {
        eprintln!("system face has no non-CP1252 glyph to test with; skipping");
        return;
    };
    let text = format!("Ka{outside_cp1252}u\nline two");

    let params = AddTextParams::new(text, FontData::Embedded(Arc::clone(&font)), "Test")
        .font_size(20.0)
        .position(100.0, 500.0);

    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);
    let result = add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(
        result.is_ok(),
        "a newline must be a line break on the composite path, not an \
         UnrepresentableText error: {result:?}"
    );
    assert_eq!(tj_count(&ops_of_page(&doc)), 2);
}
