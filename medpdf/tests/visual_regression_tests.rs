mod fixtures;

use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_watermark::{add_line, add_rect, add_text_params};
use medpdf::types::{AddTextParams, DrawLineParams, DrawRectParams, HAlign, PdfColor, VAlign};
use medpdf::pdf_encryption::{encrypt_document, EncryptionAlgorithm, EncryptionParams};
use medpdf::{create_blank_page, subset_fonts, EmbeddedFontCache, FontData};
use pdf_test_visual::{assert_images_ssim, assert_page_matches, rasterize_page_with_password, rasterizer_available, CompareMode};
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

// --- Encryption visual preservation tests ---
//
// Build a PDF with a watermark, rasterize unencrypted as golden,
// then encrypt and verify the encrypted rendering matches.

fn ensure_trailer_id(doc: &mut lopdf::Document) {
    use lopdf::{Object, StringFormat};
    if doc.trailer.get(b"ID").is_err() {
        let id_bytes = b"0123456789abcdef".to_vec();
        doc.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id_bytes.clone(), StringFormat::Literal),
                Object::String(id_bytes, StringFormat::Literal),
            ]),
        );
    }
}

/// Build a one-page PDF with a centered "SAMPLE" watermark for encryption tests.
fn build_watermarked_doc() -> lopdf::Document {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();

    let params = AddTextParams::new("SAMPLE", FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(60.0)
        .position(306.0, 396.0)
        .h_align(HAlign::Center)
        .v_align(VAlign::Center)
        .color(PdfColor::RED);
    add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();
    doc
}

fn visual_encryption_preserves_rendering(algorithm: EncryptionAlgorithm, golden_name: &str) {
    if skip_if_no_rasterizer() {
        return;
    }

    // Save and rasterize the unencrypted version as golden
    let mut doc = build_watermarked_doc();
    let unencrypted_tmp = save_to_temp(&mut doc);
    let golden_path = golden_dir().join(golden_name);
    assert_page_matches(
        unencrypted_tmp.path(),
        1,
        &golden_path,
        CompareMode::default(),
    )
    .unwrap();

    // Now encrypt and save a separate copy
    let mut doc2 = build_watermarked_doc();
    ensure_trailer_id(&mut doc2);
    let enc_params = EncryptionParams::new("user", "owner").algorithm(algorithm);
    encrypt_document(&mut doc2, &enc_params).unwrap();
    let encrypted_tmp = save_to_temp(&mut doc2);

    // Rasterize the encrypted PDF (providing the user password) and compare against the same golden
    let encrypted_png = rasterize_page_with_password(encrypted_tmp.path(), 1, 150, "user").unwrap();
    assert_images_ssim(&golden_path, &encrypted_png, 0.98).unwrap();
}

#[test]
#[ignore = "lopdf AES-256 encryption corrupts content streams — renders blank"]
fn visual_encryption_aes256_preserves_rendering() {
    visual_encryption_preserves_rendering(
        EncryptionAlgorithm::Aes256,
        "encryption-aes256-watermark.png",
    );
}

#[test]
fn visual_encryption_aes128_preserves_rendering() {
    visual_encryption_preserves_rendering(
        EncryptionAlgorithm::Aes128,
        "encryption-aes128-watermark.png",
    );
}

#[test]
fn visual_encryption_rc4_preserves_rendering() {
    visual_encryption_preserves_rendering(
        EncryptionAlgorithm::Rc4_128,
        "encryption-rc4-watermark.png",
    );
}

// --- D1: Subsetted vs non-subsetted rendering should be identical ---
// Builds ONE document, snapshots before subsetting, subsets in-place,
// snapshots again, and compares the two renders.

#[test]
fn visual_subset_vs_nonsubset_identical() {
    if skip_if_no_rasterizer() {
        return;
    }
    let font_data = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("[visual_regression] Skipping: no system TTF font found"); return; }
    };

    // Build a single document with an embedded-font watermark
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut doc, &source_doc, 1).unwrap();
    let mut cache = EmbeddedFontCache::new();
    let params = AddTextParams::new("DRAFT", FontData::Embedded(font_data.clone()), "TestFont")
        .font_size(60.0)
        .position(306.0, 396.0)
        .h_align(HAlign::Center)
        .v_align(VAlign::Center)
        .color(PdfColor::RED);
    add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();

    // Snapshot A: before subsetting
    let tmp_before = save_to_temp(&mut doc);
    let png_before = pdf_test_visual::rasterize_page(tmp_before.path(), 1, 150).unwrap();

    // Subset in-place on the same document
    subset_fonts(&mut doc, &cache).unwrap();

    // Snapshot B: after subsetting
    let tmp_after = save_to_temp(&mut doc);
    let png_after = pdf_test_visual::rasterize_page(tmp_after.path(), 1, 150).unwrap();

    // Compare the two snapshots of the same document
    let before_tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(before_tmp.path(), &png_before).unwrap();

    assert_images_ssim(before_tmp.path(), &png_after, 0.98).unwrap();
}
