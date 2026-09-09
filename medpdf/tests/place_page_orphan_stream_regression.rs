//! Regression test for bug-0039: `create_blank_page` + `place_page` orphaned one
//! stream object per destination page.
//!
//! The invariant pinned here is deliberately the *outcome*, not the mechanism:
//! after a `create_blank_page` → `place_page` round trip (pdf-maker's imposition
//! sequence), every object in the document is still reachable from the trailer.
//!
//! The mechanism, for the record: `resolve_contents_to_ref_array` was given the
//! destination's `/Contents` with `source_doc: None`, hit its
//! `Reference → Stream` arm, and *cloned* the stream into a fresh object. The
//! page dict was then rewritten to point at the clone, so the original — the
//! blank page's empty stream, or, worse, a real page's entire content stream —
//! became unreachable. No cross-document copy is needed on the destination side;
//! the reference is now passed through untouched.

mod fixtures;

use fixtures::{
    create_empty_pdf, create_pdf_with_content, create_pdf_with_pages, get_first_page_id,
};
use lopdf::{Document, Object, ObjectId};
use medpdf::{PlacePageParams, create_blank_page, place_page};
use std::collections::HashSet;

/// Every object id reachable from the trailer by following references.
fn reachable_ids(doc: &Document) -> HashSet<ObjectId> {
    let mut seen = HashSet::new();
    let mut stack: Vec<Object> = doc.trailer.iter().map(|(_, v)| v.clone()).collect();

    while let Some(obj) = stack.pop() {
        match obj {
            Object::Reference(id) => {
                if seen.insert(id)
                    && let Ok(target) = doc.get_object(id)
                {
                    stack.push(target.clone());
                }
            }
            Object::Array(items) => stack.extend(items),
            Object::Dictionary(dict) => stack.extend(dict.iter().map(|(_, v)| v.clone())),
            Object::Stream(stream) => {
                stack.extend(stream.dict.iter().map(|(_, v)| v.clone()));
            }
            _ => {}
        }
    }
    seen
}

/// Panics naming every object the trailer can no longer reach.
fn assert_no_orphans(doc: &Document, context: &str) {
    let reachable = reachable_ids(doc);
    let orphans: Vec<ObjectId> = doc
        .objects
        .keys()
        .copied()
        .filter(|id| !reachable.contains(id))
        .collect();
    assert!(
        orphans.is_empty(),
        "{context}: {} object(s) unreachable from the trailer: {orphans:?}",
        orphans.len()
    );
}

#[test]
fn blank_page_plus_place_page_leaves_no_orphans() {
    let mut dest = create_empty_pdf();
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let sheet_id = create_blank_page(&mut dest, 612.0, 792.0).unwrap();
    assert_no_orphans(&dest, "after create_blank_page");

    place_page(
        &mut dest,
        sheet_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 0.5),
    )
    .unwrap();

    assert_no_orphans(&dest, "after create_blank_page + place_page");
}

#[test]
fn imposition_sheet_with_four_placements_leaves_no_orphans() {
    // The `--nup n=4` shape: one sheet, four placements. Only the first call sees
    // a single-Reference /Contents, so a per-call leak and a per-sheet leak look
    // different here.
    let mut dest = create_empty_pdf();
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");

    let sheet_id = create_blank_page(&mut dest, 612.0, 792.0).unwrap();
    for (x, y) in [(0.0, 396.0), (306.0, 396.0), (0.0, 0.0), (306.0, 0.0)] {
        place_page(
            &mut dest,
            sheet_id,
            &source,
            1,
            &PlacePageParams::new(x, y, 0.5),
        )
        .unwrap();
    }

    assert_no_orphans(&dest, "after 4 placements on one sheet");
}

#[test]
fn place_page_does_not_duplicate_the_destinations_own_content_stream() {
    // The same dropped reference, on a destination page that *has* content: the
    // stream was cloned into a new object and the original orphaned, so the
    // duplicated bytes were arbitrarily large — not merely the zero-byte stream
    // bug-0039 was reported from.
    let mut dest = create_pdf_with_pages(1);
    let source = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let dest_page_id = get_first_page_id(&dest);

    let original_content_id = dest
        .get_dictionary(dest_page_id)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();

    place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    )
    .unwrap();

    let contents = dest
        .get_dictionary(dest_page_id)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap();
    let referenced: Vec<ObjectId> = contents.iter().map(|o| o.as_reference().unwrap()).collect();
    assert!(
        referenced.contains(&original_content_id),
        "the destination's own content stream {original_content_id:?} must stay \
         referenced, not be cloned into a new object; /Contents is {referenced:?}"
    );

    assert_no_orphans(&dest, "after place_page onto a page with content");
}
