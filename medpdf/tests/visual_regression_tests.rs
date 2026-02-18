mod fixtures;

use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_watermark::{add_line, add_rect, add_text_params};
use medpdf::types::{AddTextParams, DrawLineParams, DrawRectParams, HAlign, PdfColor, VAlign};
use medpdf::{create_blank_page, EmbeddedFontCache, FontData};
use pdf_test_visual::{assert_page_matches, rasterizer_available, CompareMode};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn save_to_temp(doc: &mut lopdf::Document) -> NamedTempFile {
    let tmp = NamedTempFile::new().expect("create temp file");
    doc.save(tmp.path()).expect("save PDF");
    tmp
}

fn skip_if_no_rasterizer() -> bool {
    if !rasterizer_available() {
        eprintln!("[visual_regression] Skipping: no PDF rasterizer (pdftoppm/mutool) found");
        true
    } else {
        false
    }
}

// --- Test 1: Blank page ---

#[test]
fn visual_blank_page_letter() {
    if skip_if_no_rasterizer() {
        return;
    }
    let mut doc = fixtures::create_empty_pdf();
    create_blank_page(&mut doc, 612.0, 792.0).unwrap();
    let tmp = save_to_temp(&mut doc);

    assert_page_matches(
        tmp.path(),
        1,
        &golden_dir().join("blank-page-letter.png"),
        CompareMode::Exact,
    )
    .unwrap();
}

// --- Test 2: Single watermark (Helvetica, centered) ---

#[test]
fn visual_watermark_centered_helvetica() {
    if skip_if_no_rasterizer() {
        return;
    }
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();

    let params = AddTextParams::new("DRAFT", FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(48.0)
        .position(306.0, 396.0) // center of letter page
        .h_align(HAlign::Center)
        .v_align(VAlign::Center)
        .color(PdfColor::BLACK);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let tmp = save_to_temp(&mut doc);
    assert_page_matches(
        tmp.path(),
        1,
        &golden_dir().join("watermark-centered-helvetica.png"),
        CompareMode::default(),
    )
    .unwrap();
}

// --- Test 3: Watermark with rotation and alpha ---

#[test]
fn visual_watermark_rotated_alpha() {
    if skip_if_no_rasterizer() {
        return;
    }
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();

    let params = AddTextParams::new("CONFIDENTIAL", FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(36.0)
        .position(306.0, 396.0)
        .h_align(HAlign::Center)
        .v_align(VAlign::Center)
        .color(PdfColor::rgba(1.0, 0.0, 0.0, 0.3))
        .rotation(45.0);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let tmp = save_to_temp(&mut doc);
    assert_page_matches(
        tmp.path(),
        1,
        &golden_dir().join("watermark-rotated-alpha.png"),
        CompareMode::default(),
    )
    .unwrap();
}

// --- Test 4: Draw rectangle with color ---

#[test]
fn visual_draw_rect_red() {
    if skip_if_no_rasterizer() {
        return;
    }
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();

    let params = DrawRectParams::new(72.0, 650.0, 468.0, 30.0).color(PdfColor::RED);
    add_rect(&mut doc, page_id, &params).unwrap();

    let tmp = save_to_temp(&mut doc);
    assert_page_matches(
        tmp.path(),
        1,
        &golden_dir().join("draw-rect-red.png"),
        CompareMode::default(),
    )
    .unwrap();
}

// --- Test 5: Draw line ---

#[test]
fn visual_draw_line_blue() {
    if skip_if_no_rasterizer() {
        return;
    }
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();

    let params = DrawLineParams::new(72.0, 720.0, 540.0, 720.0)
        .line_width(3.0)
        .color(PdfColor::rgb(0.0, 0.0, 1.0));
    add_line(&mut doc, page_id, &params).unwrap();

    let tmp = save_to_temp(&mut doc);
    assert_page_matches(
        tmp.path(),
        1,
        &golden_dir().join("draw-line-blue.png"),
        CompareMode::default(),
    )
    .unwrap();
}
