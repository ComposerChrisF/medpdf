// tests/pdf_helpers_tests.rs
// Tests for public pdf_helpers items (get_page_media_box, Unit)

mod fixtures;

use lopdf::{dictionary, Object, Stream};
use medpdf::pdf_helpers::get_page_media_box;

// --- get_page_media_box Tests ---

#[test]
fn test_get_media_box_explicit_on_page() {
    let doc = fixtures::create_pdf_with_pages_and_size(1, 595.0, 842.0);
    let page_id = fixtures::get_first_page_id(&doc);
    let mb = get_page_media_box(&doc, page_id);
    assert!(mb.is_some());
    let [x0, y0, x1, y1] = mb.unwrap();
    assert!((x0 - 0.0).abs() < 0.01);
    assert!((y0 - 0.0).abs() < 0.01);
    assert!((x1 - 595.0).abs() < 0.01);
    assert!((y1 - 842.0).abs() < 0.01);
}

#[test]
fn test_get_media_box_inherited_from_parent() {
    let doc = fixtures::create_pdf_with_inherited_media_box(2, 612.0, 792.0);
    let pages = doc.get_pages();
    let page_id = *pages.get(&1).expect("Page 1 should exist");
    let mb = get_page_media_box(&doc, page_id);
    assert!(mb.is_some(), "Should inherit MediaBox from parent Pages node");
    let [x0, y0, x1, y1] = mb.unwrap();
    assert!((x0 - 0.0).abs() < 0.01);
    assert!((y0 - 0.0).abs() < 0.01);
    assert!((x1 - 612.0).abs() < 0.01);
    assert!((y1 - 792.0).abs() < 0.01);
}

#[test]
fn test_get_media_box_integer_values() {
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let media_box = vec![
        Object::Integer(0),
        Object::Integer(0),
        Object::Integer(612),
        Object::Integer(792),
    ];
    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);
    let pages = doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    let mb = get_page_media_box(&doc, page_id);
    assert!(mb.is_some());
    let [x0, y0, x1, y1] = mb.unwrap();
    assert!((x0 - 0.0).abs() < 0.01);
    assert!((y0 - 0.0).abs() < 0.01);
    assert!((x1 - 612.0).abs() < 0.01);
    assert!((y1 - 792.0).abs() < 0.01);
}

#[test]
fn test_get_media_box_none_when_missing() {
    let doc = fixtures::create_pdf_without_media_box();
    let pages = doc.get_pages();
    let page_id = *pages.get(&1).expect("Page 1 should exist");
    let mb = get_page_media_box(&doc, page_id);
    assert!(mb.is_none(), "Should return None when no MediaBox in tree");
}

#[test]
fn test_get_media_box_deeply_nested_inheritance() {
    let doc = fixtures::create_pdf_with_nested_page_tree(500.0, 700.0);
    let pages = doc.get_pages();
    let page_id = *pages.get(&1).expect("Page 1 should exist");
    let mb = get_page_media_box(&doc, page_id);
    assert!(
        mb.is_some(),
        "Should inherit MediaBox through nested page tree"
    );
    let [x0, y0, x1, y1] = mb.unwrap();
    assert!((x0 - 0.0).abs() < 0.01);
    assert!((y0 - 0.0).abs() < 0.01);
    assert!((x1 - 500.0).abs() < 0.01);
    assert!((y1 - 700.0).abs() < 0.01);
}

#[test]
fn test_get_media_box_nonzero_origin() {
    let doc = fixtures::create_pdf_with_nonzero_origin_media_box(50.0, 100.0, 662.0, 892.0);
    let page_id = fixtures::get_first_page_id(&doc);
    let mb = get_page_media_box(&doc, page_id);
    assert!(mb.is_some());
    let [x0, y0, x1, y1] = mb.unwrap();
    assert!((x0 - 50.0).abs() < 0.01);
    assert!((y0 - 100.0).abs() < 0.01);
    assert!((x1 - 662.0).abs() < 0.01);
    assert!((y1 - 892.0).abs() < 0.01);
}

