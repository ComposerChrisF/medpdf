//! Place a source PDF page onto a destination page at a specific position, scale,
//! and rotation.
//!
//! Unlike `overlay_page()` which always places at (0,0) with no scaling,
//! `place_page()` applies a translate + uniform scale transform, enabling
//! callers to implement booklet imposition and N-up layouts.
//!
//! |          | `overlay_page()`           | `place_page()`             |
//! |----------|----------------------------|----------------------------|
//! | Position | Always (0, 0)              | Configurable (x, y)        |
//! | Scaling  | None (1:1)                 | Uniform scale factor       |
//! | Rotation | None                       | Arbitrary angle            |
//! | Use case | Full-page overlays         | Imposition (booklet, N-up) |
//!
//! Both share the resource-copying and renaming machinery in `pdf_overlay_helpers`
//! (source resources are suffixed `_p` here to avoid collisions); `place_page()` adds
//! the transform and an optional clip.
//!
//! # Division of labor
//!
//! medpdf provides only the primitive: place one page onto another with a transform.
//! All page-ordering logic — booklet imposition order, back-cover preservation, N-up
//! normal vs. multi-copy mode, padding — lives in the callers (pdf-maker's `--booklet`
//! and `--n-up`, pdf-orchestrator's `<Booklet>` and `<NUp>` elements).
//!
//! # The placement contract
//!
//! **A placed page's bounding box has its lower-left corner at exactly
//! `(params.x, params.y)`, and its size is
//! [`placed_page_size`]`(source_doc, page, scale, rotation)`** — for any MediaBox
//! origin, any source `/Rotate`, and any placement rotation. Two consequences,
//! both deliberate, both settled by ruling on 2026-07-24 and implemented in
//! v0.13.0:
//!
//! - **Placement is by visible box, not by user space** (bug-0024). The MediaBox
//!   origin is compensated out of the translation, so `(x, y, scale)` alone says
//!   where the page lands and a caller never reads the source's origin. Before
//!   v0.13.0, `tx = params.x` mapped source *user space* `(0, 0)` to `(x, y)`, so
//!   a cropped page landed offset by `scale × origin`.
//! - **The source page's `/Rotate` is honored** (bug-0023). What gets placed is the
//!   page as a viewer displays it, and its effective width and height are swapped
//!   under `/Rotate` 90/270. Before v0.13.0 nothing read `/Rotate`, so a landscape
//!   scan imposed sideways — and, worse for any caller deriving a grid from the
//!   page size, with rows and columns transposed.
//!
//! One observable consequence, load-bearing enough to state rather than leave to
//! be rediscovered: with `clip` enabled (the default) and a total rotation that is
//! a 90° step — which covers every unrotated placement and every quarter-turn —
//! the emitted clip rectangle is exactly `x y w h re`, where `(w, h)` is
//! [`placed_page_size`]. Decoding that one operator therefore pins the whole
//! contract, and pdf-orchestrator's placement tests do exactly that. An arbitrary
//! angle emits the transformed quadrilateral instead (bug-0027), whose bounding
//! box is the same rectangle.
//!
//! Callers doing grid arithmetic (N-up slots, tile columns and rows) should size
//! against [`placed_page_size`] rather than
//! [`get_page_media_box`](crate::get_page_media_box), which is the *pre-rotation*
//! box; [`get_page_effective_size`](crate::get_page_effective_size) is the
//! scale-free form of the same number.
//!
//! # Transform
//!
//! PDF's `cm` operator takes a 6-element matrix `[a b c d e f]`. `/Rotate r` means
//! "rotate `r` degrees clockwise when displayed" (PDF 32000-1 §7.7.3.3) and
//! `rotation` is counterclockwise, so the total counterclockwise angle is
//! θ = `rotation − r`. The scale is uniform, so rotation and scale commute and the
//! whole linear part is `s · R(θ)`:
//!
//! ```text
//! a =  s·cos θ    b = s·sin θ
//! c = −s·sin θ    d = s·cos θ
//! e =  x − min_x  f = y − min_y
//! ```
//!
//! where `(min_x, min_y)` is the minimum corner of the MediaBox under the linear
//! part alone — the compensation that lands the visible box at `(x, y)`. Exact
//! coefficients are substituted when θ is a 90° step, so the common cases stay free
//! of trig rounding. With `clip` enabled (the default) the transformed MediaBox
//! precedes the `cm` as a `W n` clip path — a compact `re` for a 90°-step θ, the
//! transformed quadrilateral otherwise (bug-0027) — so a placed page cannot bleed
//! into an adjacent N-up slot.
//!
//! `compute_placement_transform` is the single definition of all of this;
//! `place_page` emits it and [`placed_page_size`] reports it, so the geometry a
//! caller plans against and the geometry that lands cannot drift apart.

