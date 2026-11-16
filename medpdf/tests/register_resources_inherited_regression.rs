#![expect(
    clippy::unwrap_used,
    reason = "test helpers outside a #[test] fn; a failure there is meant to fail the test"
)]
#![expect(
    clippy::panic,
    reason = "test helpers outside a #[test] fn; a panic there is meant to fail the test"
)]
// tests/register_resources_inherited_regression.rs
//
// Regression tests for bugs/bug-0042: `register_in_page_resources` gave a page
// that inherits /Resources from its /Pages parent a new dictionary holding only
// the new entry.  A page-level /Resources replaces the inherited one, so the
// page's own content lost every resource it named.  The fix starts the page's
// dictionary from a private copy of the inherited one, leaving the ancestor's
// dictionaries, which sibling pages share, unchanged.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use medpdf::register_in_page_resources;

fn add_font(doc: &mut Document, base: &str) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(base.as_bytes().to_vec()),
    })
}

/// The shape of the inherited resources: each indirect or inline.
#[derive(Clone, Copy)]
struct Shape {
    resources_by_ref: bool,
    subdicts_by_ref: bool,
}

/// What the test needs to find again after registering.
struct Built {
    doc: Document,
    page_id: ObjectId,
    pages_id: ObjectId,
    /// The `/Resources` object, when it is indirect.
    resources_id: Option<ObjectId>,
    /// The `/Font` and `/ExtGState` objects, when they are indirect.
    font_sub_id: Option<ObjectId>,
    gs_sub_id: Option<ObjectId>,
}

/// A one-page document whose `/Resources << /Font << /F1 … >> /ExtGState << /GS1 … >> >>`
/// sits on the `/Pages` node, and whose page has no `/Resources` of its own.
fn build_inherited_doc(shape: Shape) -> Built {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let f1 = add_font(&mut doc, "Helvetica");
    let gs1 = doc.add_object(dictionary! { "Type" => "ExtGState", "CA" => 0.5 });
    let sub = |doc: &mut Document, d: Dictionary| -> (Object, Option<ObjectId>) {
        if shape.subdicts_by_ref {
            let id = doc.add_object(Object::Dictionary(d));
            (Object::Reference(id), Some(id))
        } else {
            (Object::Dictionary(d), None)
        }
    };
    let (font_value, font_sub_id) = sub(&mut doc, dictionary! { "F1" => Object::Reference(f1) });
    let (gs_value, gs_sub_id) = sub(&mut doc, dictionary! { "GS1" => Object::Reference(gs1) });
    let resources = dictionary! { "Font" => font_value, "ExtGState" => gs_value };
    let (resources_value, resources_id) = if shape.resources_by_ref {
        let id = doc.add_object(Object::Dictionary(resources));
        (Object::Reference(id), Some(id))
    } else {
        (Object::Dictionary(resources), None)
    };

    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"/GS1 gs BT /F1 12 Tf 72 720 Td (Hello) Tj ET".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
        "Contents" => Object::Reference(content_id),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
            "Resources" => resources_value,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    Built {
        doc,
        page_id,
        pages_id,
        resources_id,
        font_sub_id,
        gs_sub_id,
    }
}

fn deref_dict(doc: &Document, obj: &Object) -> Dictionary {
    match obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
        other => panic!("expected dictionary or reference, got {other:?}"),
    }
}

/// The `category` sub-dictionary of the `/Resources` held directly by `holder_id`.
fn subdict_of(doc: &Document, holder_id: ObjectId, category: &[u8]) -> Dictionary {
    let holder = doc.get_dictionary(holder_id).unwrap();
    let resources = deref_dict(doc, holder.get(b"Resources").unwrap());
    deref_dict(doc, resources.get(category).unwrap())
}

fn keys(dict: &Dictionary) -> Vec<Vec<u8>> {
    dict.iter().map(|(k, _)| k.clone()).collect()
}

/// Registers `/F9` and then `/GS9` on the page, and checks that the page keeps
/// what it inherited and that nothing the ancestor holds has changed.
fn register_and_check(shape: Shape) {
    let mut b = build_inherited_doc(shape);
    let f9 = add_font(&mut b.doc, "Courier");
    let gs9 = b
        .doc
        .add_object(dictionary! { "Type" => "ExtGState", "CA" => 0.25 });

    register_in_page_resources(&mut b.doc, b.page_id, b"Font", b"F9", f9).unwrap();
    register_in_page_resources(&mut b.doc, b.page_id, b"ExtGState", b"GS9", gs9).unwrap();

    let doc = &b.doc;
    let page_fonts = subdict_of(doc, b.page_id, b"Font");
    let page_gs = subdict_of(doc, b.page_id, b"ExtGState");
    assert!(
        page_fonts.has(b"F1") && page_gs.has(b"GS1"),
        "the page's content uses the inherited /F1 and /GS1, which must still resolve \
         after registering (bug-0042); got fonts {:?}, ExtGState {:?}",
        keys(&page_fonts),
        keys(&page_gs)
    );
    assert!(
        page_fonts.has(b"F9") && page_gs.has(b"GS9"),
        "both new entries must be registered"
    );

    assert_eq!(
        keys(&subdict_of(doc, b.pages_id, b"Font")),
        vec![b"F1".to_vec()]
    );
    assert_eq!(
        keys(&subdict_of(doc, b.pages_id, b"ExtGState")),
        vec![b"GS1".to_vec()]
    );
    for (what, id, key) in [
        ("/Font", b.font_sub_id, &b"F1"[..]),
        ("/ExtGState", b.gs_sub_id, b"GS1"),
    ] {
        if let Some(id) = id {
            assert_eq!(
                keys(doc.get_dictionary(id).unwrap()),
                vec![key.to_vec()],
                "the inherited {what} object is shared with sibling pages and must not change"
            );
        }
    }
    if let Some(id) = b.resources_id {
        assert_eq!(
            keys(doc.get_dictionary(id).unwrap()),
            vec![b"Font".to_vec(), b"ExtGState".to_vec()],
            "the inherited /Resources object must not change"
        );
    }
}

#[test]
fn register_keeps_inline_inherited_resources() {
    register_and_check(Shape {
        resources_by_ref: false,
        subdicts_by_ref: false,
    });
}

#[test]
fn register_does_not_change_shared_inherited_subdicts() {
    register_and_check(Shape {
        resources_by_ref: false,
        subdicts_by_ref: true,
    });
}

#[test]
fn register_keeps_indirect_inherited_resources() {
    register_and_check(Shape {
        resources_by_ref: true,
        subdicts_by_ref: true,
    });
}
