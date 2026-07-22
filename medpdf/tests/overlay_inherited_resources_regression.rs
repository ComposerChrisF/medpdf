// tests/overlay_inherited_resources_regression.rs
//
// Regression tests for bugs/bug-0017: inherited /Resources (held on a /Pages
// ancestor, as Acrobat routinely emits) was mishandled by overlay and place.
//
//   Facet 1 — a destination page with inherited resources got a page-level dict
//     holding only the overlay's keys, which REPLACES the inherited one (PDF
//     inheritance is replace-not-merge), stranding the page's own content.
//   Facet 2 — place_page substituted an empty dict for inherited SOURCE resources,
//     so the placed content bound to the wrong font.
//   Facet 3 — overlay_page erred outright on a source page with inherited resources.
//
// The fix resolves a page's effective /Resources by walking /Parent, materializes
// the inherited dict onto a destination page (privately, so a shared ancestor
// sub-dict is not polluted), and resolves the source page's resources the same way.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use medpdf::pdf_overlay::overlay_page;
use medpdf::{PlacePageParams, place_page};

fn add_font(doc: &mut Document, base: &str) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(base.as_bytes().to_vec()),
    })
}

/// Builds a one-page doc with a `/Font` resource `{ key: base }`. When
/// `resources_on_page` is false, the `/Resources` dict lives on the `/Pages` node
/// and the page dict has none (so the page *inherits* it).
fn build_font_doc(
    key: &[u8],
    base: &str,
    content: &[u8],
    resources_on_page: bool,
) -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font = add_font(&mut doc, base);
    let mut font_sub = Dictionary::new();
    font_sub.set(key.to_vec(), Object::Reference(font));
    let resources = Object::Dictionary(dictionary! { "Font" => Object::Dictionary(font_sub) });

    let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
        "Contents" => Object::Reference(content_id),
    };
    if resources_on_page {
        page.set(b"Resources".to_vec(), resources.clone());
    }
    let page_id = doc.add_object(page);

    let mut pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    };
    if !resources_on_page {
        pages.set(b"Resources".to_vec(), resources);
    }
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

fn page_font_dict(doc: &Document, page_id: ObjectId) -> Dictionary {
    let page = doc.get_dictionary(page_id).unwrap();
    let resources = deref_dict(doc, page.get(b"Resources").unwrap());
    deref_dict(doc, resources.get(b"Font").unwrap())
}

fn base_font(doc: &Document, fonts: &Dictionary, key: &[u8]) -> String {
    let font_dict = deref_dict(doc, fonts.get(key).expect("font key present"));
    let base = font_dict
        .get(b"BaseFont")
        .expect("BaseFont")
        .as_name()
        .expect("BaseFont is a name");
    String::from_utf8_lossy(base).into_owned()
}

fn renamed_keys(fonts: &Dictionary, originals: &[&[u8]]) -> Vec<Vec<u8>> {
    fonts
        .iter()
        .map(|(k, _)| k.clone())
        .filter(|k| !originals.contains(&k.as_slice()))
        .collect()
}

fn tf_font_names(doc: &Document, page_id: ObjectId) -> Vec<Vec<u8>> {
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
    let mut ops: Vec<Operation> = Vec::new();
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
    ops.iter()
        .filter(|op| op.operator == "Tf")
        .filter_map(|op| op.operands.first().and_then(|o| o.as_name().ok()))
        .map(|n| n.to_vec())
        .collect()
}

/// Facet 1: a destination page's INHERITED resources must be materialized onto the
/// page, not shadowed by a page-level dict holding only the overlay's keys.
#[test]
fn overlay_materializes_inherited_dest_resources() {
    let (mut dest, dest_page) = build_font_doc(b"F1", "Times", b"q BT /F1 12 Tf ET Q", false);
    let (overlay, _) = build_font_doc(b"F1", "Courier", b"q BT /F1 12 Tf ET Q", true);

    overlay_page(&mut dest, dest_page, &overlay, 1).expect("overlay must succeed");

    let fonts = page_font_dict(&dest, dest_page);
    assert_eq!(
        base_font(&dest, &fonts, b"F1"),
        "Times",
        "the inherited F1 must be materialized onto the page — otherwise the page-level dict \
         replaces the inherited one and the page's own text loses its font (bug-0017 facet 1)"
    );
    let renamed = renamed_keys(&fonts, &[b"F1"]);
    assert_eq!(
        renamed.len(),
        1,
        "the overlay's font must merge alongside under a renamed key, got {renamed:?}"
    );
    assert_eq!(base_font(&dest, &fonts, &renamed[0]), "Courier");
}

