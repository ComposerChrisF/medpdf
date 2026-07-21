// tests/overlay_split_operation_regression.rs
//
// Regression tests for bugs/bug-0019: an operation split across two /Contents
// fragments (operands in one, operator in the next) was silently destroyed
// because the source content streams were decoded one fragment at a time.
//
// PDF 32000-1 §7.8.2: a page's content is the concatenation of its /Contents
// streams, and the split between fragments may fall at any token boundary. The
// fix concatenates the source fragments and decodes them once, so no operation
// can straddle a parse boundary. (The destination side was already handled by
// bug-0018, which never decodes destination content at all.)

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf::pdf_overlay::overlay_page;
use medpdf::{PlacePageParams, place_page};

/// A `q` followed by the six operands of a `cm`, ending mid-operation.
const SPLIT_FRAG_1: &[u8] = b"q 2 0 0 2 10 20";
/// The `cm` operator whose operands live in the previous fragment, then more ops.
const SPLIT_FRAG_2: &[u8] = b"cm BT /F1 12 Tf ET Q";

/// Builds a one-page document whose `/Contents` is an ARRAY of the given
/// (uncompressed) fragment byte strings.
fn doc_with_content_fragments(fragments: &[&[u8]]) -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let resources_id = doc.add_object(dictionary! {});

    let frag_refs: Vec<Object> = fragments
        .iter()
        .map(|f| Object::Reference(doc.add_object(Stream::new(dictionary! {}, f.to_vec()))))
        .collect();

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
        "Contents" => Object::Array(frag_refs),
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
    (doc, page_id)
}

/// Every operation on a page. Each result fragment is decoded on its own, which is
/// sound here because the fix collapses split source content into ONE stream — so
/// no operation straddles a fragment boundary in the result.
fn all_operations(doc: &Document, page_id: ObjectId) -> Vec<Operation> {
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
        if let Ok(content) = Content::decode(&bytes) {
            ops.extend(content.operations);
        }
    }
    ops
}

#[test]
fn overlay_source_operation_split_across_fragments_survives() {
    let (overlay, _) = doc_with_content_fragments(&[SPLIT_FRAG_1, SPLIT_FRAG_2]);
    // Destination content has no `cm`, so the only `cm` in the result is the source's.
    let (mut dest, dest_page_id) = doc_with_content_fragments(&[b"q 1 0 0 rg Q"]);

    overlay_page(&mut dest, dest_page_id, &overlay, 1).expect("overlay must succeed");

    let ops = all_operations(&dest, dest_page_id);
    let cm = ops
        .iter()
        .find(|op| op.operator == "cm")
        .expect("the source's cm operation must be present after overlay");
    assert_eq!(
        cm.operands.len(),
        6,
        "the cm split across source fragments must keep all six operands (bug-0019: the old \
         per-fragment decode dropped the operands, leaving a bare zero-operand cm)"
    );
}

#[test]
fn place_page_source_operation_split_across_fragments_survives() {
    let (source, _) = doc_with_content_fragments(&[SPLIT_FRAG_1, SPLIT_FRAG_2]);
    let (mut dest, dest_page_id) = doc_with_content_fragments(&[b"q 1 0 0 rg Q"]);

    place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    )
    .expect("place must succeed");

    let ops = all_operations(&dest, dest_page_id);
    let cms: Vec<&Operation> = ops.iter().filter(|op| op.operator == "cm").collect();
    // place_page emits its own transform `cm` (6 operands); the source's `cm`
    // (also 6) must survive alongside it.
    assert!(
        cms.len() >= 2,
        "both the placement transform cm and the source cm should be present, got {}",
        cms.len()
    );
    assert!(
        cms.iter().all(|op| op.operands.len() == 6),
        "every cm must have six operands; a zero-operand cm means the split source operation was \
         dropped (bug-0019)"
    );
}
