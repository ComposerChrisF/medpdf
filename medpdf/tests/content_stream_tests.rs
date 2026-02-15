// tests/content_stream_tests.rs
// Tests for insert_content_stream, register_extgstate_in_page_resources,
// and overlay + watermark pipeline combinations.

mod fixtures;

use lopdf::{dictionary, Object, Stream};
use medpdf::{
    insert_content_stream, overlay_page, register_extgstate_in_page_resources, EmbeddedFontCache,
};

// ---------------------------------------------------------------------------
// insert_content_stream tests
// ---------------------------------------------------------------------------

#[test]
fn test_insert_content_stream_over_single_ref() {
    // Page has Contents as a single Reference. After insert_over, should become Array.
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let new_stream = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Test) Tj ET".to_vec());
    let new_id = doc.add_object(new_stream);

    insert_content_stream(&mut doc, page_id, new_id, true).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap();
    assert!(
        contents.as_array().is_ok(),
        "Contents should be an Array after insert"
    );
    let arr = contents.as_array().unwrap();
    assert!(arr.len() >= 3, "Should have q, original, Q+, new content");
}

#[test]
fn test_insert_content_stream_under_single_ref() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let new_stream = Stream::new(dictionary! {}, b"0 0 1 rg 0 0 612 792 re f".to_vec());
    let new_id = doc.add_object(new_stream);

    insert_content_stream(&mut doc, page_id, new_id, false).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap();
    let arr = contents.as_array().unwrap();
    // Under mode: new content should be first element
    assert!(arr.len() >= 2, "Should have new content + original");
    // First element should be our new content
    if let Object::Reference(first_id) = &arr[0] {
        assert_eq!(*first_id, new_id, "First content should be the new stream");
    } else {
        panic!("Expected Reference in contents array");
    }
}

#[test]
fn test_insert_content_stream_over_array() {
    // Pre-build a page with an array of content streams
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    // First insert to convert to array
    let first = Stream::new(dictionary! {}, b"% first insert".to_vec());
    let first_id = doc.add_object(first);
    insert_content_stream(&mut doc, page_id, first_id, true).unwrap();

    // Second insert — page already has an array
    let second = Stream::new(dictionary! {}, b"% second insert".to_vec());
    let second_id = doc.add_object(second);
    insert_content_stream(&mut doc, page_id, second_id, true).unwrap();

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    // Should keep growing; new content appended at end
    let last_ref = contents.last().unwrap().as_reference().unwrap();
    assert_eq!(last_ref, second_id, "Latest content should be appended last");
}

#[test]
fn test_insert_content_stream_multiple_layers() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    // Insert 5 layers over
    let mut ids = Vec::new();
    for i in 0..5 {
        let stream = Stream::new(dictionary! {}, format!("% layer {i}").into_bytes());
        let id = doc.add_object(stream);
        insert_content_stream(&mut doc, page_id, id, true).unwrap();
        ids.push(id);
    }

    let page_dict = doc.get_dictionary(page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    // Verify all 5 are present
    for id in &ids {
        assert!(
            contents
                .iter()
                .any(|obj| obj.as_reference().map_or(false, |r| r == *id)),
            "Content stream {:?} should be in contents array",
            id
        );
    }
}

#[test]
fn test_insert_content_stream_preserves_q_balance() {
    // Page with content that has extra q (unbalanced)
    let doc_with_unbalanced = fixtures::create_pdf_with_unbalanced_q(2);
    let _page_id_src = fixtures::get_first_page_id(&doc_with_unbalanced);

    // Copy that page to dest
    let mut dest = fixtures::create_empty_pdf();
    let new_page_id = medpdf::copy_page(&mut dest, &doc_with_unbalanced, 1).unwrap();

    // Insert a content stream over — should auto-close extra q's
    let stream = Stream::new(dictionary! {}, b"% overlay".to_vec());
    let stream_id = dest.add_object(stream);
    insert_content_stream(&mut dest, new_page_id, stream_id, true).unwrap();

    // Read concatenated content and check q/Q balance
    let content_bytes = fixtures::get_page_content_bytes(&dest, new_page_id);
    let (q_count, big_q_count) = fixtures::count_q_operators(&content_bytes);
    assert!(
        q_count <= big_q_count + 1,
        "q/Q should be balanced or nearly balanced: q={q_count}, Q={big_q_count}"
    );
}

// ---------------------------------------------------------------------------
// register_extgstate_in_page_resources tests
// ---------------------------------------------------------------------------