use crate::error::{MedpdfError, Result};
use crate::pdf_helpers::{self, KEY_CONTENTS, KEY_PAGES};
use crate::pdf_overlay_helpers::{
    accumulate_dictionary_keys, isolate_dest_content_streams, merge_resources_into_dest_page,
    normalize_resource_subdicts, rename_resources_in_dict, rename_source_content_streams,
    resolve_contents_to_ref_array,
};
use crate::types::PlacePageParams;
use log::{debug, trace};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The fully-composed transform for one placement.
///
/// Built by [`compute_placement_transform`], which is the single place the
/// placement geometry is defined — `place_page` emits it, and
/// [`placed_page_size`] reports it, so the two can never disagree about where a
/// page lands or how much room it takes.
pub(crate) struct PlacementTransform {
    /// `cm` operands `[a b c d tx ty]`.
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
    /// The transformed MediaBox corners in destination space, in MediaBox order
    /// (`x0y0`, `x1y0`, `x1y1`, `x0y1`).
    pub quad: [(f64, f64); 4],
    /// True when the total rotation is a 90° step, so `quad` is itself an
    /// axis-aligned rectangle (AABB == the rect) and the clip can stay a compact
    /// `re`.
    pub axis_aligned: bool,
    /// Width and height of the placed footprint — the `quad`'s bounding box.
    pub width: f64,
    pub height: f64,
}

