// tests/pdf_helpers_tests.rs
// Tests for pdf_helpers module functions

mod fixtures;

use lopdf::{dictionary, Object, Stream};
use medpdf::pdf_helpers::{
    deep_copy_object, deep_copy_object_by_id, get_page_media_box, get_page_object_id_from_doc,
};
use std::collections::BTreeMap;

// --- get_page_object_id_from_doc Tests ---

#[test]
fn test_get_page_object_id_single_page() {
    let doc = fixtures::create_pdf_with_pages(1);
    let result = get_page_object_id_from_doc(&doc, 1);
    assert!(result.is_ok());
    let page_id = result.unwrap();
    // Verify it's actually a page
    let page = doc.get_dictionary(page_id).unwrap();
    assert_eq!(page.get(b"Type").unwrap().as_name().unwrap(), b"Page");
}

#[test]
fn test_get_page_object_id_multiple_pages() {
    let doc = fixtures::create_pdf_with_pages(5);

    for page_num in 1..=5 {
        let result = get_page_object_id_from_doc(&doc, page_num);
        assert!(result.is_ok(), "Failed to get page {}", page_num);
    }
}

#[test]
fn test_get_page_object_id_different_pages_different_ids() {
    let doc = fixtures::create_pdf_with_pages(3);

    let id1 = get_page_object_id_from_doc(&doc, 1).unwrap();
    let id2 = get_page_object_id_from_doc(&doc, 2).unwrap();
    let id3 = get_page_object_id_from_doc(&doc, 3).unwrap();

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

#[test]
fn test_get_page_object_id_page_not_found() {
    let doc = fixtures::create_pdf_with_pages(3);
    let result = get_page_object_id_from_doc(&doc, 4);
    assert!(result.is_err());
}

#[test]
fn test_get_page_object_id_page_zero() {
    let doc = fixtures::create_pdf_with_pages(3);
    // Page 0 doesn't exist (1-indexed)
    let result = get_page_object_id_from_doc(&doc, 0);
    assert!(result.is_err());
}

#[test]
fn test_get_page_object_id_empty_doc() {
    let doc = fixtures::create_empty_pdf();
    let result = get_page_object_id_from_doc(&doc, 1);
    assert!(result.is_err());
}

// --- deep_copy_object Tests ---

#[test]
fn test_deep_copy_integer() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Integer(42);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Object::Integer(42));
}

#[test]
fn test_deep_copy_real() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Real(3.15);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Real(v) = result.unwrap() {
        assert!((v - 3.15).abs() < 0.001);
    } else {
        panic!("Expected Real object");
    }
}

#[test]
fn test_deep_copy_string() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::String(b"Hello".to_vec(), lopdf::StringFormat::Literal);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::String(bytes, _) = result.unwrap() {
        assert_eq!(bytes, b"Hello".to_vec());
    } else {
        panic!("Expected String object");
    }
}

#[test]
fn test_deep_copy_name() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Name(b"TestName".to_vec());
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Name(bytes) = result.unwrap() {
        assert_eq!(bytes, b"TestName".to_vec());
    } else {
        panic!("Expected Name object");
    }
}

#[test]
fn test_deep_copy_boolean_true() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Boolean(true);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Object::Boolean(true));
}

#[test]
fn test_deep_copy_boolean_false() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Boolean(false);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Object::Boolean(false));
}

#[test]
fn test_deep_copy_null() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Null;
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Object::Null);
}

#[test]
fn test_deep_copy_array() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Array(vec![
        Object::Integer(1),
        Object::Integer(2),
        Object::Integer(3),
    ]);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Array(arr) = result.unwrap() {
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], Object::Integer(1));
        assert_eq!(arr[1], Object::Integer(2));
        assert_eq!(arr[2], Object::Integer(3));
    } else {
        panic!("Expected Array object");
    }
}

