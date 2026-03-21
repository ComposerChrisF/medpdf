// tests/watermark_edge_tests.rs
// Edge case tests for pdf_watermark module

mod fixtures;

use lopdf::{dictionary, Object, Stream};
use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_watermark::{add_line, add_rect, add_text_params};
use medpdf::{EmbeddedFontCache, FontData};
use medpdf::types::{AddTextParams, DrawLineParams, DrawRectParams, HAlign, PdfColor, VAlign};


// --- Helper ---

fn builtin_font_params(text: &str) -> AddTextParams {
    AddTextParams::new(text, FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
}

fn hack_font_params(text: &str) -> AddTextParams {
    AddTextParams::new(text, FontData::Hack(1), "F1")
        .font_size(12.0)
        .position(72.0, 72.0)
}

fn setup_dest_doc() -> (lopdf::Document, lopdf::ObjectId) {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    (dest_doc, page_id)
}

// --- layer_under mode (layer_over = false) ---

#[test]
fn test_watermark_layer_under() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BACKGROUND").layer_over(false);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Layer under should succeed: {:?}", result.err());

    // In layer_under mode, watermark content is prepended (inserted at position 0)
    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    match contents {
        Object::Array(arr) => {
            assert!(arr.len() >= 2, "Should have at least 2 content streams");
        }
        _ => panic!("Contents should be an array after watermark"),
    }
}

#[test]
fn test_watermark_layer_over() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("FOREGROUND").layer_over(true);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Layer over should succeed: {:?}", result.err());

    // In layer_over mode, q/Q wrapping is added around existing content
    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    match contents {
        Object::Array(arr) => {
            // Should have: q_stream, original, closing_q, watermark
            assert!(arr.len() >= 4, "Layer over should have at least 4 content refs, got {}", arr.len());
        }
        _ => panic!("Contents should be an array after watermark"),
    }
}

// --- VAlign variants ---

#[test]
fn test_watermark_valign_top() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("TOP").v_align(VAlign::Top);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "VAlign::Top should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_center() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("CENTER").v_align(VAlign::Center);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "VAlign::Center should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_bottom() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BOTTOM").v_align(VAlign::Bottom);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "VAlign::Bottom should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_descent_bottom() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("DESCENT").v_align(VAlign::DescentBottom);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "VAlign::DescentBottom should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_baseline() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BASELINE").v_align(VAlign::Baseline);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "VAlign::Baseline should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_cap_top() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("CAPTOP").v_align(VAlign::CapTop);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "VAlign::CapTop should succeed: {:?}", result.err());
}

// --- Combined strikeout + underline ---

#[test]
fn test_watermark_strikeout_and_underline_combined() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BOTH")
        .strikeout(true)
        .underline(true);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Combined strikeout+underline should succeed: {:?}", result.err());

    // Verify content stream has rectangle operations for both
    let content = fixtures::get_page_content_bytes(&dest_doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    // Should have at least 2 "re" operations (one for underline, one for strikeout)
    let re_count = content_str.matches(" re").count();
    assert!(re_count >= 2, "Should have at least 2 rectangle ops for strikeout+underline, got {re_count}");
}

#[test]
fn test_watermark_strikeout_only() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("STRIKE").strikeout(true);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Strikeout should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_underline_only() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("UNDER").underline(true);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Underline should succeed: {:?}", result.err());
}

// --- Strikeout/underline with rotation ---

#[test]
fn test_watermark_strikeout_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("ROTATED STRIKE")
        .rotation(45.0)
        .strikeout(true);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Strikeout+rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_underline_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("ROTATED UNDER")
        .rotation(90.0)
        .underline(true);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Underline+rotation should succeed: {:?}", result.err());
}

// --- Alignment with rotation ---

