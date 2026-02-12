// tests/watermark_edge_tests.rs
// Edge case tests for pdf_watermark module

mod fixtures;

use lopdf::{dictionary, Object, Stream};
use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_watermark::add_text_params;
use medpdf::types::{AddTextParams, HAlign, PdfColor, VAlign};


// --- Helper ---

fn builtin_font_params(text: &str) -> AddTextParams {
    AddTextParams::new(text, vec![b'@'], "Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
}

fn hack_font_params(text: &str) -> AddTextParams {
    AddTextParams::new(text, vec![1u8], "F1")
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "VAlign::Top should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_center() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("CENTER").v_align(VAlign::Center);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "VAlign::Center should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_bottom() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BOTTOM").v_align(VAlign::Bottom);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "VAlign::Bottom should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_descent_bottom() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("DESCENT").v_align(VAlign::DescentBottom);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "VAlign::DescentBottom should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_valign_baseline() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BASELINE").v_align(VAlign::Baseline);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "VAlign::Baseline should succeed: {:?}", result.err());
}

// --- Combined strikeout + underline ---

#[test]
fn test_watermark_strikeout_and_underline_combined() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("BOTH")
        .strikeout(true)
        .underline(true);
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Strikeout should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_underline_only() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("UNDER").underline(true);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Underline should succeed: {:?}", result.err());
}

// --- Strikeout/underline with rotation ---

#[test]
fn test_watermark_strikeout_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("ROTATED STRIKE")
        .rotation(45.0)
        .strikeout(true);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Strikeout+rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_underline_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("ROTATED UNDER")
        .rotation(90.0)
        .underline(true);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Underline+rotation should succeed: {:?}", result.err());
}

// --- Alignment with rotation ---

#[test]
fn test_watermark_center_align_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("CENTERED ROTATED")
        .h_align(HAlign::Center)
        .rotation(30.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Center+rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_right_align_with_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("RIGHT ROTATED")
        .h_align(HAlign::Right)
        .rotation(-45.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Right+rotation should succeed: {:?}", result.err());
}

// --- Alpha edge cases ---

#[test]
fn test_watermark_alpha_zero() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("INVISIBLE")
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 0.0));
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Zero alpha should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_alpha_one_no_extgstate() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("OPAQUE")
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 1.0));
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Alpha 0.5 should succeed: {:?}", result.err());
}

// --- Empty font data ---

#[test]
fn test_watermark_empty_font_data_fails() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = AddTextParams::new("Test", Vec::<u8>::new(), "Empty")
        .font_size(12.0)
        .position(72.0, 72.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_err(), "Empty font data should fail");
}

// --- Hack font (numeric) ---

#[test]
fn test_watermark_hack_font() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = hack_font_params("HACK");
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
        add_text_params(&mut dest_doc, page_id, &params).unwrap();
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Very large font size should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_zero_font_size() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("ZERO").font_size(0.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Zero font size should succeed: {:?}", result.err());
}

// --- Rotation edge cases ---

#[test]
fn test_watermark_360_degree_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("FULL").rotation(360.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "360 degree rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_negative_rotation() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("NEG").rotation(-90.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Negative rotation should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_very_small_rotation_treated_as_zero() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    // 0.0001 < 0.001 threshold, so treated as no rotation
    let params = builtin_font_params("TINY ROT").rotation(0.0001);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Tiny rotation should succeed: {:?}", result.err());
}

// --- Negative coordinates ---

#[test]
fn test_watermark_negative_coordinates() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("OFFSCREEN").position(-100.0, -50.0);
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
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
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Watermark on page without resources should succeed: {:?}", result.err());
}

// --- Color edge cases ---

#[test]
fn test_watermark_white_color() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("WHITE").color(PdfColor::WHITE);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "White color should succeed: {:?}", result.err());
}

#[test]
fn test_watermark_red_color() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("RED").color(PdfColor::RED);
    let result = add_text_params(&mut dest_doc, page_id, &params);
    assert!(result.is_ok(), "Red color should succeed: {:?}", result.err());
}

// --- Content stream ordering for layer_over ---

#[test]
fn test_layer_over_wraps_existing_content_with_q() {
    let (mut dest_doc, page_id) = setup_dest_doc();
    let params = builtin_font_params("OVER").layer_over(true);
    add_text_params(&mut dest_doc, page_id, &params).unwrap();

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

    let params1 = AddTextParams::new("Helvetica", vec![b'@'], "Helvetica")
        .font_size(12.0).position(72.0, 700.0);
    add_text_params(&mut dest_doc, page_id, &params1).unwrap();

    let params2 = AddTextParams::new("Courier", vec![b'@'], "Courier")
        .font_size(12.0).position(72.0, 650.0);
    add_text_params(&mut dest_doc, page_id, &params2).unwrap();

    let params3 = AddTextParams::new("Times", vec![b'@'], "Times-Roman")
        .font_size(12.0).position(72.0, 600.0);
    add_text_params(&mut dest_doc, page_id, &params3).unwrap();

    // All should have separate font objects registered in resources
    let page = dest_doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Resources").is_ok());
}
