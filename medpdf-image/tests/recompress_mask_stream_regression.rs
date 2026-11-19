#![expect(
    clippy::unwrap_used,
    reason = "test helpers outside a #[test] fn; a failure there is meant to fail the test"
)]
// tests/recompress_mask_stream_regression.rs
//
// Regression test for bugs/bug-0046: recompression skipped an image that carries /SMask
// or /Mask (bug-0028), but not the mask stream itself.  A soft mask is an 8-bit
// DeviceGray FlateDecode image, so it qualified and was JPEG-encoded, turning the
// parent's alpha channel into JPEG artefacts.
//
// The mask data is pseudo-random so its JPEG really is smaller than its Flate stream;
// with flat data the "only if smaller" check would skip it and the test would pass with
// the bug still in.

use flate2::{Compression, write::ZlibEncoder};
use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf_image::recompress::{RecompressParams, recompress_images};
use std::io::Write;

const W: i64 = 200;
const H: i64 = 200;

fn noisy(len: usize) -> Vec<u8> {
    let mut val: u32 = 42;
    (0..len)
        .map(|_| {
            val = val.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (val >> 16) as u8
        })
        .collect()
}

fn add_flate_image(doc: &mut Document, color_space: &str, components: usize) -> ObjectId {
    // Encoded by hand: Stream::compress declines when Flate does not shrink noisy data.
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&noisy((W * H) as usize * components))
        .unwrap();
    let stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => W,
            "Height" => H,
            "BitsPerComponent" => 8,
            "ColorSpace" => color_space,
            "Filter" => "FlateDecode",
        },
        encoder.finish().unwrap(),
    );
    doc.add_object(Object::Stream(stream))
}

/// An RGB image whose `key` (`SMask` or `Mask`) references a DeviceGray image.
fn image_with_mask(key: &str) -> (Document, ObjectId, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let mask = add_flate_image(&mut doc, "DeviceGray", 1);
    let parent = add_flate_image(&mut doc, "DeviceRGB", 3);
    doc.get_object_mut(parent)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .dict
        .set(key, Object::Reference(mask));
    (doc, parent, mask)
}

fn params() -> RecompressParams {
    RecompressParams {
        quality: 30,
        min_size: 1,
    }
}

fn filter(doc: &Document, id: ObjectId) -> Vec<u8> {
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    stream
        .dict
        .get(b"Filter")
        .unwrap()
        .as_name()
        .unwrap()
        .to_vec()
}

#[test]
fn soft_mask_stream_is_not_jpeg_encoded() {
    let (mut doc, parent, mask) = image_with_mask("SMask");
    recompress_images(&mut doc, &[parent, mask], &params()).unwrap();
    assert_eq!(
        filter(&doc, mask),
        b"FlateDecode",
        "a soft-mask stream must stay lossless — bug-0046"
    );
    assert_eq!(filter(&doc, parent), b"FlateDecode");
}

#[test]
fn soft_mask_passed_without_its_parent_is_not_jpeg_encoded() {
    let (mut doc, _parent, mask) = image_with_mask("SMask");
    recompress_images(&mut doc, &[mask], &params()).unwrap();
    assert_eq!(
        filter(&doc, mask),
        b"FlateDecode",
        "a soft mask is skipped even when its parent is not among the ids — bug-0046"
    );
}

#[test]
fn stencil_mask_stream_is_not_jpeg_encoded() {
    let (mut doc, parent, mask) = image_with_mask("Mask");
    recompress_images(&mut doc, &[parent, mask], &params()).unwrap();
    assert_eq!(
        filter(&doc, mask),
        b"FlateDecode",
        "a stream used as /Mask must stay lossless — bug-0046"
    );
}

#[test]
fn unrelated_gray_image_is_still_recompressed() {
    // Control: the same gray image, referenced by nothing, must still be recompressed,
    // which also proves the fixture is one the size check would let through.
    let mut doc = Document::with_version("1.7");
    let gray = add_flate_image(&mut doc, "DeviceGray", 1);
    let stats = recompress_images(&mut doc, &[gray], &params()).unwrap();
    assert_eq!(stats.recompressed, 1);
    assert_eq!(filter(&doc, gray), b"DCTDecode");
}