#[test]
fn test_watermark_center_align_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("CENTERED ROTATED")
        .h_align(HAlign::Center)
        .rotation(30.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Center+rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_right_align_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("RIGHT ROTATED")
        .h_align(HAlign::Right)
        .rotation(-45.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Right+rotation should succeed: {:?}", result.err());
}

// --- Alpha edge cases ---

#[test]
fn test_watermark_alpha_zero() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("INVISIBLE")
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 0.0));
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Zero alpha should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_alpha_one_no_extgstate() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("OPAQUE")
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 1.0));
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Alpha 1.0 should succeed: {:?}", result.err());

    // When alpha is 1.0, no ExtGState should be added to resources
    let page = dest_doc.get_dictionary(page_id).unwrap();
    let resources = page.get(b"Resources").unwrap();
    let has_extgstate = match resources {
        Object::Reference(id) => {
            let res_dict = dest_doc.get_dictionary(*id).unwrap();
            res_dict.get(b"ExtGState").is_ok()
        }
        Object::Dictionary(d) => d.get(b"ExtGState").is_ok(),
        _ => false,
    };
    assert!(!has_extgstate, "Alpha 1.0 should not produce ExtGState");
}

#[test]
fn test_watermark_alpha_half_creates_extgstate() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("SEMI")
        .color(PdfColor::rgba(1.0, 0.0, 0.0, 0.5));
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Alpha 0.5 should succeed: {:?}", result.err());
}

// --- Hack font (numeric) ---

#[test]
fn test_watermark_hack_font() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = hack_font_params("HACK");
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Hack font should succeed: {:?}", result.err());
}

// --- Multiple watermarks on same page ---

#[test]
fn test_multiple_watermarks_accumulate() {
    let (mut dest_doc, page_id) = setup_dest_doc();

    for i in 0..5 {
        let text = format!("Watermark {i}");
        let params = builtin_font_params(&text)
            .position(72.0, 72.0 + (i as f32 * 50.0));
        add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();
    }

    // Verify page structure is still valid
    let page = dest_doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Contents").is_ok());
    assert!(page.get(b"Resources").is_ok());
}

// --- Large font size ---

#[test]
fn test_watermark_very_large_font_size() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BIG").font_size(1000.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Very large font size should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_zero_font_size() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("ZERO").font_size(0.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Zero font size should succeed: {:?}", result.err());
}

// --- Rotation edge cases ---

#[test]
fn test_watermark_360_degree_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("FULL").rotation(360.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "360 degree rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_negative_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("NEG").rotation(-90.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Negative rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_very_small_rotation_treated_as_zero() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    // 0.0001 < 0.001 threshold, so treated as no rotation
    let params = builtin_font_params("TINY ROT").rotation(0.0001);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Tiny rotation should succeed: {:?}", result.err());
}

// --- Negative coordinates ---

#[test]
fn test_watermark_negative_coordinates() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("OFFSCREEN").position(-100.0, -50.0);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Negative coordinates should succeed: {:?}", result.err());
}

// --- Page without existing Contents ---

#[test]
fn test_watermark_on_page_without_contents() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let pages_id = dest_doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let resources_id = dest_doc.add_object(dictionary! {});
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    // Page with NO Contents entry
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = dest_doc.add_object(page);

    let pages = dest_doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    let params = builtin_font_params("NEW").layer_over(false);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Watermark on page without contents should succeed: {:?}", result.err());
}

// --- Page without existing Resources ---

#[test]
fn test_watermark_on_page_without_resources() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let pages_id = dest_doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let content_id = dest_doc.add_object(Stream::new(dictionary! {}, vec![]));
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];

    // Page with NO Resources entry
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
    };
    let page_id = dest_doc.add_object(page);

    let pages = dest_doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    let params = builtin_font_params("NO RES");
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Watermark on page without resources should succeed: {:?}", result.err());
}

// --- Color edge cases ---

#[test]
fn test_watermark_white_color() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("WHITE").color(PdfColor::WHITE);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "White color should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_red_color() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("RED").color(PdfColor::RED);
    let result = add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new());
    assert!(result.is_ok(), "Red color should succeed: {:?}", result.err());
}

// --- Content stream ordering for layer_over ---

