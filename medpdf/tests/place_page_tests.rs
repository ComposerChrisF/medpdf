mod fixtures;

use fixtures::{
    create_pdf_with_content, create_pdf_with_nonzero_origin_media_box, create_pdf_with_pages,
    create_pdf_without_media_box, get_first_page_id, create_empty_pdf,
};
use lopdf::content::Content;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use medpdf::{place_page, PlacePageParams};

/// Helper: create a source PDF with a font resource and content that uses it.
fn create_source_with_font(font_name: &str) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_obj_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources = dictionary! {
        "Font" => dictionary! {
            font_name => Object::Reference(font_obj_id),
        },
    };
    let resources_id = doc.add_object(resources);

    let content = format!("q\nBT /{font_name} 12 Tf (Hello) Tj ET\nQ\n");
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => Object::Integer(1),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// Helper: collect all operations from all content streams on a page.
fn collect_all_ops(doc: &Document, page_id: ObjectId) -> Vec<lopdf::content::Operation> {
    let page = doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();
    let refs = match contents {
        Object::Array(arr) => arr.clone(),
        Object::Reference(_) => vec![contents.clone()],
        _ => panic!("Unexpected Contents type"),
    };

    let mut ops = Vec::new();
    for r in &refs {
        let id = r.as_reference().unwrap();
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let bytes = if stream.is_compressed() {
            stream.decompressed_content().unwrap_or_else(|_| stream.content.clone())
        } else {
            stream.content.clone()
        };
        if let Ok(content) = Content::decode(&bytes) {
            ops.extend(content.operations);
        }
    }
    ops
}

/// Helper: check if an operation sequence contains a `cm` with specific values.
fn find_cm_ops(ops: &[lopdf::content::Operation]) -> Vec<&lopdf::content::Operation> {
    ops.iter().filter(|op| op.operator == "cm").collect()
}

/// Helper: extract f32 from Object (Real or Integer).
fn obj_to_f32(obj: &Object) -> f32 {
    match obj {
        Object::Real(v) => *v,
        Object::Integer(v) => *v as f32,
        _ => panic!("Expected numeric object, got {:?}", obj),
    }
}

// --- Tests ---

#[test]
fn test_place_page_basic_at_origin() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, 1.0)).unwrap();

    // Should have appended content streams
    let page = dest.get_dictionary(dest_page_id).unwrap();
    let contents = page.get(b"Contents").unwrap().as_array().unwrap();
    // Original content (1) + open transform (1) + source content (1) + close transform (1)
    assert!(contents.len() >= 3, "Expected at least 3 content stream refs, got {}", contents.len());
}

#[test]
fn test_place_page_with_offset() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(306.0, 396.0, 1.0)).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    assert!(!cm_ops.is_empty(), "Should have at least one cm operator");

    // Find the cm with our translate values
    let found = cm_ops.iter().any(|op| {
        op.operands.len() == 6
            && (obj_to_f32(&op.operands[4]) - 306.0).abs() < 0.01
            && (obj_to_f32(&op.operands[5]) - 396.0).abs() < 0.01
    });
    assert!(found, "Should find cm with translate (306, 396)");
}

#[test]
fn test_place_page_with_scaling() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, 0.5)).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    assert!(!cm_ops.is_empty());

    let found = cm_ops.iter().any(|op| {
        op.operands.len() == 6
            && (obj_to_f32(&op.operands[0]) - 0.5).abs() < 0.01
            && (obj_to_f32(&op.operands[3]) - 0.5).abs() < 0.01
    });
    assert!(found, "Should find cm with scale 0.5");
}

#[test]
fn test_place_page_multiple_placements() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let positions = [
        (0.0, 396.0),   // top-left
        (306.0, 396.0),  // top-right
        (0.0, 0.0),      // bottom-left
        (306.0, 0.0),    // bottom-right
    ];

    for (x, y) in &positions {
        place_page(
            &mut dest,
            dest_page_id,
            &source,
            1,
            &PlacePageParams::new(*x, *y, 0.5),
        )
        .unwrap();
    }

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    assert_eq!(cm_ops.len(), 4, "Should have 4 cm operators for 4-up layout");

    // Verify all 4 positions appear
    for (x, y) in &positions {
        let found = cm_ops.iter().any(|op| {
            (obj_to_f32(&op.operands[4]) - *x as f32).abs() < 0.01
                && (obj_to_f32(&op.operands[5]) - *y as f32).abs() < 0.01
        });
        assert!(found, "Should find cm with position ({x}, {y})");
    }
}

