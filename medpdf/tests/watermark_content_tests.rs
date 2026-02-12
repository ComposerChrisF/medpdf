// tests/watermark_content_tests.rs
// Tests that verify actual content stream operators produced by add_text_params, add_rect, add_line.
// These go beyond checking is_ok() to verify the PDF operators are correct.

mod fixtures;

use medpdf::types::{AddTextParams, DrawLineParams, DrawRectParams, HAlign, PdfColor, VAlign};
use medpdf::{add_line, add_rect, add_text_params};
use medpdf::FontData;

/// Helper: extract all content stream bytes from the first page.
fn get_first_page_content(doc: &lopdf::Document) -> String {
    let page_id = fixtures::get_first_page_id(doc);
    let bytes = fixtures::get_page_content_bytes(doc, page_id);
    String::from_utf8_lossy(&bytes).into_owned()
}

// --- add_text_params: Content Stream Operators ---

#[test]
fn test_watermark_contains_q_and_big_q() {
    // Every watermark should be wrapped in a graphics state save/restore
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Test", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    let (q_count, big_q_count) = fixtures::count_q_operators(content.as_bytes());
    assert!(q_count >= 1, "Should have at least one 'q' operator, got {q_count}");
    assert!(big_q_count >= 1, "Should have at least one 'Q' operator, got {big_q_count}");
}

#[test]
fn test_watermark_contains_bt_et() {
    // Text operations must be between BT and ET
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Hello", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(100.0, 200.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("BT"), "Should contain BT (begin text)");
    assert!(content.contains("ET"), "Should contain ET (end text)");
}

#[test]
fn test_watermark_contains_tf_operator() {
    // Font selection: /FontKey size Tf
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Font test", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(36.0)
        .position(72.0, 72.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("Tf"), "Should contain Tf (set font)");
    assert!(content.contains("36"), "Should contain font size 36");
}

#[test]
fn test_watermark_contains_tj_with_text() {
    // The text should appear as a Tj operand
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("SAMPLE", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(0.0, 0.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("Tj"), "Should contain Tj (show text)");
    assert!(content.contains("SAMPLE"), "Should contain the text 'SAMPLE'");
}

#[test]
fn test_watermark_contains_td_position() {
    // Non-rotated text uses Td for positioning
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let x = 150.0_f32;
    let y = 300.0_f32;
    let params = AddTextParams::new("Positioned", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(x, y);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("Td"), "Should contain Td (text position)");
    // For left-aligned, baseline text, the coordinates should be exactly x, y
    assert!(content.contains("150"), "Should contain x=150");
    assert!(content.contains("300"), "Should contain y=300");
}

#[test]
fn test_watermark_color_rg_operator() {
    // Color is set with "r g b rg"
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Red text", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .color(PdfColor::RED);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("rg"), "Should contain 'rg' (set fill color)");
    // Red = (1, 0, 0)
    assert!(content.contains("1 0 0 rg"), "Should contain '1 0 0 rg' for red color");
}

#[test]
fn test_watermark_custom_color() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    // Use a distinctive color
    let params = AddTextParams::new("Custom", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .color(PdfColor::rgb(0.0, 1.0, 0.0)); // Green
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("0 1 0 rg"), "Should contain '0 1 0 rg' for green color");
}

// --- Alpha / ExtGState ---

#[test]
fn test_watermark_alpha_emits_gs_operator() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Transparent", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 0.5));
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("gs"), "Should contain 'gs' operator for alpha < 1.0");
}

#[test]
fn test_watermark_full_alpha_no_gs_operator() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    // Alpha = 1.0 (fully opaque): should NOT emit gs
    let params = AddTextParams::new("Opaque", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .color(PdfColor::rgba(0.0, 0.0, 0.0, 1.0));
    add_text_params(&mut doc, page_id, &params).unwrap();

    // Check the watermark stream specifically (last content stream added)
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    // The last reference is our watermark stream
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let watermark_content = String::from_utf8_lossy(&stream.content);
    assert!(!watermark_content.contains("gs"), "Opaque text should not emit gs operator");
}

// --- Rotation ---