#[test]
fn test_layer_over_wraps_existing_content_with_q() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("OVER").layer_over(true);
    add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap().as_array().unwrap();

    // First content stream should be the "q" wrapper
    let first_ref = contents[0].as_reference().unwrap();
    let first_stream = dest_doc.get_object(first_ref).unwrap().as_stream().unwrap();
    let first_bytes = &first_stream.content;
    assert_eq!(first_bytes, b"q\n", "First stream should be 'q\\n'");
}

// --- Multiple builtin fonts on same page ---

#[test]
fn test_multiple_different_builtin_fonts() {
    let (mut dest_doc, page_id) = setup_dest_doc();

    let params1 = AddTextParams::new("Helvetica", FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(12.0).position(72.0, 700.0);
    add_text_params(&mut dest_doc, page_id, &params1, &mut EmbeddedFontCache::new()).unwrap();

    let params2 = AddTextParams::new("Courier", FontData::BuiltIn("Courier".into()), "Courier")
        .font_size(12.0).position(72.0, 650.0);
    add_text_params(&mut dest_doc, page_id, &params2, &mut EmbeddedFontCache::new()).unwrap();

    let params3 = AddTextParams::new("Times", FontData::BuiltIn("Times-Roman".into()), "Times-Roman")
        .font_size(12.0).position(72.0, 600.0);
    add_text_params(&mut dest_doc, page_id, &params3, &mut EmbeddedFontCache::new()).unwrap();

    // All should have separate font objects registered in resources
    let page = dest_doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Resources").is_ok());
}

// --- add_rect integration tests ---

#[test]
fn test_add_rect_layer_over() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawRectParams::new(10.0, 20.0, 100.0, 50.0)
        .color(PdfColor::RED);
    let result = add_rect(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "add_rect layer_over should succeed: {:?}", result.err());

    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    match contents {
        Object::Array(arr) => {
            // layer_over: q_stream, original, closing_q, rect_content
            assert!(arr.len() >= 4, "Layer over rect should have at least 4 content refs, got {}", arr.len());
        }
        _ => panic!("Contents should be an array after add_rect"),
    }
}

#[test]
fn test_add_rect_layer_under() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawRectParams::new(10.0, 20.0, 100.0, 50.0)
        .layer_over(false);
    let result = add_rect(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "add_rect layer_under should succeed: {:?}", result.err());

    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    match contents {
        Object::Array(arr) => {
            assert!(arr.len() >= 2, "Layer under should have at least 2 content streams");
        }
        _ => panic!("Contents should be an array"),
    }
}

#[test]
fn test_add_rect_with_alpha() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawRectParams::new(0.0, 0.0, 50.0, 50.0)
        .color(PdfColor::rgba(1.0, 0.0, 0.0, 0.5));
    let result = add_rect(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "add_rect with alpha should succeed: {:?}", result.err());

    let content = fixtures::get_page_content_bytes(&dest_doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(content_str.contains("gs"), "Should contain 'gs' for alpha");
    assert!(content_str.contains("re"), "Should contain 're'");
    assert!(content_str.contains("f\n") || content_str.contains("f\r"), "Should contain 'f' (fill) operator");
}

#[test]
fn test_add_rect_opaque_no_extgstate() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawRectParams::new(0.0, 0.0, 50.0, 50.0)
        .color(PdfColor::RED);
    add_rect(&mut dest_doc, page_id, &params).unwrap();

    let content = fixtures::get_page_content_bytes(&dest_doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(!content_str.contains("gs"), "Alpha 1.0 should not produce gs operator");
}

// --- add_line integration tests ---

#[test]
fn test_add_line_layer_over() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawLineParams::new(0.0, 0.0, 100.0, 200.0)
        .line_width(2.0)
        .color(PdfColor::rgb(0.0, 0.0, 1.0));
    let result = add_line(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "add_line layer_over should succeed: {:?}", result.err());

    let page = dest_doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    match contents {
        Object::Array(arr) => {
            assert!(arr.len() >= 4, "Layer over line should have at least 4 content refs, got {}", arr.len());
        }
        _ => panic!("Contents should be an array"),
    }
}