#[test]
fn test_get_media_box_invalid_object_id() {
    let doc = fixtures::create_pdf_with_pages(1);
    let bogus_id = (9999, 0);
    let mb = get_page_media_box(&doc, bogus_id);
    assert!(mb.is_none(), "Bogus ID should return None");
}

// --- Unit tests ---

#[test]
fn test_unit_equality() {
    use medpdf::Unit;
    assert_eq!(Unit::In, Unit::In);
    assert_eq!(Unit::Mm, Unit::Mm);
    assert_ne!(Unit::In, Unit::Mm);
}

#[test]
fn test_unit_copy() {
    use medpdf::Unit;
    let u = Unit::In;
    let u2 = u; // Copy
    assert_eq!(u, u2);
}

#[test]
fn test_unit_to_points_zero() {
    use medpdf::Unit;
    assert!((Unit::In.to_points(0.0)).abs() < f32::EPSILON);
    assert!((Unit::Mm.to_points(0.0)).abs() < f32::EPSILON);
}

#[test]
fn test_unit_to_points_negative() {
    use medpdf::Unit;
    let result = Unit::In.to_points(-1.0);
    assert!((result - (-72.0)).abs() < f32::EPSILON);
}

// --- get_page_rotation / set_page_rotation Tests ---

use medpdf::pdf_helpers::{get_page_rotation, set_page_rotation};

#[test]
fn test_get_page_rotation_default_is_zero() {
    let doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);
    assert_eq!(get_page_rotation(&doc, page_id), 0);
}

#[test]
fn test_set_page_rotation_90() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    set_page_rotation(&mut doc, page_id, 90).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 90);
}

#[test]
fn test_set_page_rotation_180() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    set_page_rotation(&mut doc, page_id, 180).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 180);
}

#[test]
fn test_set_page_rotation_270() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    set_page_rotation(&mut doc, page_id, 270).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 270);
}

#[test]
fn test_set_page_rotation_360_normalizes_to_zero() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    set_page_rotation(&mut doc, page_id, 360).unwrap();
    // 360 % 360 == 0, so /Rotate should be removed
    assert_eq!(get_page_rotation(&doc, page_id), 0);
}

#[test]
fn test_set_page_rotation_zero_removes_key() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    // First set a rotation, then set to 0
    set_page_rotation(&mut doc, page_id, 90).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 90);

    set_page_rotation(&mut doc, page_id, 0).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 0);

    // Verify the key is actually removed
    let page = doc.get_dictionary(page_id).unwrap();
    assert!(page.get(b"Rotate").is_err(), "/Rotate key should be removed when set to 0");
}

#[test]
fn test_set_page_rotation_invalid_angle() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let result = set_page_rotation(&mut doc, page_id, 45);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("multiple of 90"), "got: {err}");
}

#[test]
fn test_set_page_rotation_invalid_angle_15() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let result = set_page_rotation(&mut doc, page_id, 15);
    assert!(result.is_err());
}

#[test]
fn test_get_page_rotation_bogus_id() {
    let doc = fixtures::create_pdf_with_pages(1);
    let bogus_id = (9999, 0);
    // Should return 0 (default) for non-existent pages
    assert_eq!(get_page_rotation(&doc, bogus_id), 0);
}

#[test]
fn test_get_page_rotation_inherited_from_parent() {
    // Create a PDF where Rotate is set on the Pages node, not on the page itself
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    // Get the parent Pages node ID
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let parent_id = page_dict.get(b"Parent").unwrap().as_reference().unwrap();

    // Set Rotate on the parent Pages node
    let parent = doc.get_object_mut(parent_id).unwrap().as_dict_mut().unwrap();
    parent.set("Rotate", lopdf::Object::Integer(90));

    // Page should inherit the rotation from parent
    assert_eq!(get_page_rotation(&doc, page_id), 90);
}

#[test]
fn test_set_page_rotation_overwrite() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    set_page_rotation(&mut doc, page_id, 90).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 90);

    set_page_rotation(&mut doc, page_id, 270).unwrap();
    assert_eq!(get_page_rotation(&doc, page_id), 270);
}