#[test]
fn test_watermark_rotation_emits_cm_operator() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Rotated", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(100.0, 200.0)
        .rotation(45.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("cm"), "Rotated text should contain 'cm' (concat matrix)");
}

#[test]
fn test_watermark_rotation_matrix_values() {
    // 45 degrees: cos(45) = sin(45) = ~0.7071
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let x = 100.0_f32;
    let y = 200.0_f32;
    let params = AddTextParams::new("R45", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(x, y)
        .rotation(45.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // Extract the watermark content stream (last one added)
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let watermark_content = String::from_utf8_lossy(&stream.content);

    assert!(watermark_content.contains("cm"), "Should have cm operator");
    // The cm operands should include cos(45deg) ≈ 0.7071
    assert!(
        watermark_content.contains("0.7071") || watermark_content.contains("0.70710"),
        "cm should contain cos(45°) ≈ 0.7071, got: {}",
        watermark_content
    );
}

#[test]
fn test_watermark_no_rotation_no_cm() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("NoRot", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .rotation(0.0);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // Check the watermark stream specifically
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let watermark_content = String::from_utf8_lossy(&stream.content);
    assert!(!watermark_content.contains("cm"), "Non-rotated text should not have cm operator");
}

// --- Horizontal Alignment ---

#[test]
fn test_watermark_center_align_offsets_position() {
    // Center alignment should shift x by -textWidth/2
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let x = 300.0_f32;
    let params = AddTextParams::new("Center", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(x, 400.0)
        .h_align(HAlign::Center);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // The Td x-coordinate should be less than 300 (shifted left by half text width)
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let td_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "Td")
        .collect();
    assert!(!td_ops.is_empty(), "Should have Td operator");

    // For center alignment, Td x should be x - textWidth/2 (i.e. less than x=300)
    if let Some(td) = td_ops.first() {
        let td_x = td.operands[0].as_float().unwrap();
        assert!(
            td_x < x,
            "Center-aligned Td x ({td_x}) should be less than position x ({x})"
        );
    }
}

#[test]
fn test_watermark_right_align_offsets_position() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let x = 500.0_f32;
    let params = AddTextParams::new("Right", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(x, 400.0)
        .h_align(HAlign::Right);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let td_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "Td")
        .collect();

    if let Some(td) = td_ops.first() {
        let td_x = td.operands[0].as_float().unwrap();
        assert!(
            td_x < x,
            "Right-aligned Td x ({td_x}) should be less than position x ({x})"
        );
    }
}

#[test]
fn test_watermark_left_align_exact_position() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let x = 72.0_f32;
    let y = 720.0_f32;
    let params = AddTextParams::new("Left", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(x, y)
        .h_align(HAlign::Left)
        .v_align(VAlign::Baseline);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let td_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "Td")
        .collect();

    if let Some(td) = td_ops.first() {
        let td_x = td.operands[0].as_float().unwrap();
        let td_y = td.operands[1].as_float().unwrap();
        assert!(
            (td_x - x).abs() < 0.01,
            "Left-aligned baseline Td x ({td_x}) should be exactly x ({x})"
        );
        assert!(
            (td_y - y).abs() < 0.01,
            "Baseline Td y ({td_y}) should be exactly y ({y})"
        );
    }
}

// --- Vertical Alignment ---

#[test]
fn test_watermark_valign_top_shifts_down() {
    // VAlign::Top should shift y downward (negative dy = -ascent)
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let y = 700.0_f32;
    let params = AddTextParams::new("TopAlign", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, y)
        .v_align(VAlign::Top);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let td_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "Td")
        .collect();

    if let Some(td) = td_ops.first() {
        let td_y = td.operands[1].as_float().unwrap();
        // VAlign::Top uses dy = -ascent_scaled (negative), so final y < original y
        assert!(
            td_y < y,
            "Top-aligned Td y ({td_y}) should be less than position y ({y})"
        );
    }
}

#[test]
fn test_watermark_valign_center_adjusts_y() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let y = 400.0_f32;
    let params = AddTextParams::new("CenterV", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, y)
        .v_align(VAlign::Center);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let td_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "Td")
        .collect();

    if let Some(td) = td_ops.first() {
        let td_y = td.operands[1].as_float().unwrap();
        // VAlign::Center uses dy = -x_height/2 (negative), so final y < original y
        assert!(
            td_y < y,
            "Center-aligned Td y ({td_y}) should be less than position y ({y})"
        );
    }
}

