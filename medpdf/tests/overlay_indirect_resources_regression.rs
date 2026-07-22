// tests/overlay_indirect_resources_regression.rs
//
// Regression tests for bugs/bug-0030: a resource-type sub-dictionary held as an
// indirect reference (`/Font 10 0 R`, which Acrobat emits routinely) was
// mishandled on every path — the rename pass skipped it, the merge dropped it or
// errored, and the collision scan was blind to names inside it.
//
// The fix normalizes indirect source sub-dicts to inline (so rename + merge work),
// dereferences references in the collision scan, and dereferences an indirect
// *destination* sub-dict during the merge instead of erroring on it.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use medpdf::pdf_overlay::overlay_page;
use medpdf::{PlacePageParams, place_page};

/// A Type1 font object with an explicit `/BaseFont` name.
fn add_font(doc: &mut Document, base: &str) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(base.as_bytes().to_vec()),
    })
}

/// Builds a one-page doc. `build` produces the page's `/Resources` value (an inline
/// `Object::Dictionary` or an `Object::Reference`); `content` is its single stream.
fn doc_with_resources(
    build: impl FnOnce(&mut Document) -> Object,
    content: &[u8],
) -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let resources = build(&mut doc);
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => resources,
    });

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    (doc, page_id)
}

fn deref_dict(doc: &Document, obj: &Object) -> Dictionary {
    match obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
        other => panic!("expected dictionary or reference, got {other:?}"),
    }
}

/// The page's `/Font` sub-dictionary, dereferencing `/Resources` and `/Font` as needed.
fn page_font_dict(doc: &Document, page_id: ObjectId) -> Dictionary {
    let page = doc.get_dictionary(page_id).unwrap();
    let resources = deref_dict(doc, page.get(b"Resources").unwrap());
    deref_dict(doc, resources.get(b"Font").unwrap())
}

/// The `/BaseFont` name of the font stored under `key` in a `/Font` sub-dict.
fn base_font(doc: &Document, fonts: &Dictionary, key: &[u8]) -> String {
    let font_obj = fonts.get(key).expect("font key present");
    let font_dict = deref_dict(doc, font_obj);
    let base = font_dict
        .get(b"BaseFont")
        .expect("BaseFont")
        .as_name()
        .expect("BaseFont is a name");
    String::from_utf8_lossy(base).into_owned()
}

/// Every font key in a `/Font` sub-dict other than the given original(s).
fn renamed_keys(fonts: &Dictionary, originals: &[&[u8]]) -> Vec<Vec<u8>> {
    fonts
        .iter()
        .map(|(k, _)| k.clone())
        .filter(|k| !originals.contains(&k.as_slice()))
        .collect()
}

/// Every operation on a page (decompressing + decoding each fragment).
fn page_operations(doc: &Document, page_id: ObjectId) -> Vec<Operation> {
    let contents = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Contents")
        .unwrap();
    let ids: Vec<ObjectId> = match contents {
        Object::Array(a) => a.iter().map(|o| o.as_reference().unwrap()).collect(),
        Object::Reference(id) => vec![*id],
        other => panic!("unexpected /Contents object: {other:?}"),
    };
    let mut ops = Vec::new();
    for id in ids {
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let bytes = if stream.is_compressed() {
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone())
        } else {
            stream.content.clone()
        };
        if let Ok(c) = Content::decode(&bytes) {
            ops.extend(c.operations);
        }
    }
    ops
}

/// The `/Font` name operands of every `Tf` operation on a page.
fn tf_font_names(doc: &Document, page_id: ObjectId) -> Vec<Vec<u8>> {
    page_operations(doc, page_id)
        .iter()
        .filter(|op| op.operator == "Tf")
        .filter_map(|op| op.operands.first().and_then(|o| o.as_name().ok()))
        .map(|n| n.to_vec())
        .collect()
}