/// Composes the source page's `/Rotate`, the requested scale and rotation, and the
/// translation that lands the placed page's visible box at `(x, y)`.
///
/// # The two rulings this encodes
///
/// - **Source `/Rotate` is honored** (bug-0023). `/Rotate r` means "rotate `r`
///   degrees *clockwise* when displayed" (PDF 32000-1 §7.7.3.3), and `rotation`
///   is counterclockwise, so the total counterclockwise angle is
///   `rotation − source_rotate`. Because the scale is uniform, rotation and scale
///   commute and the whole linear part collapses to `s · R(rotation − rotate)` —
///   the exact 90°-step coefficients still apply to the common cases.
/// - **Placement is by visible box** (bug-0024). The translation is
///   `(x, y) − (min_x, min_y)` of the linearly-transformed MediaBox, so the
///   placed page's bounding box has its lower-left corner at exactly `(x, y)`
///   for any MediaBox origin and any rotation. A caller never reads the source
///   MediaBox origin, and a rotated placement no longer swings off the sheet.
pub(crate) fn compute_placement_transform(
    media_box: [f32; 4],
    source_rotate: u32,
    x: f64,
    y: f64,
    scale: f64,
    rotation: f64,
) -> PlacementTransform {
    let s = scale;
    let theta = (rotation - source_rotate as f64).rem_euclid(360.0);

    // Exact values for the 90° steps; trig for arbitrary angles.
    let (a, b, c, d, axis_aligned) = if theta.abs() < 1e-10 {
        (s, 0.0, 0.0, s, true)
    } else if (theta - 90.0).abs() < 1e-10 {
        (0.0, s, -s, 0.0, true)
    } else if (theta - 180.0).abs() < 1e-10 {
        (-s, 0.0, 0.0, -s, true)
    } else if (theta - 270.0).abs() < 1e-10 {
        (0.0, -s, s, 0.0, true)
    } else {
        let rad = theta.to_radians();
        (
            s * rad.cos(),
            s * rad.sin(),
            -s * rad.sin(),
            s * rad.cos(),
            false,
        )
    };

    let [x0, y0, x1, y1] = media_box.map(f64::from);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    // Linear part only: (sx, sy) → (a·sx + c·sy, b·sx + d·sy).
    let linear = corners.map(|(sx, sy)| (a * sx + c * sy, b * sx + d * sy));

    let min_x = linear.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = linear.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = linear.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let max_y = linear.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);

    let tx = x - min_x;
    let ty = y - min_y;

    PlacementTransform {
        a,
        b,
        c,
        d,
        tx,
        ty,
        quad: linear.map(|(px, py)| (px + tx, py + ty)),
        axis_aligned,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

/// The footprint [`place_page`] will occupy on the destination page: the width and
/// height of the placed page's bounding box, with the source's `/Rotate`, the
/// `scale`, and the placement `rotation` all applied.
///
/// This is the number to do grid arithmetic with — N-up slot sizing, `--tile`
/// column and row counts — because it is computed by the same internal transform
/// that `place_page` emits. Reading
/// [`get_page_media_box`](crate::get_page_media_box) directly gives the
/// *pre-rotation* size, which for a `/Rotate 90` source has width and height
/// swapped relative to what actually lands on the sheet.
///
/// Placement is by visible box, so the placed page occupies exactly
/// `(x, y)`–`(x + width, y + height)` for a `PlacePageParams` with this `scale`
/// and `rotation`; the MediaBox origin never enters a caller's arithmetic.
///
/// # Fitting a page to a cell
///
/// **The footprint is exactly linear in `scale`:** `placed_page_size(d, p, s, r)`
/// is `s ×` `placed_page_size(d, p, 1.0, r)`, because the scale is uniform and
/// factors out of the whole transform. So a fit-to-cell scale is one division, not
/// a fixed-point iteration:
///
/// ```no_run
/// # use lopdf::Document;
/// # fn demo(src: &Document, page_id: lopdf::ObjectId) -> Option<()> {
/// # let (cell_w, cell_h, rotation) = (306.0_f32, 396.0_f32, 0.0);
/// // Measure at scale 1 with the rotation you intend to place at, then divide.
/// let (unit_w, unit_h) = medpdf::placed_page_size(src, page_id, 1.0, rotation)?;
/// let scale = (cell_w / unit_w).min(cell_h / unit_h);
/// // placed_page_size(src, page_id, scale as f64, rotation) now fits the cell exactly.
/// # Some(())
/// # }
/// ```
///
/// Measuring at scale 1 with the intended `rotation` is the part to get right: a
/// 90° placement rotation transposes the footprint, so a fit computed from an
/// unrotated measurement will overflow the cell. When the placement rotation is 0
/// this reduces to [`get_page_effective_size`](crate::get_page_effective_size).
/// The linearity is pinned by `placed_size_is_linear_in_scale` in
/// `tests/place_page_rotate_and_origin_regression.rs`.
///
/// Returns `None` if the page has no `/MediaBox` on itself or any ancestor.
///
/// ```no_run
/// # use lopdf::Document;
/// # fn demo(src: &Document, page_id: lopdf::ObjectId) -> Option<()> {
/// // How many 8.5×11 sheets does this page need at 200%, unrotated?
/// let (w, h) = medpdf::placed_page_size(src, page_id, 2.0, 0.0)?;
/// let cols = (w / 612.0).ceil() as usize;
/// let rows = (h / 792.0).ceil() as usize;
/// # let _ = (cols, rows);
/// # Some(())
/// # }
/// ```
pub fn placed_page_size(
    doc: &Document,
    page_id: ObjectId,
    scale: f64,
    rotation: f64,
) -> Option<(f32, f32)> {
    let media_box = pdf_helpers::get_page_media_box(doc, page_id)?;
    let rotate = pdf_helpers::get_page_rotation(doc, page_id);
    let t = compute_placement_transform(media_box, rotate, 0.0, 0.0, scale, rotation);
    Some((t.width as f32, t.height as f32))
}

/// Places a source page onto a destination page at the position and scale
/// specified by `params`.
///
/// The placed page's visible box lands with its lower-left corner at
/// `(params.x, params.y)` and occupies
/// [`placed_page_size`]`(source_doc, page, params.scale, params.rotation)`, with
/// the source page's `/Rotate` honored and its MediaBox origin compensated out —
/// see the module docs for the full contract.
///
/// Each call is self-contained in its own `q ... Q` graphics state wrapper,
/// so multiple calls can safely compose on the same destination page without
/// interfering with each other or with existing destination content.
pub fn place_page(
    dest_doc: &mut Document,
    dest_page_id: ObjectId,
    source_doc: &Document,
    source_page_num: u32,
    params: &PlacePageParams,
) -> Result<()> {
    // Every placement parameter flows into `cm`/`re` operands; a NaN or infinity (e.g.
    // from a caller dividing by a zero page dimension) would serialize as the literal
    // tokens `NaN`/`inf`, which are not valid PDF numbers — a silently corrupt content
    // stream. Fail loudly instead, naming the offending field (bug-0026 extended this
    // from the scale-only check to x/y/rotation too).
    for (name, value) in [
        ("x", params.x),
        ("y", params.y),
        ("scale", params.scale),
        ("rotation", params.rotation),
    ] {
        if !value.is_finite() {
            return Err(MedpdfError::new(format!(
                "PlacePageParams {name} must be finite (got {value})"
            )));
        }
    }

    let source_page_id = pdf_helpers::get_page_object_id_from_doc(source_doc, source_page_num)?;

    // Get source MediaBox (needed for clipping)
    let media_box = pdf_helpers::get_page_media_box(source_doc, source_page_id)
        .ok_or_else(|| MedpdfError::new("Source page has no MediaBox"))?;

    let source_page = source_doc.get_dictionary(source_page_id)?;

    // Early return if source page has no /Contents (nothing to place)
    let source_contents = match source_page.get(KEY_CONTENTS) {
        Ok(contents) => contents,
        Err(_) => {
            debug!("Source page {source_page_id:?} has no /Contents; nothing to place");
            return Ok(());
        }
    };

    let mut copied_objects = BTreeMap::new();

    // Deep-copy source /Contents as ref array
    debug!("Deep-copying source /Contents for place_page");
    let source_contents_arr = resolve_contents_to_ref_array(
        dest_doc,
        Some(source_doc),
        source_contents,
        &mut copied_objects,
        &format!("Source page {source_page_id:?}"),
    )?;

    // Deep-copy the source page's effective /Resources. /Resources is inheritable —
    // resolve it up the /Parent chain rather than reading only the page dict, which
    // substituted an empty dict for inherited resources and so left the placed
    // content's font references unrenamed and bound to the wrong font (bug-0017
    // facet 2).
    debug!("Deep-copying source /Resources for place_page");
    let source_resources_dict_id = match pdf_helpers::get_page_resources(source_doc, source_page_id)
    {
        Some(Object::Dictionary(dict)) => {
            let d_new = pdf_helpers::deep_copy_object(
                dest_doc,
                source_doc,
                &Object::Dictionary(dict),
                &mut copied_objects,
            )?;
            dest_doc.add_object(d_new)
        }
        Some(Object::Reference(id)) => {
            pdf_helpers::deep_copy_object_by_id(dest_doc, source_doc, id, &mut copied_objects)?
        }
        Some(_) => {
            return Err(MedpdfError::Message(format!(
                "Source page {source_page_id:?} /Resources must be dictionary or reference"
            )));
        }
        // No resources anywhere — still place the content (it may be purely geometric).
        None => dest_doc.add_object(Object::Dictionary(Dictionary::new())),
    };

    // Inline any indirect resource-type sub-dicts (`/Font 10 0 R`) so the rename
    // and merge paths, which only understand inline sub-dicts, work (bug-0030).
    normalize_resource_subdicts(dest_doc, source_resources_dict_id)?;

    // Collect existing dest resource keys
    debug!("Accumulating destination resource keys");
    let mut keys_used = HashSet::<Vec<u8>>::new();
    accumulate_dictionary_keys(
        &mut keys_used,
        dest_doc,
        dest_doc.catalog()?.get(KEY_PAGES)?.as_reference()?,
    )?;

    // Rename source resources with _p suffix (distinct from overlay's _o)
    debug!("Renaming source resources with _p suffix");
    let mut key_mapping = HashMap::<Vec<u8>, Vec<u8>>::new();
    rename_resources_in_dict(
        &mut key_mapping,
        &mut keys_used,
        dest_doc,
        source_resources_dict_id,
        b"_p",
    )?;
    if log::log_enabled!(log::Level::Trace) {
        trace!("key_mapping:");
        for (k, v) in key_mapping.iter() {
            trace!(
                "{} => {}",
                String::from_utf8_lossy(k),
                String::from_utf8_lossy(v)
            );
        }
    }

    // Update source content streams with renamed resource references + q/Q wrapping.
    // Returns a single combined stream (fragments decoded once — bug-0019).
    debug!("Updating source content streams with renamed keys");
    let source_contents_arr =
        rename_source_content_streams(dest_doc, &source_contents_arr, &key_mapping)?;
    if log::log_enabled!(log::Level::Trace) {
        trace!("source_contents_arr: {source_contents_arr:?}");
    }

    // Build the transform-open content stream:
    //   q
    //   [clip quad/rect] W n     (if params.clip)
    //   a b c d tx ty cm         (source /Rotate + rotation + scale + translate)
    let source_rotate = pdf_helpers::get_page_rotation(source_doc, source_page_id);
    let PlacementTransform {
        a,
        b,
        c,
        d,
        tx,
        ty,
        quad,
        axis_aligned,
        ..
    } = compute_placement_transform(
        media_box,
        source_rotate,
        params.x,
        params.y,
        params.scale,
        params.rotation,
    );

    trace!("cm matrix: a={a}, b={b}, c={c}, d={d}, tx={tx}, ty={ty}");

    let mut open_ops = vec![Operation::new("q", vec![])];

    if params.clip {
        if axis_aligned {
            // 90°-step total rotation: the transformed MediaBox is axis-aligned, so
            // its AABB equals the rect — emit the compact `re` (keeps the
            // common-case output byte-stable).
            let min_x = quad.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
            let min_y = quad.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let max_x = quad.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
            let max_y = quad.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
            open_ops.push(Operation::new(
                "re",
                vec![
                    Object::Real(min_x as f32),
                    Object::Real(min_y as f32),
                    Object::Real((max_x - min_x) as f32),
                    Object::Real((max_y - min_y) as f32),
                ],
            ));
        } else {
            // Arbitrary angle: the AABB strictly contains the rotated page, so source
            // content outside the MediaBox (bleed, crop-hidden artwork — exactly what
            // clipping exists to suppress) would leak through the four corner wedges.
            // Clip to the transformed MediaBox quadrilateral itself (bug-0027).
            open_ops.push(Operation::new(
                "m",
                vec![
                    Object::Real(quad[0].0 as f32),
                    Object::Real(quad[0].1 as f32),
                ],
            ));
            for &(px, py) in &quad[1..] {
                open_ops.push(Operation::new(
                    "l",
                    vec![Object::Real(px as f32), Object::Real(py as f32)],
                ));
            }
            open_ops.push(Operation::new("h", vec![]));
        }
        open_ops.push(Operation::new("W", vec![]));
        open_ops.push(Operation::new("n", vec![]));
    }

    open_ops.push(Operation::new(
        "cm",
        vec![
            Object::Real(a as f32),
            Object::Real(b as f32),
            Object::Real(c as f32),
            Object::Real(d as f32),
            Object::Real(tx as f32),
            Object::Real(ty as f32),
        ],
    ));

    let open_content = Content {
        operations: open_ops,
    };
    let mut open_stream = Stream::new(Dictionary::new(), open_content.encode()?);
    open_stream.compress()?;
    let open_id = dest_doc.add_object(open_stream);

    // Build transform-close content stream: Q (too small to benefit from compression)
    let close_content = Content {
        operations: vec![Operation::new("Q", vec![])],
    };
    let close_stream = Stream::new(Dictionary::new(), close_content.encode()?);
    let close_id = dest_doc.add_object(close_stream);

    // Get dest page's current /Contents and append: open + source streams + close
    debug!("Appending placed content to destination page");
    let dest_contents_base = match dest_doc
        .get_object(dest_page_id)?
        .as_dict()?
        .get(KEY_CONTENTS)
    {
        Ok(dest_contents) => {
            let dest_contents = dest_contents.clone();
            resolve_contents_to_ref_array(
                dest_doc,
                None,
                &dest_contents,
                &mut copied_objects,
                &format!("Dest page {dest_page_id:?}"),
            )?
        }
        Err(_) => Vec::new(),
    };
    // Isolate the destination's own content with standalone q/Q wrapper streams,
    // so any graphics state it leaks — a top-level `cm` with no q/Q, a leftover
    // clip — is popped before the placement transform runs. Without this, a
    // dangling destination CTM displaces the placed page, contradicting this
    // function's self-containment contract (bug-0025). Uses bug-0018's mechanism,
    // which never re-encodes the destination streams (that was bug-0018's bug).
    let mut dest_contents_arr = isolate_dest_content_streams(dest_doc, dest_contents_base)?;

    dest_contents_arr.push(Object::Reference(open_id));
    for item in &source_contents_arr {
        dest_contents_arr.push(item.clone());
    }
    dest_contents_arr.push(Object::Reference(close_id));

    let dest_page_dict = dest_doc.get_object_mut(dest_page_id)?.as_dict_mut()?;
    dest_page_dict.set(KEY_CONTENTS, Object::Array(dest_contents_arr));

    // Merge renamed resources into dest /Resources
    debug!("Merging placed page resources into destination");
    merge_resources_into_dest_page(dest_doc, dest_page_id, source_resources_dict_id)?;

    Ok(())
}
