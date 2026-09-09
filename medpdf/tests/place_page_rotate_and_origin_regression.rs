//! Regression tests for bug-0023 (`place_page` ignored the source page's
//! `/Rotate`) and bug-0024 (`(x, y)` addressed source *user space*, so a
//! non-zero-origin MediaBox landed offset by `scale × origin`).
//!
//! Both rulings landed together because they share one transform, and together
//! they define a single contract that these tests pin:
//!
//! > The placed page's bounding box has its lower-left corner at exactly
//! > `(params.x, params.y)`, and its size is
//! > `placed_page_size(doc, page, scale, rotation)` — for any MediaBox origin,
//! > any `/Rotate`, and any placement rotation.
//!
//! A caller therefore never reads the source MediaBox origin (bug-0024) and
//! never computes its own `/Rotate` width/height swap (bug-0023). The clip
//! rectangle, which is built from the same transform, is what the assertions
//! read back.

mod fixtures;

use fixtures::{create_pdf_with_content, create_pdf_with_pages, get_first_page_id};
use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use medpdf::{
    PlacePageParams, get_page_effective_size, place_page, placed_page_size, set_page_rotation,
};

fn obj_to_f32(obj: &Object) -> f32 {
    match obj {
        Object::Real(r) => *r,
        Object::Integer(i) => *i as f32,
        _ => panic!("expected numeric operand, got {obj:?}"),
    }
}

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

/// The bounding box of the emitted clip path — `(x, y, w, h)`. Reads either
/// form: the compact `re` used for 90°-step rotations, or the `m l l l h` quad
/// used for arbitrary angles (bug-0027).
fn clip_bbox(doc: &Document, page_id: ObjectId) -> (f32, f32, f32, f32) {
    let ops = collect_all_ops(doc, page_id);
    let w_idx = ops
        .iter()
        .position(|op| op.operator == "W")
        .expect("clip is on by default; a W op must be present");

    if ops[w_idx - 1].operator == "re" {
        let o = &ops[w_idx - 1].operands;
        return (
            obj_to_f32(&o[0]),
            obj_to_f32(&o[1]),
            obj_to_f32(&o[2]),
            obj_to_f32(&o[3]),
        );
    }
    let pts: Vec<(f32, f32)> = ops[w_idx - 5..w_idx - 1]
        .iter()
        .map(|op| (obj_to_f32(&op.operands[0]), obj_to_f32(&op.operands[1])))
        .collect();
    let min_x = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

/// A one-page source with the given MediaBox and `/Rotate`.
fn source_page(media_box: [f32; 4], rotate: u32) -> Document {
    let mut doc = create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let page_id = get_first_page_id(&doc);
    let mb: Vec<Object> = media_box.iter().map(|v| Object::Real(*v)).collect();
    doc.get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("MediaBox", Object::Array(mb));
    set_page_rotation(&mut doc, page_id, rotate).unwrap();
    doc
}

#[test]
fn source_rotate_swaps_the_placed_footprint() {
    // bug-0023: a /Rotate 90 portrait page is displayed landscape, so it must
    // impose landscape — 792 wide by 612 tall, not the raw MediaBox extents.
    let source = source_page([0.0, 0.0, 612.0, 792.0], 90);
    let src_page_id = get_first_page_id(&source);

    assert_eq!(
        get_page_effective_size(&source, src_page_id),
        Some((792.0, 612.0)),
        "a /Rotate 90 page's displayed size has width and height swapped"
    );
    assert_eq!(
        placed_page_size(&source, src_page_id, 1.0, 0.0),
        Some((792.0, 612.0)),
        "the placed footprint must agree with the displayed size"
    );

    let mut dest = create_pdf_with_pages(1);
    let dest_page_id = get_first_page_id(&dest);
    place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0),
    )
    .unwrap();

    let (x, y, w, h) = clip_bbox(&dest, dest_page_id);
    assert!(
        x.abs() < 0.01 && y.abs() < 0.01,
        "placed at (0,0), got ({x},{y})"
    );
    assert!(
        (w - 792.0).abs() < 0.01 && (h - 612.0).abs() < 0.01,
        "a /Rotate 90 source must occupy 792×612, got {w}×{h} — bug-0023"
    );
}

