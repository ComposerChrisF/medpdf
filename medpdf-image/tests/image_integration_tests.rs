// tests/image_integration_tests.rs
// Integration tests for medpdf-image: load_image, add_image roundtrip, edge cases

use lopdf::{dictionary, Document, Object, Stream};
use medpdf_image::{add_image, load_image, DrawImageParams, ImageData, ImageFit};
use std::io::Write;
use tempfile::NamedTempFile;

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

/// Creates a minimal valid JPEG file in memory.
/// Returns raw JPEG bytes for a 2x2 pixel image.
fn create_minimal_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_fn(2, 2, |x, y| {
        if (x + y) % 2 == 0 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .expect("encode JPEG");
    buf.into_inner()
}

/// Creates a minimal valid PNG file in memory (RGBA).
fn create_minimal_png_rgba() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(4, 4, |x, y| {
        let alpha = ((x + y) * 40) as u8;
        image::Rgba([100, 150, 200, alpha])
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode PNG");
    buf.into_inner()
}

/// Creates a minimal valid PNG file in memory (RGB, no alpha).
fn create_minimal_png_rgb() -> Vec<u8> {
    let img = image::RgbImage::from_fn(8, 8, |x, y| {
        image::Rgb([(x * 30) as u8, (y * 30) as u8, 128])
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode PNG");
    buf.into_inner()
}

/// Creates a grayscale PNG.
fn create_grayscale_png() -> Vec<u8> {
    let img = image::GrayImage::from_fn(4, 4, |x, y| image::Luma([(x * 60 + y * 20) as u8]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode grayscale PNG");
    buf.into_inner()
}

/// Writes bytes to a temp file and returns the path handle.
fn write_temp_file(data: &[u8], suffix: &str) -> NamedTempFile {
    let mut tmp = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("create temp file");
    tmp.write_all(data).expect("write temp file");
    tmp.flush().expect("flush temp file");
    tmp
}

/// Save PDF doc and reload it from buffer.
fn save_and_reload(doc: &mut Document) -> Document {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save PDF");
    Document::load_mem(&buf).expect("reload PDF")
}

// ---------------------------------------------------------------------------
// load_image tests
// ---------------------------------------------------------------------------

#[test]
fn test_load_image_jpeg() {
    let jpeg_bytes = create_minimal_jpeg();
    let tmp = write_temp_file(&jpeg_bytes, ".jpg");

    let img = load_image(tmp.path()).unwrap();
    assert!(matches!(img, ImageData::Jpeg { .. }));
    assert!(img.pixel_width() > 0);
    assert!(img.pixel_height() > 0);
}

#[test]
fn test_load_image_png_rgb() {
    let png_bytes = create_minimal_png_rgb();
    let tmp = write_temp_file(&png_bytes, ".png");

    let img = load_image(tmp.path()).unwrap();
    match &img {
        ImageData::Decoded {
            alpha_channel,
            pixel_width,
            pixel_height,
            components,
            ..
        } => {
            assert_eq!(*pixel_width, 8);
            assert_eq!(*pixel_height, 8);
            assert_eq!(*components, 3);
            assert!(alpha_channel.is_none());
        }
        _ => panic!("Expected Decoded variant for PNG"),
    }
}

#[test]
fn test_load_image_png_rgba() {
    let png_bytes = create_minimal_png_rgba();
    let tmp = write_temp_file(&png_bytes, ".png");

    let img = load_image(tmp.path()).unwrap();
    match &img {
        ImageData::Decoded {
            alpha_channel,
            pixel_width,
            pixel_height,
            components,
            ..
        } => {
            assert_eq!(*pixel_width, 4);
            assert_eq!(*pixel_height, 4);
            assert_eq!(*components, 3);
            assert!(alpha_channel.is_some(), "RGBA PNG should have alpha channel");
        }
        _ => panic!("Expected Decoded variant for PNG with alpha"),
    }
}

#[test]
fn test_load_image_nonexistent_file() {
    let result = load_image(std::path::Path::new("/nonexistent/path/image.png"));
    assert!(result.is_err());
}

#[test]
fn test_load_image_invalid_data() {
    let tmp = write_temp_file(b"this is not an image", ".png");
    let result = load_image(tmp.path());
    assert!(result.is_err());
}

#[test]
fn test_load_image_empty_file() {
    let tmp = write_temp_file(b"", ".png");
    let result = load_image(tmp.path());
    assert!(result.is_err());
}

#[test]
fn test_load_image_truncated_jpeg() {
    // Just the SOI marker, no SOF
    let tmp = write_temp_file(&[0xFF, 0xD8], ".jpg");
    let result = load_image(tmp.path());
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// ImageData accessor tests
// ---------------------------------------------------------------------------

#[test]
fn test_image_data_jpeg_dimensions() {
    let data = ImageData::Jpeg {
        data: vec![],
        pixel_width: 640,
        pixel_height: 480,
        components: 3,
    };
    assert_eq!(data.pixel_width(), 640);
    assert_eq!(data.pixel_height(), 480);
}

#[test]
fn test_image_data_decoded_dimensions() {
    let data = ImageData::Decoded {
        pixels: vec![0; 100 * 200 * 3],
        alpha_channel: None,
        pixel_width: 100,
        pixel_height: 200,
        components: 3,
    };
    assert_eq!(data.pixel_width(), 100);
    assert_eq!(data.pixel_height(), 200);
}

// ---------------------------------------------------------------------------
// add_image roundtrip tests
// ---------------------------------------------------------------------------

#[test]
fn test_add_image_jpeg_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let jpeg_bytes = create_minimal_jpeg();
    let tmp = write_temp_file(&jpeg_bytes, ".jpg");

    let img = load_image(tmp.path()).unwrap();
    let params = DrawImageParams::new(img, 50.0, 100.0, 200.0, 150.0);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Verify XObject was registered
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
fn test_add_image_png_rgb_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let png_bytes = create_minimal_png_rgb();
    let tmp = write_temp_file(&png_bytes, ".png");

    let img = load_image(tmp.path()).unwrap();
    let params = DrawImageParams::new(img, 0.0, 0.0, 100.0, 100.0);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_image_png_rgba_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let png_bytes = create_minimal_png_rgba();
    let tmp = write_temp_file(&png_bytes, ".png");

    let img = load_image(tmp.path()).unwrap();
    let params = DrawImageParams::new(img, 10.0, 20.0, 150.0, 150.0);
    add_image(&mut doc, page_id, params).unwrap();

    // RGBA should create an SMask
    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_image_with_rotation_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let img = ImageData::Decoded {
        pixels: vec![128; 3 * 4],
        alpha_channel: None,
        pixel_width: 2,
        pixel_height: 2,
        components: 3,
    };
    let params = DrawImageParams::new(img, 100.0, 100.0, 200.0, 200.0).rotation(45.0);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_image_with_alpha_opacity_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let img = ImageData::Decoded {
        pixels: vec![200; 3],
        alpha_channel: None,
        pixel_width: 1,
        pixel_height: 1,
        components: 3,
    };
    let params = DrawImageParams::new(img, 50.0, 50.0, 72.0, 72.0).alpha(0.5);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    // Verify ExtGState exists for alpha
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
        "ExtGState should exist for alpha"
    );
}

// ---------------------------------------------------------------------------
// Multiple images on same page
// ---------------------------------------------------------------------------

#[test]
fn test_add_multiple_images_to_same_page() {
    let (mut doc, page_id) = create_one_page_pdf();

    for i in 0..3 {
        let img = ImageData::Decoded {
            pixels: vec![(i * 80) as u8; 3 * 4],
            alpha_channel: None,
            pixel_width: 2,
            pixel_height: 2,
            components: 3,
        };
        let params =
            DrawImageParams::new(img, 50.0 + (i as f32) * 100.0, 400.0, 80.0, 80.0);
        add_image(&mut doc, page_id, params).unwrap();
    }

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);

    // Should have 3 XObjects registered
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let page_dict = reloaded.get_dictionary(page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = reloaded.get_dictionary(res_ref).unwrap();
    let xobj_dict = res_dict.get(b"XObject").unwrap().as_dict().unwrap();
    assert_eq!(xobj_dict.len(), 3, "Should have 3 XObject entries");
}

// ---------------------------------------------------------------------------
// Fit mode behavior with add_image
// ---------------------------------------------------------------------------

#[test]
fn test_add_image_stretch_fit() {
    let (mut doc, page_id) = create_one_page_pdf();
    let img = ImageData::Decoded {
        pixels: vec![0; 3 * 4],
        alpha_channel: None,
        pixel_width: 2,
        pixel_height: 2,
        components: 3,
    };
    let params = DrawImageParams::new(img, 0.0, 0.0, 300.0, 100.0).fit(ImageFit::Stretch);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

#[test]
fn test_add_image_cover_fit() {
    let (mut doc, page_id) = create_one_page_pdf();
    let img = ImageData::Decoded {
        pixels: vec![255; 3 * 100],
        alpha_channel: None,
        pixel_width: 10,
        pixel_height: 10,
        components: 3,
    };
    // Non-square box with square image -> cover should overflow and clip
    let params = DrawImageParams::new(img, 0.0, 0.0, 200.0, 100.0).fit(ImageFit::Cover);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

// ---------------------------------------------------------------------------
// Layer under
// ---------------------------------------------------------------------------

#[test]
fn test_add_image_layer_under_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let img = ImageData::Decoded {
        pixels: vec![50; 3],
        alpha_channel: None,
        pixel_width: 1,
        pixel_height: 1,
        components: 3,
    };
    let params = DrawImageParams::new(img, 0.0, 0.0, 72.0, 72.0).layer_over(false);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

// ---------------------------------------------------------------------------
// DrawImageParams builder tests
// ---------------------------------------------------------------------------

#[test]
fn test_draw_image_params_defaults() {
    let img = ImageData::Decoded {
        pixels: vec![0; 3],
        alpha_channel: None,
        pixel_width: 1,
        pixel_height: 1,
        components: 3,
    };
    let params = DrawImageParams::new(img, 10.0, 20.0, 100.0, 200.0);
    assert_eq!(params.fit, ImageFit::Contain);
    assert!((params.max_dpi - 300.0).abs() < f32::EPSILON);
    assert!((params.alpha - 1.0).abs() < f32::EPSILON);
    assert!((params.rotation - 0.0).abs() < f32::EPSILON);
    assert!(params.layer_over);
}

#[test]
fn test_draw_image_params_builder_chain() {
    let img = ImageData::Decoded {
        pixels: vec![0; 3],
        alpha_channel: None,
        pixel_width: 1,
        pixel_height: 1,
        components: 3,
    };
    let params = DrawImageParams::new(img, 0.0, 0.0, 72.0, 72.0)
        .fit(ImageFit::Cover)
        .max_dpi(150.0)
        .alpha(0.8)
        .rotation(90.0)
        .layer_over(false);

    assert_eq!(params.fit, ImageFit::Cover);
    assert!((params.max_dpi - 150.0).abs() < f32::EPSILON);
    assert!((params.alpha - 0.8).abs() < f32::EPSILON);
    assert!((params.rotation - 90.0).abs() < f32::EPSILON);
    assert!(!params.layer_over);
}

// ---------------------------------------------------------------------------
// Grayscale image
// ---------------------------------------------------------------------------

#[test]
fn test_add_grayscale_image_roundtrip() {
    let (mut doc, page_id) = create_one_page_pdf();
    let gray_bytes = create_grayscale_png();
    let tmp = write_temp_file(&gray_bytes, ".png");

    let img = load_image(tmp.path()).unwrap();
    let params = DrawImageParams::new(img, 0.0, 0.0, 100.0, 100.0);
    add_image(&mut doc, page_id, params).unwrap();

    let reloaded = save_and_reload(&mut doc);
    assert_eq!(reloaded.get_pages().len(), 1);
}

// ---------------------------------------------------------------------------
// Zero and extreme DPI
// ---------------------------------------------------------------------------

#[test]
fn test_add_image_zero_dpi_no_downsampling() {
    let (mut doc, page_id) = create_one_page_pdf();
    // Large image at small output -> would normally downsample
    let img = ImageData::Decoded {
        pixels: vec![128; 3 * 500 * 500],
        alpha_channel: None,
        pixel_width: 500,
        pixel_height: 500,
        components: 3,
    };
    let params = DrawImageParams::new(img, 0.0, 0.0, 36.0, 36.0).max_dpi(0.0);
    add_image(&mut doc, page_id, params).unwrap();
    // Should succeed without downsampling
}
