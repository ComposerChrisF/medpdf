// tests/overlay_length_regression_tests.rs
//
// Regression tests for the overlay `/Length` synchronization bug.
//
// `modify_content_stream` re-encodes each page content stream (wrapping the
// operators in a `q` … `Q` pair and renaming source resource keys). The new
// body differs in length from the original. Before the fix it assigned the
// bytes via the raw public field `Stream::content`, which does NOT update the
// dictionary's `/Length`. lopdf's reader trusts `/Length`: with a stale (too
// short) value it reads fewer bytes than the body, fails to find `endstream`,
// and falls back to a bare dictionary — silently dropping the overlaid content.
//
// The fix routes the assignment through `Stream::set_content`, which re-syncs
// `/Length`. These tests fail before that fix and pass after.

mod fixtures;

use lopdf::{Document, Object, ObjectId};
use medpdf::pdf_copy_page::copy_page;
use medpdf::pdf_overlay::overlay_page;

/// Collects the ObjectIds of every fragment in a page's `/Contents` array.
/// (After an overlay, `/Contents` is always normalized to an array of refs.)
fn contents_fragment_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let contents = doc
        .get_dictionary(page_id)
        .expect("page dict")
        .get(b"Contents")
        .expect("page has /Contents");
    match contents {
        Object::Array(arr) => arr
            .iter()
            .map(|o| o.as_reference().expect("contents fragment is a reference"))
            .collect(),
        Object::Reference(id) => vec![*id],
        other => panic!("unexpected /Contents object: {other:?}"),
    }
}

/// Builds a destination page (empty content) with one text-bearing overlay
/// applied, returning the document and the destination page id.
fn overlaid_doc() -> (Document, ObjectId) {
    let source_doc = fixtures::create_pdf_with_pages(1);
    let mut dest_doc = fixtures::create_empty_pdf();
    let dest_page_id = copy_page(&mut dest_doc, &source_doc, 1).expect("copy dest page");

    // An overlay page carrying recognizable text. The operators get wrapped in
    // q/Q and re-encoded, so the body length necessarily changes.
    let overlay_doc =
        fixtures::create_pdf_with_content(b"BT\n/F7 36 Tf\n144 360 Td\n(OVERLAYTEXT) Tj\nET\n");

    overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, 1).expect("overlay");
    (dest_doc, dest_page_id)
}

/// Layer 1 — pinpoints the defect at its source.
///
/// Immediately after the overlay (before any save), every content stream's
/// declared `/Length` must equal its actual body length. The raw-field
/// assignment left `/Length` stale; `set_content` keeps it in sync.
#[test]
fn overlay_content_streams_have_synced_length_in_memory() {
    let (doc, page_id) = overlaid_doc();

    let fragment_ids = contents_fragment_ids(&doc, page_id);
    assert!(
        !fragment_ids.is_empty(),
        "overlaid page should have content fragments"
    );

    for id in fragment_ids {
        let stream = doc
            .get_object(id)
            .expect("fragment object")
            .as_stream()
            .expect("fragment is a stream in memory");
        let declared = stream
            .dict
            .get(b"Length")
            .expect("stream has /Length")
            .as_i64()
            .expect("/Length is an integer");
        assert_eq!(
            declared as usize,
            stream.content.len(),
            "content stream {id:?} declares /Length {declared} but body is {} bytes — \
             a stale /Length makes lopdf drop the body on reload",
            stream.content.len()
        );
    }
}

/// Layer 2 — proves the user-visible consequence is gone.
///
/// Save to a buffer and reload (as any spec-compliant reader would). With a
/// stale `/Length` the overlay fragments degrade to bare dictionaries and the
/// text vanishes. After the fix every fragment survives as a stream and the
/// overlaid text is recoverable.
#[test]
fn overlay_content_survives_save_and_reload() {
    let (mut doc, page_id) = overlaid_doc();

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save PDF to buffer");
    let reloaded = Document::load_mem(&buf).expect("reload PDF from buffer");

    // The reloaded page id matches (same object graph) — locate page 1 fresh.
    let reloaded_page_id = *reloaded.get_pages().get(&1).expect("reloaded page 1");
    assert_eq!(
        reloaded_page_id, page_id,
        "page object id should be stable across save/reload"
    );

    let fragment_ids = contents_fragment_ids(&reloaded, reloaded_page_id);
    let mut all_text = Vec::new();
    for id in fragment_ids {
        let obj = reloaded.get_object(id).expect("fragment object");
        let stream = obj.as_stream().unwrap_or_else(|_| {
            panic!(
                "content fragment {id:?} reloaded as {obj:?}, not a stream — \
                 overlay body was dropped (stale /Length regression)"
            )
        });
        let body = if stream.is_compressed() {
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone())
        } else {
            stream.content.clone()
        };
        all_text.extend_from_slice(&body);
    }

    assert!(
        all_text
            .windows(b"OVERLAYTEXT".len())
            .any(|w| w == b"OVERLAYTEXT"),
        "overlaid text 'OVERLAYTEXT' should survive save/reload but was not found"
    );
}