#[test]
fn test_place_page_clipping_enabled() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(100.0, 200.0, 0.5)).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);

    // Should have re (clip rect), W (clip), n (no-paint) operators
    let has_re = ops.iter().any(|op| op.operator == "re" && op.operands.len() == 4);
    let has_w = ops.iter().any(|op| op.operator == "W");
    let has_n = ops.iter().any(|op| op.operator == "n");
    assert!(has_re, "Should have clip rectangle (re)");
    assert!(has_w, "Should have clip (W)");
    assert!(has_n, "Should have no-paint path (n)");

    // Verify clip rect dimensions: for 612x792 source at scale 0.5 starting at (100, 200)
    let re_op = ops.iter().find(|op| op.operator == "re" && op.operands.len() == 4).unwrap();
    let re_x = obj_to_f32(&re_op.operands[0]);
    let re_y = obj_to_f32(&re_op.operands[1]);
    let re_w = obj_to_f32(&re_op.operands[2]);
    let re_h = obj_to_f32(&re_op.operands[3]);
    assert!((re_x - 100.0).abs() < 0.01, "Clip x should be 100, got {re_x}");
    assert!((re_y - 200.0).abs() < 0.01, "Clip y should be 200, got {re_y}");
    assert!((re_w - 306.0).abs() < 0.01, "Clip w should be 306 (612*0.5), got {re_w}");
    assert!((re_h - 396.0).abs() < 0.01, "Clip h should be 396 (792*0.5), got {re_h}");
}

#[test]
fn test_place_page_clipping_disabled() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(100.0, 200.0, 0.5).clip(false);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);

    // Should NOT have W (clip) operator in the transform-open stream.
    // The source content itself might have its own re/W operators, so we check
    // that no W operator appears right after a 4-operand re (our clip pattern).
    let has_clip_pattern = ops.windows(3).any(|w| {
        w[0].operator == "re" && w[0].operands.len() == 4
            && w[1].operator == "W"
            && w[2].operator == "n"
    });
    assert!(!has_clip_pattern, "Should NOT have clip re/W/n pattern when clip=false");
}

#[test]
fn test_place_page_resource_renaming() {
    // Source has font "F1", dest also has font "F1"
    let source = create_source_with_font("F1");

    // Create dest with its own "F1" font
    let mut dest = Document::with_version("1.7");
    let pages_id = dest.new_object_id();
    let font_id = dest.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    });
    let resources = dictionary! {
        "Font" => dictionary! {
            "F1" => Object::Reference(font_id),
        },
    };
    let resources_id = dest.add_object(resources);
    let content_id = dest.add_object(Stream::new(dictionary! {}, b"q\nBT /F1 12 Tf ET\nQ\n".to_vec()));
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = dest.add_object(page);
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => Object::Integer(1),
    };
    dest.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = dest.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    dest.trailer.set("Root", catalog_id);

    let dest_page_id = get_first_page_id(&dest);
    place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, 1.0)).unwrap();

    // Check that resources have _p suffix
    let page_dict = dest.get_dictionary(dest_page_id).unwrap();
    let res_ref = page_dict.get(b"Resources").unwrap().as_reference().unwrap();
    let resources = dest.get_dictionary(res_ref).unwrap();
    let fonts = resources.get(b"Font").unwrap().as_dict().unwrap();

    assert!(fonts.has(b"F1"), "Original F1 should remain");
    assert!(fonts.len() >= 2, "Should have at least 2 font entries");

    let has_p_suffix = fonts.iter().any(|(k, _)| {
        let key_str = String::from_utf8_lossy(k);
        key_str.contains("_p")
    });
    assert!(has_p_suffix, "Placed page font should be renamed with _p suffix");
}

#[test]
fn test_place_page_nonzero_media_box_origin() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_nonzero_origin_media_box(50.0, 100.0, 662.0, 892.0);

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(0.0, 0.0, 0.5);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let re_op = ops.iter().find(|op| op.operator == "re" && op.operands.len() == 4).unwrap();

    // clip_x = 0 + 50*0.5 = 25
    // clip_y = 0 + 100*0.5 = 50
    // clip_w = (662-50)*0.5 = 306
    // clip_h = (892-100)*0.5 = 396
    let clip_x = obj_to_f32(&re_op.operands[0]);
    let clip_y = obj_to_f32(&re_op.operands[1]);
    let clip_w = obj_to_f32(&re_op.operands[2]);
    let clip_h = obj_to_f32(&re_op.operands[3]);
    assert!((clip_x - 25.0).abs() < 0.01, "clip_x should be 25, got {clip_x}");
    assert!((clip_y - 50.0).abs() < 0.01, "clip_y should be 50, got {clip_y}");
    assert!((clip_w - 306.0).abs() < 0.01, "clip_w should be 306, got {clip_w}");
    assert!((clip_h - 396.0).abs() < 0.01, "clip_h should be 396, got {clip_h}");
}

