// tests/svg_integration_tests.rs
// Integration tests for medpdf-image SVG support: roundtrip, file loading, mixed content

#![cfg(feature = "svg")]

use lopdf::{dictionary, Document, Object, Stream};
use medpdf_image::{
    add_image, add_svg, load_svg, load_svg_bytes, load_svg_str, DrawImageParams, DrawSvgParams,
    ImageData, ImageFit,
};
use std::io::Write;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Test SVG constants
// ---------------------------------------------------------------------------

/// Landscape SVG: 100x50 user units.
const SIMPLE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
  <rect width="100" height="50" fill="red"/>
</svg>"#;

/// Square SVG: 200x200 user units.
const SQUARE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
  <circle cx="100" cy="100" r="80" fill="blue"/>
</svg>"#;

/// SVG with multiple shapes (more complex content).
const COMPLEX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="200">
  <rect x="10" y="10" width="100" height="80" fill="red" stroke="black" stroke-width="2"/>
  <circle cx="200" cy="100" r="50" fill="blue" opacity="0.5"/>
  <line x1="0" y1="0" x2="300" y2="200" stroke="green" stroke-width="3"/>
  <polygon points="150,10 190,80 110,80" fill="yellow"/>
</svg>"#;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Creates a minimal valid PDF document with one US Letter page.
fn create_one_page_pdf() -> (Document, lopdf::ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![],
        "Count" => 0,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
    let media_box = vec![
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(612.0),
        Object::Real(792.0),
    ];
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);
    let pages_obj = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let kids = pages_obj
        .get_mut(b"Kids")
        .unwrap()
        .as_array_mut()
        .unwrap();
    kids.push(Object::Reference(page_id));
    pages_obj.set("Count", Object::Integer(1));
    (doc, page_id)
}

/// Write bytes to a temp file and return the handle (keeps file alive).
fn write_temp_file(data: &[u8], suffix: &str) -> NamedTempFile {
    let mut tmp = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("create temp file");
    tmp.write_all(data).expect("write temp file");
    tmp.flush().expect("flush temp file");
    tmp
}

/// Save a PDF doc to bytes and reload it.
fn save_and_reload(doc: &mut Document) -> Document {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save PDF");
    Document::load_mem(&buf).expect("reload PDF")
}

// ---------------------------------------------------------------------------
// load_svg from file
// ---------------------------------------------------------------------------

#[test]
fn test_load_svg_from_file() {
    let tmp = write_temp_file(SIMPLE_SVG.as_bytes(), ".svg");

    let svg = load_svg(tmp.path()).unwrap();
    assert!((svg.width - 100.0).abs() < 0.01);
    assert!((svg.height - 50.0).abs() < 0.01);
}

#[test]
fn test_load_svg_from_nonexistent_file() {
    let result = load_svg(std::path::Path::new("/nonexistent/path/test.svg"));
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("Failed to read SVG file"),
        "Error should mention file read failure: {err_msg}"
    );
}

#[test]
fn test_load_svg_from_invalid_file() {
    let tmp = write_temp_file(b"this is not svg content at all", ".svg");
    let result = load_svg(tmp.path());
    assert!(result.is_err());
}

#[test]
fn test_load_svg_from_empty_file() {
    let tmp = write_temp_file(b"", ".svg");
    let result = load_svg(tmp.path());
    assert!(result.is_err());
}

#[test]
fn test_load_svg_from_binary_file() {
    let tmp = write_temp_file(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10], ".svg");
    let result = load_svg(tmp.path());
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Roundtrip: add_svg -> save -> reload -> verify
// ---------------------------------------------------------------------------

#[test]
fn test_add_svg_roundtrip_basic() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 50.0, 100.0, 200.0, 100.0);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Verify XObject exists in reloaded doc
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    assert!(
        res_dict.get(b"XObject").is_ok(),
        "XObject should exist in resources after roundtrip"
    );
}

#[test]
fn test_add_svg_roundtrip_with_rotation() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SQUARE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 100.0, 100.0, 200.0, 200.0).rotation(45.0);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_svg_roundtrip_with_alpha() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 0.0, 0.0, 100.0, 50.0).alpha(0.3);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    assert!(
        res_dict.get(b"ExtGState").is_ok(),
        "ExtGState should survive roundtrip"
    );
}

#[test]
fn test_add_svg_roundtrip_cover_mode() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 0.0, 0.0, 100.0, 100.0).fit(ImageFit::Cover);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_svg_roundtrip_layer_under() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 0.0, 0.0, 200.0, 100.0).layer_over(false);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_svg_roundtrip_complex_svg() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(COMPLEX_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 50.0, 300.0, 400.0, 250.0);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Form XObject should have non-trivial resources (fonts, patterns, etc.)
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    assert!(res_dict.get(b"XObject").is_ok());
}

// ---------------------------------------------------------------------------
// Multiple SVGs on same page
// ---------------------------------------------------------------------------

#[test]
fn test_add_multiple_svgs_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();

    for i in 0..4 {
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(
            svg,
            (i as f32) * 150.0,
            0.0,
            140.0,
            70.0,
        );
        add_svg(&mut doc, page_id, params).unwrap();
    }

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    let xobj_dict = res_dict.get(b"XObject").unwrap().as_dict().unwrap();
    assert!(
        xobj_dict.len() >= 4,
        "Should have at least 4 XObject entries, got {}",
        xobj_dict.len()
    );
}

// ---------------------------------------------------------------------------
// Mixed content: SVG + raster image on same page
// ---------------------------------------------------------------------------

