// tests/place_page_isolation_regression.rs
//
// Regression test for bugs/bug-0025: place_page appended its placement (open q+cm,
// source, close Q) after the destination content without neutralizing it. A
// destination page whose content leaks graphics state — a top-level `cm` with no
// q/Q, as scanned pages commonly emit — then displaced the placed page, breaking
// place_page's own documented self-containment contract.
//
// The fix isolates the destination content with standalone q/Q wrapper streams
// (bug-0018's mechanism, which never re-encodes the destination streams), the same
// way the overlay and watermark paths do, so any leaked state is popped before the
// placement transform runs.

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf::{PlacePageParams, place_page};

/// Builds a one-page doc with `content` as its single content stream, plus a
/// MediaBox (place_page requires one on the source).
fn single_page_doc(content: &[u8]) -> (Document, ObjectId) {
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
    (doc, page_id)
}

/// The decoded operations of each `/Contents` fragment, in order (decompressing).
fn content_fragments_ops(doc: &Document, page_id: ObjectId) -> Vec<Vec<Operation>> {
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
    ids.into_iter()
        .map(|id| {
            let stream = doc.get_object(id).unwrap().as_stream().unwrap();
            let bytes = if stream.is_compressed() {
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone())
            } else {
                stream.content.clone()
            };
            Content::decode(&bytes)
                .map(|c| c.operations)
                .unwrap_or_default()
        })
        .collect()
}

fn op_num(o: &Object) -> f64 {
    o.as_float()
        .map(|f| f as f64)
        .or_else(|_| o.as_i64().map(|i| i as f64))
        .unwrap_or(f64::NAN)
}

fn operators(frag: &[Operation]) -> Vec<&str> {
    frag.iter().map(|op| op.operator.as_str()).collect()
}

#[test]
fn place_page_isolates_leaked_destination_ctm() {
    // Destination content leaks a 2x CTM with no q/Q — legal, and common in
    // scanned-page content.
    let (mut dest, dest_page) = single_page_doc(b"2 0 0 2 0 0 cm\n");
    let (source, _) = single_page_doc(b"q 0 0 1 rg 0 0 100 100 re f Q\n");

    place_page(
        &mut dest,
        dest_page,
        &source,
        1,
        &PlacePageParams::new(100.0, 100.0, 1.0),
    )
    .expect("place must succeed");

    let frags = content_fragments_ops(&dest, dest_page);

    // Locate the destination's leaked `2 0 0 2 0 0 cm`.
    let dest_idx = frags
        .iter()
        .position(|ops| {
            ops.iter()
                .any(|op| op.operator == "cm" && (op_num(&op.operands[0]) - 2.0).abs() < 1e-6)
        })
        .expect("the destination's 2x cm must survive as a fragment");

    // A standalone `q` fragment must precede it, and a balancing `Q` fragment
    // must follow it — the destination's leaked state, bracketed (bug-0025).
    assert!(
        dest_idx >= 1,
        "an isolation q fragment must precede the destination content"
    );
    assert_eq!(
        operators(&frags[dest_idx - 1]),
        vec!["q"],
        "the fragment before the destination content must be a lone q"
    );
    assert!(
        !frags[dest_idx + 1].is_empty() && frags[dest_idx + 1].iter().all(|op| op.operator == "Q"),
        "the fragment after the destination content must be the balancing Q(s), got {:?}",
        operators(&frags[dest_idx + 1])
    );

    // The placement transform (cm with tx=100) must run AFTER the destination's
    // leaked state is popped — otherwise the dangling 2x CTM displaces it.
    let place_idx = frags
        .iter()
        .position(|ops| {
            ops.iter().any(|op| {
                op.operator == "cm"
                    && op.operands.len() >= 6
                    && (op_num(&op.operands[4]) - 100.0).abs() < 1e-6
            })
        })
        .expect("the placement transform (cm tx=100) must be present");
    assert!(
        place_idx > dest_idx + 1,
        "the placement transform must run after the destination content is popped, \
         but dest cm is fragment {dest_idx} and placement cm is fragment {place_idx}"
    );
}
