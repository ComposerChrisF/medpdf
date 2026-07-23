// tests/overlay_contentless_source_regression.rs
//
// Regression test for bugs/bug-0016: overlay_page errored on a source page with no
// /Contents. /Contents is optional on a page (PDF 32000-1 §7.7.3.3) — a blank page
// legally omits it. The pre-fix code did `overlay_page.get(KEY_CONTENTS)?`, so
// overlaying *from* a contentless page returned Err(DictKey("Contents")) instead of
// succeeding as a no-op. The sibling place_page already treats this as a no-op.
//
// The fix mirrors place_page: match on the /Contents lookup and, on Err, log and
// return Ok(()). To confirm this test pins the fix, revert the match in
// pdf_overlay.rs back to `overlay_page.get(KEY_CONTENTS)?`; overlay_ok_on_contentless_source
// then fails with Err(DictKey).

use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf::{PlacePageParams, overlay_page, place_page};

/// A destination doc with one normal page that carries a single /Contents stream.
fn dest_with_content() -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    (doc, page_id)
}

/// An overlay/source doc whose only page has NO /Contents key — a valid blank page.
fn contentless_source() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        // No /Contents — a legal blank page.
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
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
fn overlay_ok_on_contentless_source() {
    let (mut dest, dest_page) = dest_with_content();
    let ovl = contentless_source();

    // Capture the destination's /Contents before the overlay so we can prove the
    // no-op left the page untouched (Ok must mean "nothing happened", not a partial
    // mutation).
    let before = dest
        .get_dictionary(dest_page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();

    let r = overlay_page(&mut dest, dest_page, &ovl, 1);
    assert!(
        r.is_ok(),
        "overlaying from a contentless source must be a no-op, got {r:?} (bug-0016)"
    );

    let after = dest
        .get_dictionary(dest_page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    assert_eq!(
        before, after,
        "a no-op overlay must leave the destination /Contents untouched"
    );
}

#[test]
fn place_ok_on_contentless_source_unchanged() {
    // Guards the sibling behavior overlay is being aligned to: place_page already
    // no-ops on a contentless source, and must continue to.
    let (mut dest, dest_page) = dest_with_content();
    let src = contentless_source();

    let before = dest
        .get_dictionary(dest_page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();

    let r = place_page(
        &mut dest,
        dest_page,
        &src,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    );
    assert!(
        r.is_ok(),
        "place_page must no-op on a contentless source, got {r:?}"
    );

    let after = dest
        .get_dictionary(dest_page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    assert_eq!(
        before, after,
        "place_page no-op must leave /Contents untouched"
    );
}