#[test]
fn test_place_page_params_builder() {
    let p = PlacePageParams::new(100.0, 200.0, 0.5);
    assert!((p.x - 100.0).abs() < f64::EPSILON);
    assert!((p.y - 200.0).abs() < f64::EPSILON);
    assert!((p.scale - 0.5).abs() < f64::EPSILON);
    assert!((p.rotation - 0.0).abs() < f64::EPSILON, "rotation should default to 0");
    assert!(p.clip, "clip should default to true");

    let p2 = PlacePageParams::new(0.0, 0.0, 1.0).clip(false).rotation(90.0);
    assert!(!p2.clip);
    assert!((p2.rotation - 90.0).abs() < f64::EPSILON);
}

#[test]
fn test_place_page_rotation_90() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(100.0, 200.0, 0.5).rotation(90.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    assert!(!cm_ops.is_empty(), "Should have at least one cm operator");

    // 90°: cm = [0, s, -s, 0, tx, ty]
    let cm = cm_ops.iter().find(|op| {
        op.operands.len() == 6
            && (obj_to_f32(&op.operands[4]) - 100.0).abs() < 0.01
            && (obj_to_f32(&op.operands[5]) - 200.0).abs() < 0.01
    }).expect("Should find cm with translate (100, 200)");
    assert!((obj_to_f32(&cm.operands[0]) - 0.0).abs() < 0.01, "a should be 0");
    assert!((obj_to_f32(&cm.operands[1]) - 0.5).abs() < 0.01, "b should be s");
    assert!((obj_to_f32(&cm.operands[2]) - (-0.5)).abs() < 0.01, "c should be -s");
    assert!((obj_to_f32(&cm.operands[3]) - 0.0).abs() < 0.01, "d should be 0");
}

#[test]
fn test_place_page_rotation_180() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(50.0, 60.0, 1.0).rotation(180.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    let cm = cm_ops.iter().find(|op| {
        op.operands.len() == 6
            && (obj_to_f32(&op.operands[4]) - 50.0).abs() < 0.01
    }).expect("Should find cm with tx=50");
    // 180°: cm = [-s, 0, 0, -s, tx, ty]
    assert!((obj_to_f32(&cm.operands[0]) - (-1.0)).abs() < 0.01, "a should be -s");
    assert!((obj_to_f32(&cm.operands[1]) - 0.0).abs() < 0.01, "b should be 0");
    assert!((obj_to_f32(&cm.operands[2]) - 0.0).abs() < 0.01, "c should be 0");
    assert!((obj_to_f32(&cm.operands[3]) - (-1.0)).abs() < 0.01, "d should be -s");
}

#[test]
fn test_place_page_rotation_270() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(0.0, 0.0, 2.0).rotation(270.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    let cm = cm_ops.iter().find(|op| op.operands.len() == 6)
        .expect("Should find a cm operator");
    // 270°: cm = [0, -s, s, 0, tx, ty]
    assert!((obj_to_f32(&cm.operands[0]) - 0.0).abs() < 0.01, "a should be 0");
    assert!((obj_to_f32(&cm.operands[1]) - (-2.0)).abs() < 0.01, "b should be -s");
    assert!((obj_to_f32(&cm.operands[2]) - 2.0).abs() < 0.01, "c should be s");
    assert!((obj_to_f32(&cm.operands[3]) - 0.0).abs() < 0.01, "d should be 0");
}