#[test]
fn source_rotate_composes_with_the_placement_rotation() {
    // /Rotate 90 (clockwise when displayed) cancelled by a 90° counterclockwise
    // placement rotation leaves the raw page orientation: 612×792 again.
    let source = source_page([0.0, 0.0, 612.0, 792.0], 90);
    let src_page_id = get_first_page_id(&source);
    assert_eq!(
        placed_page_size(&source, src_page_id, 1.0, 90.0),
        Some((612.0, 792.0))
    );

    let mut dest = create_pdf_with_pages(1);
    let dest_page_id = get_first_page_id(&dest);
    place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(0.0, 0.0, 1.0).rotation(90.0),
    )
    .unwrap();

    let (_, _, w, h) = clip_bbox(&dest, dest_page_id);
    assert!(
        (w - 612.0).abs() < 0.01 && (h - 792.0).abs() < 0.01,
        "/Rotate 90 plus a 90° placement rotation cancel; got {w}×{h}"
    );
}

#[test]
fn nonzero_origin_lands_at_the_requested_point() {
    // bug-0024: the visible box goes where the caller asked, without the caller
    // ever reading the MediaBox origin.
    let source = source_page([50.0, 100.0, 662.0, 892.0], 0);
    let mut dest = create_pdf_with_pages(1);
    let dest_page_id = get_first_page_id(&dest);
    place_page(
        &mut dest,
        dest_page_id,
        &source,
        1,
        &PlacePageParams::new(37.0, 11.0, 1.0),
    )
    .unwrap();

    let (x, y, w, h) = clip_bbox(&dest, dest_page_id);
    assert!(
        (x - 37.0).abs() < 0.01 && (y - 11.0).abs() < 0.01,
        "visible box must land at (37, 11), got ({x}, {y}) — bug-0024"
    );
    assert!((w - 612.0).abs() < 0.01 && (h - 792.0).abs() < 0.01);
}

#[test]
fn visible_box_contract_holds_across_rotations_origins_and_scales() {
    // The whole contract as one sweep: whatever the /Rotate, the MediaBox origin,
    // the scale, and the placement rotation, the placed page's bounding box is
    // (x, y) — (x + w, y + h) with (w, h) = placed_page_size(...).
    let media_boxes = [
        [0.0, 0.0, 612.0, 792.0],
        [50.0, 100.0, 662.0, 892.0],
        [-30.0, -40.0, 582.0, 752.0],
    ];
    for media_box in media_boxes {
        for rotate in [0, 90, 180, 270] {
            for rotation in [0.0, 90.0, 45.0, -90.0, 180.0] {
                for scale in [1.0, 0.5, 2.0] {
                    let source = source_page(media_box, rotate);
                    let src_page_id = get_first_page_id(&source);
                    let (want_w, want_h) =
                        placed_page_size(&source, src_page_id, scale, rotation).unwrap();

                    let mut dest = create_pdf_with_pages(1);
                    let dest_page_id = get_first_page_id(&dest);
                    let params = PlacePageParams::new(70.0, 130.0, scale).rotation(rotation);
                    place_page(&mut dest, dest_page_id, &source, 1, &params).unwrap();

                    let (x, y, w, h) = clip_bbox(&dest, dest_page_id);
                    let case = format!(
                        "mb={media_box:?} /Rotate {rotate} rotation {rotation} scale {scale}"
                    );
                    assert!(
                        (x - 70.0).abs() < 0.05 && (y - 130.0).abs() < 0.05,
                        "{case}: visible box must land at (70, 130), got ({x}, {y})"
                    );
                    assert!(
                        (w - want_w).abs() < 0.05 && (h - want_h).abs() < 0.05,
                        "{case}: footprint must match placed_page_size {want_w}×{want_h}, \
                         got {w}×{h}"
                    );
                }
            }
        }
    }
}

#[test]
fn effective_size_and_placed_size_agree_for_every_rotate() {
    // The two caller-facing size helpers must never drift: at scale 1 with no
    // placement rotation they are the same number, by construction.
    for rotate in [0, 90, 180, 270] {
        let source = source_page([50.0, 100.0, 662.0, 892.0], rotate);
        let page_id = get_first_page_id(&source);
        assert_eq!(
            get_page_effective_size(&source, page_id),
            placed_page_size(&source, page_id, 1.0, 0.0),
            "helpers disagree for /Rotate {rotate}"
        );
    }
}