#[test]
fn test_add_svg_and_image_same_page() {
    let (mut doc, page_id) = create_one_page_pdf();

    // Add a raster image first
    let image_data = ImageData::Decoded {
        pixels: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        alpha_channel: None,
        pixel_width: 2,
        pixel_height: 2,
        components: 3,
    };
    let img_params = DrawImageParams::new(image_data, 10.0, 10.0, 100.0, 100.0);
    add_image(&mut doc, page_id, img_params).unwrap();

    // Add an SVG
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let svg_params = DrawSvgParams::new(svg, 200.0, 10.0, 200.0, 100.0);
    add_svg(&mut doc, page_id, svg_params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Both XObjects should exist: Img0 and Svg0
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    let xobj_dict = res_dict.get(b"XObject").unwrap().as_dict().unwrap();
    assert!(
        xobj_dict.get(b"Img0").is_ok(),
        "Should have raster image Img0"
    );
    assert!(xobj_dict.get(b"Svg0").is_ok(), "Should have SVG Svg0");
}

#[test]
fn test_add_image_then_svg_then_image() {
    let (mut doc, page_id) = create_one_page_pdf();

    // Image 1
    let img1 = ImageData::Decoded {
        pixels: vec![100; 3],
        alpha_channel: None,
        pixel_width: 1,
        pixel_height: 1,
        components: 3,
    };
    add_image(&mut doc, page_id, DrawImageParams::new(img1, 0.0, 0.0, 50.0, 50.0)).unwrap();

    // SVG
    let svg = load_svg_str(SQUARE_SVG).unwrap();
    add_svg(
        &mut doc,
        page_id,
        DrawSvgParams::new(svg, 60.0, 0.0, 100.0, 100.0),
    )
    .unwrap();

    // Image 2
    let img2 = ImageData::Decoded {
        pixels: vec![200; 3],
        alpha_channel: None,
        pixel_width: 1,
        pixel_height: 1,
        components: 3,
    };
    add_image(
        &mut doc,
        page_id,
        DrawImageParams::new(img2, 170.0, 0.0, 50.0, 50.0),
    )
    .unwrap();

    let reloaded = save_and_reload(&mut doc);
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    let xobj_dict = res_dict.get(b"XObject").unwrap().as_dict().unwrap();

    // Should have Img0, Svg0, Img1
    assert!(xobj_dict.get(b"Img0").is_ok());
    assert!(xobj_dict.get(b"Svg0").is_ok());
    assert!(xobj_dict.get(b"Img1").is_ok());
    assert_eq!(xobj_dict.len(), 3);
}

// ---------------------------------------------------------------------------
// load_svg_str / load_svg_bytes consistency
// ---------------------------------------------------------------------------

#[test]
fn test_load_svg_str_and_bytes_produce_same_dimensions() {
    let from_str = load_svg_str(SIMPLE_SVG).unwrap();
    let from_bytes = load_svg_bytes(SIMPLE_SVG.as_bytes()).unwrap();

    assert!(
        (from_str.width - from_bytes.width).abs() < f32::EPSILON,
        "Width should match: str={}, bytes={}",
        from_str.width,
        from_bytes.width
    );
    assert!(
        (from_str.height - from_bytes.height).abs() < f32::EPSILON,
        "Height should match: str={}, bytes={}",
        from_str.height,
        from_bytes.height
    );
}

// ---------------------------------------------------------------------------
// Roundtrip: all options combined
// ---------------------------------------------------------------------------

#[test]
fn test_add_svg_roundtrip_all_options() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 50.0, 50.0, 200.0, 200.0)
        .fit(ImageFit::Cover)
        .alpha(0.5)
        .rotation(30.0)
        .layer_over(false)
        .compress(false)
        .svg_dpi(72.0)
        .raster_scale(2.0)
        .embed_text(false);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // ExtGState should exist from alpha
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    assert!(res_dict.get(b"ExtGState").is_ok());
    assert!(res_dict.get(b"XObject").is_ok());
}

// ---------------------------------------------------------------------------
// Form XObject structure survives roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_form_xobject_structure_survives_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let svg = load_svg_str(SIMPLE_SVG).unwrap();
    let params = DrawSvgParams::new(svg, 0.0, 0.0, 200.0, 100.0);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    let xobj_dict = res_dict.get(b"XObject").unwrap().as_dict().unwrap();
    let form_ref = xobj_dict.get(b"Svg0").unwrap().as_reference().unwrap();

    let form_obj = reloaded
        .get_object(form_ref)
        .unwrap()
        .as_stream()
        .unwrap();

    // Verify Type and Subtype survived
    let type_val = form_obj.dict.get(b"Type").unwrap().as_name().unwrap();
    assert_eq!(type_val, b"XObject");
    let subtype_val = form_obj
        .dict
        .get(b"Subtype")
        .unwrap()
        .as_name()
        .unwrap();
    assert_eq!(subtype_val, b"Form");

    // Verify BBox survived
    let bbox = form_obj.dict.get(b"BBox").unwrap().as_array().unwrap();
    assert_eq!(bbox.len(), 4);
}

// ---------------------------------------------------------------------------
// load_svg from file, then add_svg roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_load_svg_file_then_add_roundtrip() {
    let tmp = write_temp_file(COMPLEX_SVG.as_bytes(), ".svg");
    let svg = load_svg(tmp.path()).unwrap();
    assert!(svg.width > 0.0);
    assert!(svg.height > 0.0);

    let (mut doc, page_id) = create_one_page_pdf();
    let params = DrawSvgParams::new(svg, 0.0, 0.0, 400.0, 300.0);
    add_svg(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}