#[test]
fn test_register_extgstate_creates_entry() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let gs_dict = dictionary! {
        "Type" => "ExtGState",
        "ca" => 0.5,
        "CA" => 0.5,
    };
    let gs_id = doc.add_object(gs_dict);

    let gs_key = register_extgstate_in_page_resources(&mut doc, page_id, gs_id).unwrap();

    // Key should be "GS{id}"
    assert!(gs_key.starts_with("GS"), "Key should start with 'GS'");

    // Verify the resource was registered
    let page_dict = doc.get_dictionary(page_id).unwrap();
    let resources = page_dict.get(b"Resources").unwrap();
    let res_id = resources.as_reference().unwrap();
    let res_dict = doc.get_dictionary(res_id).unwrap();
    let extgstate = res_dict.get(b"ExtGState").unwrap().as_dict().unwrap();
    assert!(
        extgstate.get(gs_key.as_bytes()).is_ok(),
        "ExtGState should contain key '{}'",
        gs_key
    );
}

#[test]
fn test_register_extgstate_multiple_entries() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    let page_id = fixtures::get_first_page_id(&doc);

    let mut keys = Vec::new();
    for alpha in [0.3, 0.5, 0.7] {
        let gs_dict = dictionary! {
            "Type" => "ExtGState",
            "ca" => alpha,
        };
        let gs_id = doc.add_object(gs_dict);
        let key = register_extgstate_in_page_resources(&mut doc, page_id, gs_id).unwrap();
        keys.push(key);
    }

    // All keys should be unique
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(unique.len(), 3, "All ExtGState keys should be unique");
}

#[test]
fn test_register_extgstate_on_page_with_no_resources() {
    // Create a page that has no Resources key at all
    let mut doc = fixtures::create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![Object::Real(0.0), Object::Real(0.0), Object::Real(612.0), Object::Real(792.0)],
    };
    let page_id = doc.add_object(page);
    let pages = doc
        .get_object_mut(pages_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    pages
        .get_mut(b"Kids")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(Object::Reference(page_id));
    pages.set("Count", Object::Integer(1));

    let gs_dict = dictionary! { "Type" => "ExtGState", "ca" => 0.5 };
    let gs_id = doc.add_object(gs_dict);

    let result = register_extgstate_in_page_resources(&mut doc, page_id, gs_id);
    assert!(result.is_ok(), "Should create Resources if missing");
}

// ---------------------------------------------------------------------------
// Overlay + watermark pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn test_overlay_then_watermark() {
    let mut dest = fixtures::create_pdf_with_pages(1);
    let dest_page_id = fixtures::get_first_page_id(&dest);

    // Create an overlay source with some content
    let overlay_src = fixtures::create_pdf_with_content(b"0.5 0.5 0.5 rg 50 50 100 100 re f");

    // Apply overlay
    overlay_page(&mut dest, dest_page_id, &overlay_src, 1).unwrap();

    // Apply a watermark (built-in font, no embedding needed)
    let params = medpdf::AddTextParams::new(
        "OVERLAY+WM",
        medpdf::FontData::BuiltIn("Helvetica".into()),
        "@Helvetica",
    )
    .font_size(36.0)
    .position(100.0, 400.0);
    medpdf::add_text_params(&mut dest, dest_page_id, &params, &mut EmbeddedFontCache::new()).unwrap();

    // Verify both operations are reflected in content
    let content = fixtures::get_page_content_bytes(&dest, dest_page_id);
    let content_str = String::from_utf8_lossy(&content);
    assert!(
        content_str.contains("OVERLAY+WM"),
        "Watermark text should be present"
    );
}

#[test]
fn test_double_overlay_same_page() {
    let mut dest = fixtures::create_pdf_with_pages(1);
    let dest_page_id = fixtures::get_first_page_id(&dest);

    let overlay1 = fixtures::create_pdf_with_content(b"1 0 0 rg 0 0 100 100 re f");
    let overlay2 = fixtures::create_pdf_with_content(b"0 0 1 rg 200 200 100 100 re f");

    overlay_page(&mut dest, dest_page_id, &overlay1, 1).unwrap();
    overlay_page(&mut dest, dest_page_id, &overlay2, 1).unwrap();

    // Both overlays should be in the content
    // Both should have added content (hard to check exact operators since resources get renamed)
    let page_dict = dest.get_dictionary(dest_page_id).unwrap();
    let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
    assert!(
        contents.len() >= 3,
        "Should have multiple content stream entries after two overlays"
    );
}

#[test]
fn test_overlay_with_font_resources() {
    let mut dest = fixtures::create_pdf_with_pages(1);
    let dest_page_id = fixtures::get_first_page_id(&dest);

    let overlay_src = fixtures::create_pdf_with_shared_font(1);

    overlay_page(&mut dest, dest_page_id, &overlay_src, 1).unwrap();

    // Destination page should now have font resources
    let page_dict = dest.get_dictionary(dest_page_id).unwrap();
    let res_ref = page_dict
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    let res_dict = dest.get_dictionary(res_ref).unwrap();
    assert!(
        res_dict.get(b"Font").is_ok(),
        "Page should have Font resources after overlay with font"
    );
}