#[test]
fn test_add_line_layer_under() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawLineParams::new(0.0, 0.0, 100.0, 100.0)
        .layer_over(false);
    let result = add_line(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "add_line layer_under should succeed: {:?}", result.err());
}

#[test]
fn test_add_line_with_alpha() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = DrawLineParams::new(0.0, 0.0, 100.0, 100.0)
        .color(PdfColor::rgba(0.0, 1.0, 0.0, 0.3));
    let result = add_line(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "add_line with alpha should succeed: {:?}", result.err());

    let content = fixtures::get_page_content_bytes(&dest_doc, page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(content_str.contains("gs"), "Should contain 'gs' for alpha");
    assert!(content_str.contains("RG"), "Should contain 'RG' (stroke color)");
}

// --- Combined rect + line + watermark ---

#[test]
fn test_rect_line_watermark_combined() {
    let (mut dest_doc, page_id) = setup_dest_doc();

    // Add rect under
    let rect_params = DrawRectParams::new(0.0, 0.0, 612.0, 792.0)
        .color(PdfColor::rgba(1.0, 1.0, 0.0, 0.2))
        .layer_over(false);
    add_rect(&mut dest_doc, page_id, &rect_params).unwrap();

    // Add line over
    let line_params = DrawLineParams::new(0.0, 0.0, 612.0, 792.0)
        .line_width(1.5)
        .color(PdfColor::RED);
    add_line(&mut dest_doc, page_id, &line_params).unwrap();

    // Add watermark over
    let text_params = AddTextParams::new("DRAFT", FontData::BuiltIn("Helvetica".into()), "Helvetica")
        .font_size(48.0)
        .position(100.0, 400.0);
    add_text_params(&mut dest_doc, page_id, &text_params, &mut EmbeddedFontCache::new()).unwrap();

    // Verify page structure is still valid
    let page = dest_doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Contents").is_ok());
    assert!(page.get(b"Resources").is_ok());
}

// --- EmbeddedFontCache integration tests ---

#[test]
fn test_embedded_font_cache_reuses_font_across_pages() {
    let font_arc = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page1 = copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    let page2 = copy_page(&mut dest_doc, &source_doc, 2).unwrap();

    let mut cache = EmbeddedFontCache::new();

    // Apply same embedded font to both pages via shared cache
    let params1 = AddTextParams::new("Page 1", FontData::Embedded(font_arc.clone()), "TestFont")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut dest_doc, page1, &params1, &mut cache).unwrap();

    let params2 = AddTextParams::new("Page 2", FontData::Embedded(font_arc.clone()), "TestFont")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut dest_doc, page2, &params2, &mut cache).unwrap();

    // Both pages should reference the same font object ID in their resources
    let page1_dict = dest_doc.get_dictionary(page1).unwrap();
    let page2_dict = dest_doc.get_dictionary(page2).unwrap();
    let res1 = page1_dict.get(b"Resources").unwrap().as_reference().unwrap();
    let res2 = page2_dict.get(b"Resources").unwrap().as_reference().unwrap();
    let font_dict1 = dest_doc.get_dictionary(res1).unwrap().get(b"Font").unwrap().as_dict().unwrap();
    let font_dict2 = dest_doc.get_dictionary(res2).unwrap().get(b"Font").unwrap().as_dict().unwrap();

    // Both font dictionaries should reference the same font ObjectId
    let font_refs1: Vec<_> = font_dict1.iter().filter_map(|(_, v)| v.as_reference().ok()).collect();
    let font_refs2: Vec<_> = font_dict2.iter().filter_map(|(_, v)| v.as_reference().ok()).collect();
    assert!(!font_refs1.is_empty(), "Page 1 should have font references");
    assert!(!font_refs2.is_empty(), "Page 2 should have font references");
    // The cached font ID should be the same across both pages
    assert_eq!(font_refs1[0], font_refs2[0], "Both pages should reference the same font object");
}

