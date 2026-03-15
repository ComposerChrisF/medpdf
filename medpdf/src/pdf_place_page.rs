//! Place a source PDF page onto a destination page at a specific position and scale.
//!
//! Unlike `overlay_page()` which always places at (0,0) with no scaling,
//! `place_page()` applies a translate + uniform scale transform, enabling
//! callers to implement booklet imposition and N-up layouts.

use crate::error::{MedpdfError, Result};
use crate::pdf_helpers::{self, KEY_CONTENTS, KEY_PAGES, KEY_RESOURCES};
use crate::pdf_overlay_helpers::{
    accumulate_dictionary_keys, merge_resources_into_dest_page, modify_content_stream,
    rename_resources_in_dict, resolve_contents_to_ref_array,
};
use crate::types::PlacePageParams;
use log::{debug, trace};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Places a source page onto a destination page at the position and scale
/// specified by `params`.
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
    if !params.scale.is_finite() {
        return Err(MedpdfError::new("PlacePageParams scale must be finite"));
    }

    let source_page_id = pdf_helpers::get_page_object_id_from_doc(source_doc, source_page_num)?;

    // Get source MediaBox (needed for clipping)
    let media_box = pdf_helpers::get_page_media_box(source_doc, source_page_id)
        .ok_or_else(|| MedpdfError::new("Source page has no MediaBox"))?;
    let [x0, y0, x1, y1] = media_box;

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

    // Deep-copy source /Resources
    debug!("Deep-copying source /Resources for place_page");
    let source_resources = match source_page.get(KEY_RESOURCES) {
        Ok(res) => res,
        Err(_) => {
            // No resources — still place the content (it might be purely geometric)
            &Object::Dictionary(Dictionary::new())
        }
    };
    let source_resources_dict_id = match source_resources {
        Object::Dictionary(_) => {
            let d_new = pdf_helpers::deep_copy_object(
                dest_doc,
                source_doc,
                source_resources,
                &mut copied_objects,
            )?;
            dest_doc.add_object(d_new)
        }
        Object::Reference(id) => {
            pdf_helpers::deep_copy_object_by_id(dest_doc, source_doc, *id, &mut copied_objects)?
        }
        _ => {
            return Err(MedpdfError::Message(format!(
                "Source page {source_page_id:?} /Resources must be dictionary or reference"
            )))
        }
    };

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

    // Update source content streams with renamed resource references + q/Q wrapping
    debug!("Updating source content streams with renamed keys");
    modify_content_stream(dest_doc, &source_contents_arr, Some(&key_mapping))?;
    if log::log_enabled!(log::Level::Trace) {
        trace!("source_contents_arr: {source_contents_arr:?}");
    }

    // Build transform-open content stream:
    //   q
    //   [clip rect] re W n       (if params.clip)
    //   a b c d tx ty cm         (rotation + scale + translate matrix)
    let s = params.scale;
    let tx = params.x;
    let ty = params.y;

    // Rotation matrix coefficients: exact values for 90° increments, trig for arbitrary angles
    let theta = params.rotation.rem_euclid(360.0);
    let (a, b, c, d) = if (theta - 0.0).abs() < 1e-10 {
        (s, 0.0, 0.0, s)
    } else if (theta - 90.0).abs() < 1e-10 {
        (0.0, s, -s, 0.0)
    } else if (theta - 180.0).abs() < 1e-10 {
        (-s, 0.0, 0.0, -s)
    } else if (theta - 270.0).abs() < 1e-10 {
        (0.0, -s, s, 0.0)
    } else {
        let rad = theta.to_radians();
        (s * rad.cos(), s * rad.sin(), -s * rad.sin(), s * rad.cos())
    };

    trace!("cm matrix: a={a}, b={b}, c={c}, d={d}, tx={tx}, ty={ty}");

    let mut open_ops = vec![Operation::new("q", vec![])];

    if params.clip {
        // Clip rect = axis-aligned bounding box of the 4 transformed MediaBox corners.
        // Transform each corner (sx, sy) → (a*sx + c*sy + tx, b*sx + d*sy + ty).
        let corners = [
            (x0 as f64, y0 as f64),
            (x1 as f64, y0 as f64),
            (x1 as f64, y1 as f64),
            (x0 as f64, y1 as f64),
        ];
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &(sx, sy) in &corners {
            let dx = a * sx + c * sy + tx;
            let dy = b * sx + d * sy + ty;
            min_x = min_x.min(dx);
            min_y = min_y.min(dy);
            max_x = max_x.max(dx);
            max_y = max_y.max(dy);
        }
        open_ops.push(Operation::new(
            "re",
            vec![
                Object::Real(min_x as f32),
                Object::Real(min_y as f32),
                Object::Real((max_x - min_x) as f32),
                Object::Real((max_y - min_y) as f32),
            ],
        ));
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
    let mut dest_contents_arr = match dest_doc
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