#[test]
fn test_place_page_rotation_clip_aabb() {
    // 90° rotation of 612×792 page at scale 0.5, placed at origin
    // Source MediaBox: [0, 0, 612, 792]
    // cm matrix at 90°: a=0, b=0.5, c=-0.5, d=0
    // Corners: (0,0)→(0,0), (612,0)→(0,306), (612,792)→(-396,306), (0,792)→(-396,0)
    // AABB: (-396, 0, 396, 306)
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(0.0, 0.0, 0.5).rotation(90.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let re_op = ops.iter().find(|op| op.operator == "re" && op.operands.len() == 4)
        .expect("Should have clip rect");
    let re_x = obj_to_f32(&re_op.operands[0]);
    let re_y = obj_to_f32(&re_op.operands[1]);
    let re_w = obj_to_f32(&re_op.operands[2]);
    let re_h = obj_to_f32(&re_op.operands[3]);
    assert!((re_x - (-396.0)).abs() < 0.01, "clip x should be -396, got {re_x}");
    assert!((re_y - 0.0).abs() < 0.01, "clip y should be 0, got {re_y}");
    assert!((re_w - 396.0).abs() < 0.01, "clip w should be 396 (792*0.5), got {re_w}");
    assert!((re_h - 306.0).abs() < 0.01, "clip h should be 306 (612*0.5), got {re_h}");
}

#[test]
fn test_place_page_rotation_45() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(100.0, 100.0, 1.0).rotation(45.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    let cm = cm_ops.iter().find(|op| {
        op.operands.len() == 6
            && (obj_to_f32(&op.operands[4]) - 100.0).abs() < 0.01
    }).expect("Should find cm with tx=100");

    // 45°: cos(45°) = sin(45°) = √2/2 ≈ 0.7071
    let sqrt2_2 = std::f32::consts::FRAC_1_SQRT_2;
    assert!((obj_to_f32(&cm.operands[0]) - sqrt2_2).abs() < 0.001, "a should be cos(45°)");
    assert!((obj_to_f32(&cm.operands[1]) - sqrt2_2).abs() < 0.001, "b should be sin(45°)");
    assert!((obj_to_f32(&cm.operands[2]) - (-sqrt2_2)).abs() < 0.001, "c should be -sin(45°)");
    assert!((obj_to_f32(&cm.operands[3]) - sqrt2_2).abs() < 0.001, "d should be cos(45°)");
}

#[test]
fn test_place_page_rotation_negative() {
    // -90° should be equivalent to 270°
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);
    let params = PlacePageParams::new(0.0, 0.0, 1.0).rotation(-90.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let cm_ops = find_cm_ops(&ops);
    let cm = cm_ops.iter().find(|op| op.operands.len() == 6)
        .expect("Should find a cm operator");
    // 270°: cm = [0, -s, s, 0, tx, ty]
    assert!((obj_to_f32(&cm.operands[0]) - 0.0).abs() < 0.01, "a should be 0");
    assert!((obj_to_f32(&cm.operands[1]) - (-1.0)).abs() < 0.01, "b should be -s");
    assert!((obj_to_f32(&cm.operands[2]) - 1.0).abs() < 0.01, "c should be s");
    assert!((obj_to_f32(&cm.operands[3]) - 0.0).abs() < 0.01, "d should be 0");
}

#[test]
fn test_place_page_source_without_contents() {
    let mut dest = create_pdf_with_pages(1);

    // Create a source page with no /Contents
    let mut source = Document::with_version("1.7");
    let pages_id = source.new_object_id();
    let resources_id = source.add_object(dictionary! {});
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = source.add_object(page);
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => Object::Integer(1),
    };
    source.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = source.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    source.trailer.set("Root", catalog_id);

    let dest_page_id = get_first_page_id(&dest);
    let result = place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, 1.0));
    assert!(result.is_ok(), "Should return Ok for source page without /Contents");
}

#[test]
fn test_place_page_no_media_box_errors() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_without_media_box();

    let dest_page_id = get_first_page_id(&dest);
    let result = place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, 1.0));
    assert!(result.is_err(), "Should error when source has no MediaBox");
}

#[test]
fn test_place_page_q_balance() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let dest_page_id = get_first_page_id(&dest);

    // Place 3 pages
    for i in 0..3 {
        place_page(
            &mut dest,
            dest_page_id,
            &source,
            1,
            &PlacePageParams::new(i as f64 * 200.0, 0.0, 0.3),
        )
        .unwrap();
    }

    let ops = collect_all_ops(&dest, dest_page_id);
    let q_count = ops.iter().filter(|op| op.operator == "q").count();
    let big_q_count = ops.iter().filter(|op| op.operator == "Q").count();
    assert_eq!(q_count, big_q_count, "q/Q should be balanced: q={q_count}, Q={big_q_count}");
}

#[test]
fn test_place_page_dest_without_contents() {
    // Create a dest page with no /Contents key
    let mut dest = create_empty_pdf();
    let pages_id = dest
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let resources_id = dest.add_object(dictionary! {});
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = dest.add_object(page);
    let pages = dest.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    place_page(&mut dest, page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, 1.0)).unwrap();

    // Verify content was placed
    let page_dict = dest.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    assert!(!contents.is_empty(), "Should have content streams after placing onto contentless page");
}

#[test]
fn test_place_page_nan_scale_errors() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let dest_page_id = get_first_page_id(&dest);

    let result = place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, f64::NAN));
    assert!(result.is_err(), "NaN scale should error");
}

#[test]
fn test_place_page_infinity_scale_errors() {
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let dest_page_id = get_first_page_id(&dest);

    let result = place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, f64::INFINITY));
    assert!(result.is_err(), "Infinity scale should error");

    let result = place_page(&mut dest, dest_page_id, &source, 1, &PlacePageParams::new(0.0, 0.0, f64::NEG_INFINITY));
    assert!(result.is_err(), "Negative infinity scale should error");
}
