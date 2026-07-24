// tests/recompress_mask_regression.rs
//
// Regression test for bugs/bug-0028: image recompression skipped /SMask (soft masks) but
// not /Mask. Color-key masking (/Mask as an array of sample ranges) marks exact sample
// values transparent; lossy JPEG recompression drifts samples across those boundaries —
// losing transparency where samples leave the range, punching new holes where they drift
// in — while /Mask stays in the dict over the shifted data. Silent corruption reported as
// a successful optimization.
//
// The fix skips any image carrying /Mask (color-key array or stencil reference),
// symmetric with the /SMask policy.
//
// To confirm it pins the fix, set MEDPDF_TEMP_BUG0028 (a temporary guard bypassing the
// /Mask check): the masked image is then scanned/recompressed.

use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf_image::recompress::{RecompressParams, recompress_images};

const W: i64 = 200;
const H: i64 = 200;

fn add_flate_image(doc: &mut Document, mask: Option<Object>) -> ObjectId {
    let plain = vec![128u8; (W * H * 3) as usize];
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => W,
            "Height" => H,
            "BitsPerComponent" => 8,
            "ColorSpace" => "DeviceRGB",
        },
        plain,
    );
    stream.compress().unwrap();
    if let Some(m) = mask {
        stream.dict.set("Mask", m);
    }
    doc.add_object(Object::Stream(stream))
}

fn params() -> RecompressParams {
    RecompressParams {
        quality: 85,
        min_size: 0,
    }
}

#[test]
fn color_key_masked_image_is_skipped() {
    // /Mask as a color-key range array.
    let mut doc = Document::with_version("1.7");
    let mask = Object::Array(
        [250, 255, 250, 255, 250, 255]
            .into_iter()
            .map(Object::Integer)
            .collect(),
    );
    let id = add_flate_image(&mut doc, Some(mask));
    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 0,
        "a /Mask (color-key) image must be skipped — lossy JPEG breaks color-key \
         transparency — bug-0028"
    );
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    assert_eq!(
        stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"FlateDecode",
        "skipped image's /Filter must remain FlateDecode — bug-0028"
    );
}

#[test]
fn stencil_mask_reference_image_is_skipped() {
    // /Mask as a reference (stencil-mask image) — skipped for symmetry with SMask.
    let mut doc = Document::with_version("1.7");
    let dangling = ObjectId::from((9999u32, 0u16));
    let id = add_flate_image(&mut doc, Some(Object::Reference(dangling)));
    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 0,
        "a /Mask reference image must be skipped, symmetric with the /SMask policy — bug-0028"
    );
}

#[test]
fn unmasked_image_is_not_over_skipped() {
    // Control: no /Mask means the guard must let the image through to scanning.
    let mut doc = Document::with_version("1.7");
    let id = add_flate_image(&mut doc, None);
    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 1,
        "an unmasked FlateDecode image must be scanned, not skipped — bug-0028 must not \
         over-skip"
    );
}
