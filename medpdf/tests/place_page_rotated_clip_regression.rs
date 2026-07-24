// tests/place_page_rotated_clip_regression.rs
//
// Regression test for bugs/bug-0027: with clip = true and a rotation that is not a
// multiple of 90°, place_page emitted the axis-aligned bounding box of the transformed
// MediaBox corners as the clip rect. The AABB strictly contains the rotated page, so
// source content outside the MediaBox (bleed, crop-hidden artwork) leaks through the four
// corner wedges — the exact thing clipping exists to suppress. types.rs documents "clip
// source content to its MediaBox".
//
// The fix emits the transformed MediaBox quadrilateral (m/l/l/l/h) as the clip path for
// arbitrary angles, while the 90°-step cases (whose AABB equals the rect) keep the
// compact `re`.
//
// To confirm it pins the fix, set MEDPDF_TEMP_BUG0027 (a temporary guard that forces the
// AABB for every angle): the quad-path assertions then fail.

mod fixtures;

use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use medpdf::place_page;
use medpdf::types::PlacePageParams;

fn obj_to_f32(obj: &Object) -> f32 {
    match obj {
        Object::Real(r) => *r,
        Object::Integer(i) => *i as f32,
        _ => panic!("expected numeric operand, got {obj:?}"),
    }
}

/// All content-stream operations of a page, decoding each stream separately.
fn collect_all_ops(doc: &Document, page_id: ObjectId) -> Vec<lopdf::content::Operation> {
    let page = doc.get_dictionary(page_id).unwrap();
    let refs = match page.get(b"Contents").unwrap() {
        Object::Array(arr) => arr.clone(),
        r @ Object::Reference(_) => vec![r.clone()],
        _ => panic!("unexpected Contents type"),
    };
    let mut ops = Vec::new();
    for r in &refs {
        let id = r.as_reference().unwrap();
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
fn rotated_clip_is_transformed_quad_not_aabb() {
    // Source page (letter MediaBox) with some content; place at (100, 100), 45°, scale 1,
    // clip on. The clip is built from the source MediaBox, read back below.
    let source = fixtures::create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let mut dest = fixtures::create_pdf_with_pages(1);
    let dest_page_id = fixtures::get_first_page_id(&dest);
    let params = PlacePageParams::new(100.0, 100.0, 1.0).rotation(45.0);
    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

    let ops = collect_all_ops(&dest, dest_page_id);
    let w_idx = ops
        .iter()
        .position(|op| op.operator == "W")
        .expect("a clip (W) op must be present");

    // The clip path immediately precedes `W`. Post-fix it is a closed quad: m l l l h.
    // Pre-fix it was a single `re` (the AABB).
    assert_eq!(
        ops[w_idx - 1].operator,
        "h",
        "the clip before W must be a closed path (h), not an AABB rectangle — bug-0027"
    );
    assert_ne!(
        ops[w_idx - 1].operator,
        "re",
        "a non-90° rotation must not clip to an axis-aligned `re` — bug-0027"
    );
    let path = &ops[w_idx - 5..w_idx];
    assert_eq!(path[0].operator, "m", "quad must start with a moveto");
    assert!(
        path[1..4].iter().all(|op| op.operator == "l"),
        "quad must have three linetos"
    );

    // The 4 path points must be the source MediaBox corners transformed by the placement
    // matrix (a=d=cos45, b=-c=sin45, tx=ty=100). Read the actual source MediaBox rather
    // than assume a size, and apply the same transform place_page uses.
    let src_page_id = fixtures::get_first_page_id(&source);
    let mbox = source
        .get_dictionary(src_page_id)
        .unwrap()
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap();
    let (x0, y0, x1, y1) = (
        obj_to_f32(&mbox[0]),
        obj_to_f32(&mbox[1]),
        obj_to_f32(&mbox[2]),
        obj_to_f32(&mbox[3]),
    );
    let cs = std::f32::consts::FRAC_1_SQRT_2; // cos45 = sin45
    let tf = |sx: f32, sy: f32| (cs * sx - cs * sy + 100.0, cs * sx + cs * sy + 100.0);
    let expected = [tf(x0, y0), tf(x1, y0), tf(x1, y1), tf(x0, y1)];
    let got: Vec<(f32, f32)> = path[..4]
        .iter()
        .map(|op| (obj_to_f32(&op.operands[0]), obj_to_f32(&op.operands[1])))
        .collect();
    for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
        assert!(
            (g.0 - e.0).abs() < 0.05 && (g.1 - e.1).abs() < 0.05,
            "clip quad corner {i} must be the transformed MediaBox corner: got {g:?}, \
             expected {e:?} — bug-0027"
        );
    }
}
