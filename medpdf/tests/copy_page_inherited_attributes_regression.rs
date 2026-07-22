// tests/copy_page_inherited_attributes_regression.rs
//
// Regression test for bugs/bug-0008: copy_page silently lost inherited page
// attributes. /Resources, /MediaBox, /CropBox, and /Rotate are inheritable (PDF
// 32000-1 §7.7.3.4) and real producers place them on a /Pages ancestor. Because
// deep_copy skips /Parent (correctly — else it would copy the whole tree), nothing
// materialized those values onto the copied page: it arrived with only its own
// keys, so it lost its size (invalid page), its fonts (text disappears), and its
// rotation, all silently.
//
// The fix walks the source page's /Parent chain and flattens each inherited
// attribute the copied page lacks onto the leaf page.

use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_helpers::{get_page_media_box, get_page_rotation};

fn add_font(doc: &mut Document, base: &str) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(base.as_bytes().to_vec()),
    })
}

fn deref_dict(doc: &Document, obj: &Object) -> lopdf::Dictionary {
    match obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
        other => panic!("expected dictionary or reference, got {other:?}"),
    }
}

/// A source doc whose only page inherits MediaBox, CropBox, Rotate, and Resources
/// from its /Pages node — the page dict itself carries none of them.
fn source_with_inherited_attrs() -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font = add_font(&mut doc, "Helvetica");
    let resources = dictionary! {
        "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf (hello) Tj ET".to_vec(),
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => Object::Reference(content_id),
        // No MediaBox / CropBox / Rotate / Resources — all inherited below.
    });

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 300.0.into(), 400.0.into()],
        "CropBox" => vec![10.0.into(), 10.0.into(), 290.0.into(), 390.0.into()],
        "Rotate" => 90,
        "Resources" => Object::Dictionary(resources),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    (doc, page_id)
}

/// A minimal destination doc with an empty page tree.
fn empty_dest() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

#[test]
fn copy_page_materializes_inherited_attributes() {
    let (src, src_page) = source_with_inherited_attrs();
    // Sanity: the source resolves the inherited attributes via the /Parent walk.
    assert_eq!(
        get_page_media_box(&src, src_page),
        Some([0.0, 0.0, 300.0, 400.0])
    );
    assert_eq!(get_page_rotation(&src, src_page), 90);

    let mut dest = empty_dest();
    let new_page = copy_page(&mut dest, &src, 1).expect("copy_page must succeed");

    // The copied page must now render identically under its new parent, even
    // though the destination's /Pages node inherits none of these.
    assert_eq!(
        get_page_media_box(&dest, new_page),
        Some([0.0, 0.0, 300.0, 400.0]),
        "inherited MediaBox must be materialized onto the copied page (bug-0008)"
    );
    assert_eq!(
        get_page_rotation(&dest, new_page),
        90,
        "inherited Rotate must be materialized"
    );

    let page = dest.get_dictionary(new_page).unwrap();
    for key in [
        &b"MediaBox"[..],
        &b"CropBox"[..],
        &b"Rotate"[..],
        &b"Resources"[..],
    ] {
        assert!(
            page.get(key).is_ok(),
            "the copied page must carry its own {} (materialized from the inherited value)",
            String::from_utf8_lossy(key)
        );
    }

    // The content's /F1 now resolves: the inherited Resources (and its font) came
    // along, so the text would still render.
    let resources = deref_dict(&dest, page.get(b"Resources").unwrap());
    let fonts = deref_dict(&dest, resources.get(b"Font").unwrap());
    let f1 = deref_dict(
        &dest,
        fonts
            .get(b"F1")
            .expect("F1 present in materialized resources"),
    );
    assert_eq!(
        f1.get(b"BaseFont").unwrap().as_name().unwrap(),
        b"Helvetica",
        "the inherited font must be copied down, not dropped"
    );
}