// --- add_rect: Content Stream Operators ---

#[test]
fn test_add_rect_operators() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = DrawRectParams::new(10.0, 20.0, 100.0, 50.0);
    add_rect(&mut doc, page_id, &params).unwrap();

    // Decode the rect content stream (last one added) and check operators
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();
    let ops: Vec<&str> = decoded.operations.iter().map(|o| o.operator.as_str()).collect();

    assert!(ops.contains(&"rg"), "Should set fill color with rg");
    assert!(ops.contains(&"re"), "Should draw rectangle with re");
    assert!(ops.contains(&"f"), "Should fill with f");
    assert!(ops.contains(&"q"), "Should save graphics state with q");
    assert!(ops.contains(&"Q"), "Should restore graphics state with Q");
}

#[test]
fn test_add_rect_coordinates_in_stream() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = DrawRectParams::new(10.0, 20.0, 100.0, 50.0);
    add_rect(&mut doc, page_id, &params).unwrap();

    // Extract the rect content stream
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let re_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "re")
        .collect();
    assert_eq!(re_ops.len(), 1, "Should have exactly one re operator");

    let re = re_ops[0];
    let rx = re.operands[0].as_float().unwrap();
    let ry = re.operands[1].as_float().unwrap();
    let rw = re.operands[2].as_float().unwrap();
    let rh = re.operands[3].as_float().unwrap();
    assert!((rx - 10.0).abs() < 0.01);
    assert!((ry - 20.0).abs() < 0.01);
    assert!((rw - 100.0).abs() < 0.01);
    assert!((rh - 50.0).abs() < 0.01);
}

#[test]
fn test_add_rect_color() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = DrawRectParams::new(0.0, 0.0, 50.0, 50.0).color(PdfColor::rgb(0.0, 0.0, 1.0));
    add_rect(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    assert!(
        content.contains("0 0 1 rg"),
        "Should set blue fill color '0 0 1 rg'"
    );
}

#[test]
fn test_add_rect_with_alpha_has_extgstate() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params =
        DrawRectParams::new(0.0, 0.0, 50.0, 50.0).color(PdfColor::rgba(1.0, 0.0, 0.0, 0.5));
    add_rect(&mut doc, page_id, &params).unwrap();

    // Check the page has an ExtGState resource
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let resources = page_dict.get(b"Resources").unwrap();
    // Resources might be a reference
    let resources_dict = match resources {
        lopdf::Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        lopdf::Object::Dictionary(d) => d,
        _ => panic!("Unexpected Resources type"),
    };
    let extgstate = resources_dict.get(b"ExtGState");
    assert!(extgstate.is_ok(), "Should have ExtGState resource for alpha rect");
}

// --- add_line: Content Stream Operators ---

#[test]
fn test_add_line_operators() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = DrawLineParams::new(0.0, 0.0, 200.0, 300.0);
    add_line(&mut doc, page_id, &params).unwrap();

    // Decode the line content stream (last one added) and check operators
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();
    let ops: Vec<&str> = decoded.operations.iter().map(|o| o.operator.as_str()).collect();

    assert!(ops.contains(&"RG"), "Should set stroke color with RG");
    assert!(ops.contains(&"w"), "Should set line width with w");
    assert!(ops.contains(&"m"), "Should move to start with m");
    assert!(ops.contains(&"l"), "Should draw line with l");
    assert!(ops.contains(&"S"), "Should stroke with S");
}

#[test]
fn test_add_line_coordinates() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = DrawLineParams::new(10.0, 20.0, 300.0, 400.0);
    add_line(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    // Check m (moveto) operands
    let m_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "m")
        .collect();
    assert_eq!(m_ops.len(), 1, "Should have exactly one m operator");
    let m_x = m_ops[0].operands[0].as_float().unwrap();
    let m_y = m_ops[0].operands[1].as_float().unwrap();
    assert!((m_x - 10.0).abs() < 0.01);
    assert!((m_y - 20.0).abs() < 0.01);

    // Check l (lineto) operands
    let l_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "l")
        .collect();
    assert_eq!(l_ops.len(), 1, "Should have exactly one l operator");
    let l_x = l_ops[0].operands[0].as_float().unwrap();
    let l_y = l_ops[0].operands[1].as_float().unwrap();
    assert!((l_x - 300.0).abs() < 0.01);
    assert!((l_y - 400.0).abs() < 0.01);
}

