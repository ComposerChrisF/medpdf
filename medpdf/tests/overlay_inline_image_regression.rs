// tests/overlay_inline_image_regression.rs
//
// Regression tests for bugs/bug-0018: the overlay/place decode->re-encode round
// trip corrupted or deleted inline images (BI ... ID ... EI) under lopdf 0.42 —
// including inline images in the *destination* page's own untouched content, and
// in any other page sharing those content streams by reference.
//
// The fix has two halves:
//   1. Destination content is isolated with standalone q/Q wrapper streams and is
//      never re-encoded, so its bytes (inline images included) survive verbatim.
//   2. Source content, which must be re-encoded to rename its resources, is
//      screened for inline images and fails loudly instead of silently mangling
//      them (see LOPDF_INLINE_IMAGE_BUG.md).

use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf::pdf_overlay::overlay_page;
use medpdf::{PlacePageParams, place_page};

/// A 1x1 grayscale (`/CS /G`) inline image — a legal colorspace abbreviation
/// lopdf's inline-image decoder does not resolve, so the old round trip dropped
/// the image data entirely, leaving a bare orphan `BI`.
const INLINE_IMAGE_CONTENT: &[u8] = b"q\nBI\n/W 1 /H 1 /CS /G /BPC 8\nID \x80\nEI\nQ\n";

/// Ordinary content with no inline image (safe to re-encode).
const PLAIN_CONTENT: &[u8] = b"q\n0 0 1 rg\nQ\n";

/// Builds a minimal one-page document with `content` as the page's single,
/// UNCOMPRESSED content stream. Returns (doc, page_id, content_stream_id).
fn single_page_doc(content: &[u8]) -> (Document, ObjectId, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
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
    (doc, page_id, content_id)
}

fn stream_bytes(doc: &Document, id: ObjectId) -> Vec<u8> {
    doc.get_object(id)
        .expect("content object")
        .as_stream()
        .expect("content is a stream")
        .content
        .clone()
}

/// Concatenates every content fragment currently on `page_id`, decompressing as
/// needed — i.e. the actual byte stream a renderer would execute for the page.
fn concat_page_contents(doc: &Document, page_id: ObjectId) -> Vec<u8> {
    let contents = doc
        .get_dictionary(page_id)
        .expect("page dict")
        .get(b"Contents")
        .expect("page has /Contents");
    let ids: Vec<ObjectId> = match contents {
        Object::Array(a) => a.iter().map(|o| o.as_reference().unwrap()).collect(),
        Object::Reference(id) => vec![*id],
        other => panic!("unexpected /Contents object: {other:?}"),
    };
    let mut out = Vec::new();
    for id in ids {
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let bytes = if stream.is_compressed() {
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone())
        } else {
            stream.content.clone()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Fix half 1: overlaying a page must not corrupt that page's own inline image.
///
/// A single-reference `/Contents` is cloned by `resolve_contents_to_ref_array`,
/// so the corruption lands on the clone that *becomes* the page content — this
/// asserts on the resulting page content, not the now-orphaned original.
#[test]
fn overlay_preserves_destination_inline_image() {
    let (mut dest, dest_page_id, _) = single_page_doc(INLINE_IMAGE_CONTENT);

    // Stamp ordinary content on top of the inline-image page.
    let (overlay, _, _) = single_page_doc(PLAIN_CONTENT);
    overlay_page(&mut dest, dest_page_id, &overlay, 1).expect("overlay must succeed");

    let page_content = concat_page_contents(&dest, dest_page_id);
    assert!(
        contains_subslice(&page_content, INLINE_IMAGE_CONTENT),
        "the destination's inline image must survive overlay intact — its data byte and EI were \
         dropped by the old decode->re-encode round trip (bug-0018).\nPage content: {page_content:?}"
    );
}

/// Fix half 2 (overlay entry): an inline image in the *source* must fail loudly,
/// not silently corrupt — and must not damage the destination on the way out.
#[test]
fn overlay_source_with_inline_image_errors_loudly() {
    let (mut dest, dest_page_id, dest_content_id) = single_page_doc(PLAIN_CONTENT);
    let dest_before = stream_bytes(&dest, dest_content_id);

    let (overlay, _, _) = single_page_doc(INLINE_IMAGE_CONTENT);
    let err = overlay_page(&mut dest, dest_page_id, &overlay, 1)
        .expect_err("an overlay source inline image must error, not silently corrupt");
    let msg = err.to_string();
    assert!(
        msg.contains("inline image"),
        "error should name the cause: {msg}"
    );
    assert!(
        msg.contains("LOPDF_INLINE_IMAGE_BUG"),
        "error should cite the upstream-defect record: {msg}"
    );

    assert_eq!(
        stream_bytes(&dest, dest_content_id),
        dest_before,
        "destination content must be unchanged after a rejected overlay"
    );
}

/// Fix half 2 (place_page entry): same rejection through the other call site.
#[test]
fn place_page_source_with_inline_image_errors_loudly() {
    let (mut dest, dest_page_id, _) = single_page_doc(PLAIN_CONTENT);
    let (source, _, _) = single_page_doc(INLINE_IMAGE_CONTENT);

    let err = place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    )
    .expect_err("a place_page source inline image must error, not silently corrupt");
    assert!(
        err.to_string().contains("inline image"),
        "error should name the cause: {err}"
    );
}

/// The shared-stream facet: two pages whose `/Contents` ARRAYS both reference one
/// inline-image stream. The array form is the dangerous one — `resolve` returns
/// the original references, so the old in-place mutation corrupted the stream
/// page 2 also depends on. Overlaying page 1 must leave that shared stream intact.
#[test]
fn overlay_does_not_corrupt_a_shared_destination_content_stream() {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, INLINE_IMAGE_CONTENT.to_vec()));

    let make_page = |doc: &mut Document| {
        let resources_id = doc.add_object(dictionary! {});
        doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
            // Array form (single shared fragment), not a bare reference.
            "Contents" => Object::Array(vec![Object::Reference(content_id)]),
            "Resources" => Object::Reference(resources_id),
        })
    };
    let page1 = make_page(&mut doc);
    let page2 = make_page(&mut doc);

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page1), Object::Reference(page2)],
        "Count" => 2,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let (overlay, _, _) = single_page_doc(PLAIN_CONTENT);
    overlay_page(&mut doc, page1, &overlay, 1).expect("overlay page 1");

    // Page 2 still points at the shared stream, and that stream is intact.
    let page2_contents = doc.get_dictionary(page2).unwrap().get(b"Contents").unwrap();
    let page2_ref = page2_contents.as_array().unwrap()[0]
        .as_reference()
        .unwrap();
    assert_eq!(
        page2_ref, content_id,
        "page 2 should still reference the shared content stream"
    );
    assert_eq!(
        stream_bytes(&doc, content_id),
        INLINE_IMAGE_CONTENT,
        "the shared content stream must be untouched (bug-0018 shared-stream mutation)"
    );
}