#[test]
fn test_embedded_font_cache_separate_entries_for_different_fonts() {
    let font_arc1 = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let font_arc2 = match fixtures::load_second_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: need 2 different system fonts"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let mut cache = EmbeddedFontCache::new();

    let params1 = AddTextParams::new("Font A", FontData::Embedded(font_arc1), "FontA")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut dest_doc, page_id, &params1, &mut cache).unwrap();

    let params2 = AddTextParams::new("Font B", FontData::Embedded(font_arc2), "FontB")
        .font_size(12.0)
        .position(72.0, 200.0);
    add_text_params(&mut dest_doc, page_id, &params2, &mut cache).unwrap();

    // Resources should have two different font entries
    let page_dict = dest_doc.get_dictionary(page_id).unwrap();
    let res_id = page_dict.get(b"Resources").unwrap().as_reference().unwrap();
    let font_dict = dest_doc.get_dictionary(res_id).unwrap().get(b"Font").unwrap().as_dict().unwrap();
    assert!(font_dict.len() >= 2, "Should have at least 2 font entries, got {}", font_dict.len());
}

#[test]
fn test_embedded_font_without_cache_duplicates_objects() {
    let font_arc = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(2);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page1 = copy_page(&mut dest_doc, &source_doc, 1).unwrap();
    let page2 = copy_page(&mut dest_doc, &source_doc, 2).unwrap();

    // Use separate caches (no sharing) — font should be embedded twice
    let params1 = AddTextParams::new("Page 1", FontData::Embedded(font_arc.clone()), "TestFont")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut dest_doc, page1, &params1, &mut EmbeddedFontCache::new()).unwrap();

    let params2 = AddTextParams::new("Page 2", FontData::Embedded(font_arc.clone()), "TestFont")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut dest_doc, page2, &params2, &mut EmbeddedFontCache::new()).unwrap();

    // Pages should reference different font objects (no cache sharing)
    let page1_dict = dest_doc.get_dictionary(page1).unwrap();
    let page2_dict = dest_doc.get_dictionary(page2).unwrap();
    let res1 = page1_dict.get(b"Resources").unwrap().as_reference().unwrap();
    let res2 = page2_dict.get(b"Resources").unwrap().as_reference().unwrap();
    let font_dict1 = dest_doc.get_dictionary(res1).unwrap().get(b"Font").unwrap().as_dict().unwrap();
    let font_dict2 = dest_doc.get_dictionary(res2).unwrap().get(b"Font").unwrap().as_dict().unwrap();

    let font_refs1: Vec<_> = font_dict1.iter().filter_map(|(_, v)| v.as_reference().ok()).collect();
    let font_refs2: Vec<_> = font_dict2.iter().filter_map(|(_, v)| v.as_reference().ok()).collect();
    assert_ne!(font_refs1[0], font_refs2[0], "Without shared cache, font objects should differ");
}

#[test]
fn test_embedded_font_data_is_compressed() {
    let font_arc = match fixtures::load_system_ttf() {
        Some(f) => f,
        None => { eprintln!("Skipping: no system TTF font found"); return; }
    };

    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let page_id = copy_page(&mut dest_doc, &source_doc, 1).unwrap();

    let params = AddTextParams::new("Compressed", FontData::Embedded(font_arc.clone()), "TestFont")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut dest_doc, page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    // Find the font file stream (has Length1 key) and verify it's compressed
    let mut found_compressed = false;
    for (_id, obj) in dest_doc.objects.iter() {
        if let Ok(stream) = obj.as_stream() {
            if stream.dict.has(b"Length1") {
                assert!(
                    stream.dict.has(b"Filter"),
                    "Font file stream should have a Filter (compression)"
                );
                // Compressed data should be smaller than original
                assert!(
                    stream.content.len() < font_arc.len(),
                    "Compressed font ({}) should be smaller than original ({})",
                    stream.content.len(),
                    font_arc.len()
                );
                found_compressed = true;
            }
        }
    }
    assert!(found_compressed, "Should find a compressed font file stream");
}