#[test]
fn test_deep_copy_nested_array() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Array(vec![
        Object::Array(vec![Object::Integer(1), Object::Integer(2)]),
        Object::Array(vec![Object::Integer(3), Object::Integer(4)]),
    ]);
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Array(arr) = result.unwrap() {
        assert_eq!(arr.len(), 2);
        if let Object::Array(inner) = &arr[0] {
            assert_eq!(inner.len(), 2);
        } else {
            panic!("Expected nested array");
        }
    } else {
        panic!("Expected Array object");
    }
}

#[test]
fn test_deep_copy_dictionary() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Dictionary(dictionary! {
        "Key1" => 42,
        "Key2" => "value",
    });
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Dictionary(dict) = result.unwrap() {
        assert_eq!(dict.get(b"Key1").unwrap(), &Object::Integer(42));
    } else {
        panic!("Expected Dictionary object");
    }
}

#[test]
fn test_deep_copy_dictionary_skips_parent() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    // Create a dictionary with a Parent key that should be skipped
    let source_obj = Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference((1, 0)),  // This should be skipped
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Dictionary(dict) = result.unwrap() {
        // Parent should NOT be present in the copy
        assert!(
            dict.get(b"Parent").is_err(),
            "Parent key should be skipped during copy"
        );
        // Other keys should be present
        assert!(dict.get(b"Type").is_ok());
        assert!(dict.get(b"MediaBox").is_ok());
    } else {
        panic!("Expected Dictionary object");
    }
}

#[test]
fn test_deep_copy_stream() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Stream(Stream::new(dictionary! {}, b"q 1 0 0 1 0 0 cm Q".to_vec()));
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_ok());
    if let Object::Stream(stream) = result.unwrap() {
        assert_eq!(stream.content, b"q 1 0 0 1 0 0 cm Q".to_vec());
    } else {
        panic!("Expected Stream object");
    }
}

#[test]
fn test_deep_copy_reference_error() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    // Passing a Reference directly should error - caller must resolve first
    let source_obj = Object::Reference((1, 0));
    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);

    assert!(result.is_err());
}

// --- deep_copy_object_by_id Tests ---

#[test]
fn test_deep_copy_by_id_caches_result() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut source_doc = fixtures::create_empty_pdf();

    // Add an object to the source document
    let source_id = source_doc.add_object(Object::Integer(42));
    let mut copied = BTreeMap::new();

    // First copy
    let result1 = deep_copy_object_by_id(&mut dest_doc, &source_doc, source_id, &mut copied);
    assert!(result1.is_ok());
    let dest_id1 = result1.unwrap();

    // Second copy of the same object should return the cached ID
    let result2 = deep_copy_object_by_id(&mut dest_doc, &source_doc, source_id, &mut copied);
    assert!(result2.is_ok());
    let dest_id2 = result2.unwrap();

    // Should return the same destination ID
    assert_eq!(
        dest_id1, dest_id2,
        "Same source object should map to same dest ID"
    );
}

#[test]
fn test_deep_copy_by_id_different_objects_different_ids() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut source_doc = fixtures::create_empty_pdf();

    let source_id1 = source_doc.add_object(Object::Integer(1));
    let source_id2 = source_doc.add_object(Object::Integer(2));
    let mut copied = BTreeMap::new();

    let dest_id1 =
        deep_copy_object_by_id(&mut dest_doc, &source_doc, source_id1, &mut copied).unwrap();
    let dest_id2 =
        deep_copy_object_by_id(&mut dest_doc, &source_doc, source_id2, &mut copied).unwrap();

    assert_ne!(
        dest_id1, dest_id2,
        "Different source objects should have different dest IDs"
    );
}

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
    // create_pdf_with_pages uses Real values via f32.into(); verify Integer parsing
    // by building a page with explicit Integer MediaBox
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

// --- deep_copy_object with dictionary containing references ---