/// Facet 2: place_page must resolve INHERITED source resources so the placed
/// content binds to the source's own font, not the destination's.
#[test]
fn place_page_resolves_inherited_source_resources() {
    let (source, _) = build_font_doc(b"F1", "Times", b"q BT /F1 12 Tf ET Q", false);
    let (mut dest, dest_page) = build_font_doc(b"F1", "Helvetica", b"q BT /F1 12 Tf ET Q", true);

    place_page(
        &mut dest,
        dest_page,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    )
    .expect("place must succeed");

    let fonts = page_font_dict(&dest, dest_page);
    assert_eq!(
        base_font(&dest, &fonts, b"F1"),
        "Helvetica",
        "dest own F1 preserved"
    );
    let renamed = renamed_keys(&fonts, &[b"F1"]);
    assert_eq!(
        renamed.len(),
        1,
        "the source's inherited Times must arrive under a renamed key, got {renamed:?} \
         (old code substituted an empty dict and dropped it)"
    );
    assert_eq!(base_font(&dest, &fonts, &renamed[0]), "Times");
    assert!(
        tf_font_names(&dest, dest_page).contains(&renamed[0]),
        "placed content must reference the renamed source font, not bind to the dest's F1"
    );
}

/// Facet 3: overlay_page must not error on a source page with inherited resources.
#[test]
fn overlay_does_not_error_on_inherited_source_resources() {
    let (overlay, _) = build_font_doc(b"F1", "Times", b"q BT /F1 12 Tf ET Q", false);
    let (mut dest, dest_page) = build_font_doc(b"F2", "Helvetica", b"q BT /F2 12 Tf ET Q", true);

    overlay_page(&mut dest, dest_page, &overlay, 1).expect(
        "overlay of a source page with inherited resources must succeed (bug-0017 facet 3)",
    );

    let fonts = page_font_dict(&dest, dest_page);
    assert_eq!(
        base_font(&dest, &fonts, b"F2"),
        "Helvetica",
        "dest own F2 preserved"
    );
    let renamed = renamed_keys(&fonts, &[b"F2"]);
    assert_eq!(
        renamed.len(),
        1,
        "the source's inherited Times must merge, got {renamed:?}"
    );
    assert_eq!(base_font(&dest, &fonts, &renamed[0]), "Times");
}

/// Builds a doc with TWO pages that both inherit a single, SHARED (referenced)
/// `/Font` sub-dict from the `/Pages` node. Returns (doc, page1, page2, shared
/// font sub-dict id).
fn two_pages_sharing_inherited_font() -> (Document, ObjectId, ObjectId, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let times = add_font(&mut doc, "Times");
    let shared_font_sub = doc.add_object(dictionary! { "F1" => Object::Reference(times) });
    // The /Pages /Resources holds /Font as a REFERENCE to the shared sub-dict.
    let pages_resources = Object::Dictionary(dictionary! {
        "Font" => Object::Reference(shared_font_sub),
    });

    let make_page = |doc: &mut Document| {
        let content_id =
            doc.add_object(Stream::new(dictionary! {}, b"q BT /F1 12 Tf ET Q".to_vec()));
        doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
            "Contents" => Object::Reference(content_id),
        })
    };
    let page1 = make_page(&mut doc);
    let page2 = make_page(&mut doc);

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page1), Object::Reference(page2)],
        "Count" => 2,
        "Resources" => pages_resources,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    (doc, page1, page2, shared_font_sub)
}

/// Materializing inherited resources must not mutate a sub-dict a SIBLING page
/// still shares: page 1 gets a private copy, page 2's inherited font is untouched.
#[test]
fn overlay_does_not_pollute_a_shared_inherited_font_subdict() {
    let (mut doc, page1, _page2, shared_font_sub) = two_pages_sharing_inherited_font();
    let (overlay, _) = build_font_doc(b"F1", "Courier", b"q BT /F1 12 Tf ET Q", true);

    overlay_page(&mut doc, page1, &overlay, 1).expect("overlay page 1");

    let shared = doc.get_dictionary(shared_font_sub).unwrap();
    assert_eq!(
        shared.len(),
        1,
        "the shared inherited /Font sub-dict (still inherited by page 2) must keep exactly its \
         one original key — page 1 must materialize a private copy (bug-0017), got {shared:?}"
    );
    assert_eq!(base_font(&doc, shared, b"F1"), "Times");

    // Sanity: page 1 did get the overlay font, in its own materialized resources.
    let p1_fonts = page_font_dict(&doc, page1);
    assert_eq!(
        base_font(&doc, &p1_fonts, b"F1"),
        "Times",
        "page 1 keeps its inherited F1"
    );
    assert_eq!(
        renamed_keys(&p1_fonts, &[b"F1"]).len(),
        1,
        "page 1 got the overlay font under a renamed key"
    );
}
