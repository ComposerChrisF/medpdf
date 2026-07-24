// tests/recompress_predictor_regression.rs
//
// Regression test for bugs/bug-0029: image recompression checked /Filter but never
// /DecodeParms. lopdf's decompressed_content() un-applies PNG predictors (10-15) but
// hands TIFF Predictor 2 data back still horizontally differenced; recompress then
// JPEG-encoded those differences as if they were pixels — total, permanent corruption
// (set_plain_content also drops /DecodeParms on rewrite) — reported as success.
//
// The fix reads /DecodeParms and skips any image whose Predictor is not 1 or 10-15, or
// whose /DecodeParms cannot be resolved (unknown means skip, never assume raw pixels).
//
// To confirm the skip-Predictor-2 case pins the fix, set MEDPDF_TEMP_BUG0029 (a temporary
// guard bypassing the check): the Predictor-2 image is then recompressed (corrupted).

use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf_image::recompress::{RecompressParams, recompress_images};

const W: i64 = 200;
const H: i64 = 200;

/// Adds a FlateDecode RGB image XObject (200×200×3 raw pixels) and returns its id.
/// `decode_parms`, if given, is set as /DecodeParms.
fn add_flate_image(doc: &mut Document, decode_parms: Option<Object>) -> ObjectId {
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
    stream.compress().unwrap(); // → /Filter /FlateDecode, content = zlib(plain)
    if let Some(dp) = decode_parms {
        stream.dict.set("DecodeParms", dp);
    }
    doc.add_object(Object::Stream(stream))
}

/// min_size 0 so the tiny (uniform, highly compressible) content is not skipped by the
/// size threshold — the point is the predictor guard, not the size gate.
fn params() -> RecompressParams {
    RecompressParams {
        quality: 85,
        min_size: 0,
    }
}

// `scanned` is incremented the instant extract_image_info accepts an image (before any
// size/JPEG heuristic), so it directly reflects the predictor guard's decision — unlike
// `recompressed`, which additionally depends on the JPEG being smaller.

#[test]
fn predictor2_image_is_skipped() {
    let mut doc = Document::with_version("1.7");
    let id = add_flate_image(
        &mut doc,
        Some(Object::Dictionary(dictionary! {
            "Predictor" => 2,
            "Colors" => 3,
            "Columns" => W,
            "BitsPerComponent" => 8,
        })),
    );
    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 0,
        "a TIFF Predictor 2 image must be skipped before scanning (its bytes are still \
         differenced) — bug-0029"
    );
    assert_eq!(stats.recompressed, 0, "and never recompressed — bug-0029");
    // Filter must be untouched by the skip.
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    assert_eq!(
        stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"FlateDecode",
        "skipped image's /Filter must remain FlateDecode — bug-0029"
    );
}

#[test]
fn unresolvable_decodeparms_is_skipped() {
    // A /DecodeParms that references a nonexistent object cannot be proven raw → skip.
    let mut doc = Document::with_version("1.7");
    let dangling = ObjectId::from((9999u32, 0u16));
    let id = add_flate_image(&mut doc, Some(Object::Reference(dangling)));
    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 0,
        "an unresolvable /DecodeParms must be skipped (positive evidence of absence) — bug-0029"
    );
}

#[test]
fn image_without_decodeparms_is_not_over_skipped() {
    // Control: no /DecodeParms means raw pixels — the guard must let the image through to
    // scanning (whether the JPEG ends up smaller is a separate size heuristic).
    let mut doc = Document::with_version("1.7");
    let id = add_flate_image(&mut doc, None);
    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 1,
        "a plain FlateDecode image (no predictor) must be scanned, not skipped — the \
         bug-0029 guard must not over-skip"
    );
}

#[test]
fn png_predictor_image_is_not_over_skipped() {
    // PNG predictors (10-15) are safe: lopdf un-applies them and strips the parms, so the
    // guard must let them through. Build genuinely PNG-predicted content: each row is a
    // filter-type byte (0 = None) followed by its raw pixels — a valid PNG-predictor
    // stream lopdf decodes back to raw pixels.
    let row_bytes = (W * 3) as usize;
    let mut predicted = Vec::with_capacity(H as usize * (1 + row_bytes));
    for _ in 0..H {
        predicted.push(0u8); // PNG filter type: None
        predicted.extend(std::iter::repeat_n(128u8, row_bytes));
    }
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => W,
            "Height" => H,
            "BitsPerComponent" => 8,
            "ColorSpace" => "DeviceRGB",
        },
        predicted,
    );
    stream.compress().unwrap();
    stream.dict.set(
        "DecodeParms",
        Object::Dictionary(dictionary! {
            "Predictor" => 15,
            "Colors" => 3,
            "Columns" => W,
            "BitsPerComponent" => 8,
        }),
    );
    let mut doc = Document::with_version("1.7");
    let id = doc.add_object(Object::Stream(stream));

    let stats = recompress_images(&mut doc, &[id], &params()).unwrap();
    assert_eq!(
        stats.scanned, 1,
        "a PNG-predictor (10-15) image must be scanned, not skipped — lopdf un-applies it \
         to raw pixels (bug-0029 must not over-skip PNG predictors)"
    );
}