#[test]
fn test_deep_copy_dictionary_with_reference_values() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut source_doc = fixtures::create_empty_pdf();

    // Add a referenced object to source
    let child_id = source_doc.add_object(Object::Integer(999));

    // Create a dictionary that references the child
    let source_obj = Object::Dictionary(dictionary! {
        "Type" => "Test",
        "Child" => Object::Reference(child_id),
    });
    let mut copied = BTreeMap::new();

    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);
    assert!(result.is_ok());

    if let Object::Dictionary(dict) = result.unwrap() {
        // The Child reference should now point to a new ID in dest_doc
        let child_ref = dict.get(b"Child").unwrap();
        if let Object::Reference(new_id) = child_ref {
            // And the new ID should resolve to the same value
            let obj = dest_doc.get_object(*new_id).unwrap();
            assert_eq!(*obj, Object::Integer(999));
        } else {
            panic!("Child should still be a reference");
        }
    } else {
        panic!("Expected Dictionary");
    }
}

#[test]
fn test_deep_copy_array_with_reference_elements() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut source_doc = fixtures::create_empty_pdf();

    let child_id = source_doc.add_object(Object::Integer(42));

    let source_obj = Object::Array(vec![
        Object::Integer(1),
        Object::Reference(child_id),
        Object::Integer(3),
    ]);
    let mut copied = BTreeMap::new();

    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);
    assert!(result.is_ok());

    if let Object::Array(arr) = result.unwrap() {
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], Object::Integer(1));
        // Second element should be a reference to a copied object
        if let Object::Reference(new_id) = &arr[1] {
            let obj = dest_doc.get_object(*new_id).unwrap();
            assert_eq!(*obj, Object::Integer(42));
        } else {
            panic!("Expected reference in array");
        }
        assert_eq!(arr[2], Object::Integer(3));
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_deep_copy_stream_with_dict_references() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let mut source_doc = fixtures::create_empty_pdf();

    let font_id = source_doc.add_object(dictionary! {
        "Type" => "Font",
        "BaseFont" => "Helvetica",
    });

    let stream_dict = dictionary! {
        "Font" => Object::Reference(font_id),
    };
    let source_obj = Object::Stream(Stream::new(stream_dict, b"test content".to_vec()));
    let mut copied = BTreeMap::new();

    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);
    assert!(result.is_ok());

    if let Object::Stream(stream) = result.unwrap() {
        assert_eq!(stream.content, b"test content");
        let font_ref = stream.dict.get(b"Font").unwrap();
        if let Object::Reference(new_id) = font_ref {
            let font_obj = dest_doc.get_object(*new_id).unwrap();
            assert!(font_obj.as_dict().is_ok());
        } else {
            panic!("Font should be a reference");
        }
    } else {
        panic!("Expected Stream");
    }
}

#[test]
fn test_deep_copy_nested_dictionary() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let source_obj = Object::Dictionary(dictionary! {
        "Outer" => dictionary! {
            "Inner" => 42,
        },
    });

    let result = deep_copy_object(&mut dest_doc, &source_doc, &source_obj, &mut copied);
    assert!(result.is_ok());
    if let Object::Dictionary(outer) = result.unwrap() {
        if let Object::Dictionary(inner) = outer.get(b"Outer").unwrap() {
            assert_eq!(inner.get(b"Inner").unwrap(), &Object::Integer(42));
        } else {
            panic!("Expected nested dictionary");
        }
    } else {
        panic!("Expected Dictionary");
    }
}

// --- get_page_media_box with non-zero origin ---

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

// --- get_page_media_box with invalid object ID ---

#[test]
fn test_get_media_box_invalid_object_id() {
    let doc = fixtures::create_pdf_with_pages(1);
    let bogus_id = (9999, 0);
    let mb = get_page_media_box(&doc, bogus_id);
    assert!(mb.is_none(), "Bogus ID should return None");
}

// --- deep_copy_object_by_id with missing source object ---

#[test]
fn test_deep_copy_by_id_missing_source_object() {
    let mut dest_doc = fixtures::create_empty_pdf();
    let source_doc = fixtures::create_empty_pdf();
    let mut copied = BTreeMap::new();

    let bogus_id = (9999, 0);
    let result = deep_copy_object_by_id(&mut dest_doc, &source_doc, bogus_id, &mut copied);
    assert!(result.is_err(), "Missing source object should fail");
}

// --- Unit conversion tests (already covered in unit_conversion_tests.rs but adding edge cases) ---

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