/// Facets 1 + 2: an indirect `/Font` sub-dict in the SOURCE must be renamed and
/// merged, and the source content re-pointed at the renamed key.
#[test]
fn overlay_source_indirect_font_subdict_is_merged_and_renamed() {
    let (overlay, _) = doc_with_resources(
        |d| {
            let courier = add_font(d, "Courier");
            let font_sub = d.add_object(dictionary! { "F1" => Object::Reference(courier) });
            Object::Dictionary(dictionary! { "Font" => Object::Reference(font_sub) })
        },
        b"q BT /F1 12 Tf ET Q",
    );
    let (mut dest, dest_page_id) = doc_with_resources(
        |d| {
            let times = add_font(d, "Times");
            Object::Dictionary(
                dictionary! { "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(times) }) },
            )
        },
        b"q BT /F1 12 Tf ET Q",
    );

    overlay_page(&mut dest, dest_page_id, &overlay, 1).expect("overlay must succeed");

    let fonts = page_font_dict(&dest, dest_page_id);
    assert_eq!(
        base_font(&dest, &fonts, b"F1"),
        "Times",
        "original dest F1 preserved"
    );

    let renamed = renamed_keys(&fonts, &[b"F1"]);
    assert_eq!(
        renamed.len(),
        1,
        "the overlay's Courier must arrive under exactly one renamed key, got {renamed:?} \
         (old code skipped the indirect sub-dict, dropping it entirely)"
    );
    assert_eq!(base_font(&dest, &fonts, &renamed[0]), "Courier");

    assert!(
        tf_font_names(&dest, dest_page_id).contains(&renamed[0]),
        "overlay content must reference the renamed key {:?}, not a raw /F1",
        String::from_utf8_lossy(&renamed[0])
    );
}

/// Facet 3: an indirect `/Font` sub-dict in the DESTINATION must merge into its
/// target, not error.
#[test]
fn overlay_dest_indirect_font_subdict_does_not_error() {
    let (mut dest, dest_page_id) = doc_with_resources(
        |d| {
            let times = add_font(d, "Times");
            let font_sub = d.add_object(dictionary! { "F1" => Object::Reference(times) });
            Object::Dictionary(dictionary! { "Font" => Object::Reference(font_sub) })
        },
        b"q BT /F1 12 Tf ET Q",
    );
    let (overlay, _) = doc_with_resources(
        |d| {
            let courier = add_font(d, "Courier");
            Object::Dictionary(
                dictionary! { "Font" => Object::Dictionary(dictionary! { "F2" => Object::Reference(courier) }) },
            )
        },
        b"q BT /F2 12 Tf ET Q",
    );

    overlay_page(&mut dest, dest_page_id, &overlay, 1)
        .expect("overlay onto a referenced dest /Font sub-dict must succeed (bug-0030 facet 3)");

    let fonts = page_font_dict(&dest, dest_page_id);
    assert_eq!(
        base_font(&dest, &fonts, b"F1"),
        "Times",
        "original dest F1 preserved"
    );
    let renamed = renamed_keys(&fonts, &[b"F1"]);
    assert_eq!(
        renamed.len(),
        1,
        "the source font must merge into the referenced dest sub-dict, got {renamed:?}"
    );
    assert_eq!(base_font(&dest, &fonts, &renamed[0]), "Courier");
}

/// Facet 4: names inside a referenced DEST sub-dict must enter the collision scan,
/// so a renamed source key does not overwrite an existing destination resource.
#[test]
fn overlay_collision_scan_sees_keys_inside_referenced_dest_subdict() {
    // Dest /Font <ref> already holds F1 AND F1_o — the name the source's F1 would
    // naively rename to.
    let (mut dest, dest_page_id) = doc_with_resources(
        |d| {
            let times = add_font(d, "Times");
            let helvetica = add_font(d, "Helvetica");
            let font_sub = d.add_object(dictionary! {
                "F1" => Object::Reference(times),
                "F1_o" => Object::Reference(helvetica),
            });
            Object::Dictionary(dictionary! { "Font" => Object::Reference(font_sub) })
        },
        b"q BT /F1 12 Tf ET Q",
    );
    let (overlay, _) = doc_with_resources(
        |d| {
            let courier = add_font(d, "Courier");
            Object::Dictionary(
                dictionary! { "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(courier) }) },
            )
        },
        b"q BT /F1 12 Tf ET Q",
    );

    overlay_page(&mut dest, dest_page_id, &overlay, 1).expect("overlay must succeed");

    let fonts = page_font_dict(&dest, dest_page_id);
    assert_eq!(base_font(&dest, &fonts, b"F1"), "Times");
    assert_eq!(
        base_font(&dest, &fonts, b"F1_o"),
        "Helvetica",
        "the existing dest F1_o must be untouched — the collision scan saw it inside the \
         referenced sub-dict and the source F1 was renamed past it (bug-0030 facet 4)"
    );
    assert!(
        fonts.has(b"F1_o1"),
        "the source F1 should have been renamed past the collision to F1_o1"
    );
    assert_eq!(base_font(&dest, &fonts, b"F1_o1"), "Courier");
}

/// place_page shares the same helpers: an indirect source sub-dict must merge.
#[test]
fn place_page_source_indirect_font_subdict_is_merged_and_renamed() {
    let (source, _) = doc_with_resources(
        |d| {
            let courier = add_font(d, "Courier");
            let font_sub = d.add_object(dictionary! { "F1" => Object::Reference(courier) });
            Object::Dictionary(dictionary! { "Font" => Object::Reference(font_sub) })
        },
        b"q BT /F1 12 Tf ET Q",
    );
    let (mut dest, dest_page_id) = doc_with_resources(
        |d| {
            let times = add_font(d, "Times");
            Object::Dictionary(
                dictionary! { "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(times) }) },
            )
        },
        b"q BT /F1 12 Tf ET Q",
    );

    place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    )
    .expect("place must succeed");

    let fonts = page_font_dict(&dest, dest_page_id);
    assert_eq!(base_font(&dest, &fonts, b"F1"), "Times");
    let renamed = renamed_keys(&fonts, &[b"F1"]);
    assert_eq!(
        renamed.len(),
        1,
        "the placed source font must merge under a renamed key, got {renamed:?}"
    );
    assert_eq!(base_font(&dest, &fonts, &renamed[0]), "Courier");
}