#[test]
fn test_add_line_custom_width_value() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = DrawLineParams::new(0.0, 0.0, 100.0, 100.0).line_width(5.0);
    add_line(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let w_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "w")
        .collect();
    assert_eq!(w_ops.len(), 1, "Should have exactly one w operator");
    let width = w_ops[0].operands[0].as_float().unwrap();
    assert!((width - 5.0).abs() < 0.01, "Line width should be 5.0, got {width}");
}

#[test]
fn test_add_line_stroke_color() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params =
        DrawLineParams::new(0.0, 0.0, 100.0, 100.0).color(PdfColor::rgb(0.0, 1.0, 0.0));
    add_line(&mut doc, page_id, &params).unwrap();

    let content = get_first_page_content(&doc);
    // RG is stroke color (uppercase), vs rg which is fill color
    assert!(
        content.contains("0 1 0 RG"),
        "Should set green stroke color '0 1 0 RG'"
    );
}

// --- Layer Over vs Under ---

#[test]
fn test_watermark_layer_over_appends() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Over", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .layer_over(true);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    // layer_over=true: [q, original, Q(s), watermark]
    assert!(
        contents.len() >= 3,
        "Layer over should produce at least 3 content stream entries"
    );
}

#[test]
fn test_watermark_layer_under_prepends() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Under", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(12.0)
        .position(72.0, 72.0)
        .layer_over(false);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    // layer_over=false: [watermark, original]
    assert!(
        contents.len() >= 2,
        "Layer under should produce at least 2 content stream entries"
    );
}

// --- Multiple Watermarks ---

#[test]
fn test_multiple_watermarks_on_same_page() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params1 = AddTextParams::new("FIRST", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, 720.0);
    let params2 = AddTextParams::new("SECOND", FontData::BuiltIn("Helvetica".into()), "@Courier")
        .font_size(18.0)
        .position(72.0, 600.0);

    add_text_params(&mut doc, page_id, &params1).unwrap();
    add_text_params(&mut doc, page_id, &params2).unwrap();

    let content = get_first_page_content(&doc);
    assert!(content.contains("FIRST"), "Should contain first watermark");
    assert!(content.contains("SECOND"), "Should contain second watermark");
}

// --- Underline / Strikeout ---

#[test]
fn test_watermark_underline_emits_re_f() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Underlined", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, 400.0)
        .underline(true);
    add_text_params(&mut doc, page_id, &params).unwrap();

    // The watermark stream should contain re + f for the underline rectangle
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let re_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "re")
        .collect();
    assert!(
        !re_ops.is_empty(),
        "Underlined text should emit 're' for the underline rectangle"
    );
}

#[test]
fn test_watermark_strikeout_emits_re_f() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Struck", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, 400.0)
        .strikeout(true);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let re_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "re")
        .collect();
    assert!(
        !re_ops.is_empty(),
        "Strikeout text should emit 're' for the strikeout rectangle"
    );
}

#[test]
fn test_watermark_both_underline_and_strikeout() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let params = AddTextParams::new("Both", FontData::BuiltIn("Helvetica".into()), "@Helvetica")
        .font_size(24.0)
        .position(72.0, 400.0)
        .underline(true)
        .strikeout(true);
    add_text_params(&mut doc, page_id, &params).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    let stream = doc.get_object(last_ref).unwrap().as_stream().unwrap();
    let decoded = stream.decode_content().unwrap();

    let re_ops: Vec<_> = decoded
        .operations
        .iter()
        .filter(|op| op.operator == "re")
        .collect();
    assert_eq!(
        re_ops.len(),
        2,
        "Both underline + strikeout should emit 2 're' operators, got {}",
        re_ops.len()
    );
}
