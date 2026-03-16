//! Image recompression: FlateDecode → DCTDecode (JPEG).
//!
//! Word on Mac's "Save As PDF" re-encodes JPEG images as FlateDecode streams,
//! bloating file sizes. This module finds qualifying image XObjects and
//! re-encodes them as JPEG.

use lopdf::{Document, Object, ObjectId};
use medpdf::{MedpdfError, Result};
use std::io::Cursor;

/// Parameters controlling image recompression.
#[derive(Debug, Clone)]
pub struct RecompressParams {
    /// JPEG quality (1–100). Default: 85.
    pub quality: u8,
    /// Minimum FlateDecode stream size in bytes to consider. Default: 50,000.
    pub min_size: usize,
}

impl Default for RecompressParams {
    fn default() -> Self {
        Self {
            quality: 85,
            min_size: 50_000,
        }
    }
}

/// Statistics from a recompression pass.
#[derive(Debug, Default)]
pub struct RecompressStats {
    pub scanned: u32,
    pub recompressed: u32,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Metadata extracted from a qualifying image stream (phase 1, immutable borrow).
struct ImageInfo {
    width: u32,
    height: u32,
    components: u8,
    pixels: Vec<u8>,
    orig_compressed_size: usize,
}

/// Recompress qualifying FlateDecode image XObjects within `doc`.
///
/// Only objects whose IDs are in `object_ids` are considered. This allows
/// callers to scope recompression to just-copied objects.
pub fn recompress_images(
    doc: &mut Document,
    object_ids: &[ObjectId],
    params: &RecompressParams,
) -> Result<RecompressStats> {
    let mut stats = RecompressStats::default();

    for &id in object_ids {
        // Phase 1: immutable borrow — inspect and decompress
        let info = match extract_image_info(doc, id, params.min_size) {
            Some(info) => info,
            None => continue,
        };
        stats.scanned += 1;

        // Sanity check: decoded pixel count must match dimensions
        let expected_bytes = info.width as usize * info.height as usize * info.components as usize;
        if info.pixels.len() != expected_bytes {
            continue;
        }

        // Encode as JPEG
        let jpeg_bytes = match encode_jpeg(&info.pixels, info.width, info.height, info.components, params.quality) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        // Only replace if JPEG is actually smaller
        if jpeg_bytes.len() >= info.orig_compressed_size {
            continue;
        }

        // Phase 2: mutable borrow — replace stream content
        let stream = doc
            .get_object_mut(id)
            .ok()
            .and_then(|obj| obj.as_stream_mut().ok());
        let Some(stream) = stream else { continue };

        let before = info.orig_compressed_size as u64;
        let after = jpeg_bytes.len() as u64;

        stream.set_plain_content(jpeg_bytes);
        stream
            .dict
            .set("Filter", Object::Name(b"DCTDecode".to_vec()));
        // Normalize colorspace to simple DeviceRGB/DeviceGray (drops ICCBased reference)
        let cs_name = if info.components == 3 {
            "DeviceRGB"
        } else {
            "DeviceGray"
        };
        stream
            .dict
            .set("ColorSpace", Object::Name(cs_name.as_bytes().to_vec()));

        stats.recompressed += 1;
        stats.bytes_before += before;
        stats.bytes_after += after;
    }

    Ok(stats)
}

/// Inspect an object and extract image info if it qualifies for recompression.
fn extract_image_info(doc: &Document, id: ObjectId, min_size: usize) -> Option<ImageInfo> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;

    // Must be /Subtype /Image
    let subtype = stream.dict.get(b"Subtype").ok()?;
    if !matches!(subtype, Object::Name(n) if n == b"Image") {
        return None;
    }

    // Must have /Filter /FlateDecode (single filter, not array of multiple)
    let filter = stream.dict.get(b"Filter").ok()?;
    match filter {
        Object::Name(n) if n == b"FlateDecode" => {}
        _ => return None,
    }

    // No /SMask (transparency)
    if stream.dict.get(b"SMask").is_ok() {
        return None;
    }

    // No /ImageMask true
    if let Ok(Object::Boolean(true)) = stream.dict.get(b"ImageMask") {
        return None;
    }

    // /BitsPerComponent must be 8
    let bpc = stream.dict.get(b"BitsPerComponent").ok()?;
    if !matches!(bpc, Object::Integer(8)) {
        return None;
    }

    // /ColorSpace must be DeviceRGB, DeviceGray, or ICCBased with 1 or 3 components
    let cs = stream.dict.get(b"ColorSpace").ok()?;
    let components: u8 = resolve_colorspace_components(doc, cs)?;

    // Width and Height
    let width = get_integer(&stream.dict, b"Width")? as u32;
    let height = get_integer(&stream.dict, b"Height")? as u32;

    // Check compressed stream size against threshold
    if stream.content.len() < min_size {
        return None;
    }

    let orig_compressed_size = stream.content.len();

    // Decompress
    let pixels = stream.decompressed_content().ok()?;

    Some(ImageInfo {
        width,
        height,
        components,
        pixels,
        orig_compressed_size,
    })
}

/// Helper to read an integer from a dictionary.
fn get_integer(dict: &lopdf::Dictionary, key: &[u8]) -> Option<i64> {
    match dict.get(key).ok()? {
        Object::Integer(n) => Some(*n),
        _ => None,
    }
}

/// Resolve a /ColorSpace value to a component count (1 for gray, 3 for RGB).
///
/// Supports:
/// - `/DeviceRGB` → 3
/// - `/DeviceGray` → 1
/// - `[/ICCBased <stream-ref>]` → reads `/N` from the ICC profile stream
/// - Indirect reference to any of the above
///
/// Returns `None` for unsupported colorspaces (Indexed, DeviceCMYK, CalRGB, Lab, etc.).
fn resolve_colorspace_components(doc: &Document, cs: &Object) -> Option<u8> {
    match cs {
        Object::Name(n) if n == b"DeviceRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" => Some(1),
        Object::Array(arr) => {
            // [/ICCBased <stream-ref>]
            let name = arr.first()?.as_name().ok()?;
            if name != b"ICCBased" {
                return None;
            }
            let profile_id = arr.get(1)?.as_reference().ok()?;
            let profile_stream = doc.get_object(profile_id).ok()?.as_stream().ok()?;
            let n = get_integer(&profile_stream.dict, b"N")?;
            match n {
                1 => Some(1),
                3 => Some(3),
                _ => None, // 4 = CMYK, skip
            }
        }
        Object::Reference(id) => {
            // Indirect reference — resolve and recurse
            let resolved = doc.get_object(*id).ok()?;
            resolve_colorspace_components(doc, resolved)
        }
        _ => None,
    }
}

/// Encode raw pixels as JPEG at the given quality.
fn encode_jpeg(pixels: &[u8], width: u32, height: u32, components: u8, quality: u8) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());

    if components == 3 {
        let img = image::RgbImage::from_raw(width, height, pixels.to_vec())
            .ok_or_else(|| MedpdfError::new("Invalid RGB pixel buffer for JPEG encoding"))?;
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
        img.write_with_encoder(encoder)
            .map_err(|e| MedpdfError::new(format!("JPEG encoding failed: {e}")))?;
    } else {
        let img = image::GrayImage::from_raw(width, height, pixels.to_vec())
            .ok_or_else(|| MedpdfError::new("Invalid Gray pixel buffer for JPEG encoding"))?;
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
        img.write_with_encoder(encoder)
            .map_err(|e| MedpdfError::new(format!("JPEG encoding failed: {e}")))?;
    }

    Ok(buf.into_inner())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};

    /// Create a FlateDecode image stream from pseudo-random pixels.
    /// Uses varied data so flate doesn't compress it to near-zero.
    fn make_flate_image(doc: &mut Document, width: u32, height: u32, components: u8) -> ObjectId {
        let pixel_count = (width * height) as usize * components as usize;
        // Generate varied pixel data — simple PRNG to avoid flate compressing to nothing
        let mut pixels = Vec::with_capacity(pixel_count);
        let mut val: u32 = 42;
        for _ in 0..pixel_count {
            val = val.wrapping_mul(1103515245).wrapping_add(12345);
            pixels.push((val >> 16) as u8);
        }

        // Compress with flate2
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&pixels).unwrap();
        let compressed = encoder.finish().unwrap();

        let cs_name = if components == 3 { "DeviceRGB" } else { "DeviceGray" };
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => cs_name,
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "Length" => compressed.len() as i64,
        };
        doc.add_object(Stream::new(dict, compressed))
    }

    #[test]
    fn recompress_flate_to_jpeg() {
        let mut doc = Document::with_version("1.7");
        let id = make_flate_image(&mut doc, 200, 200, 3);

        // Use min_size=0 because constant-value pixels compress very small
        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.recompressed, 1);
        assert!(stats.bytes_after < stats.bytes_before);

        // Verify filter changed to DCTDecode
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let filter = stream.dict.get(b"Filter").unwrap();
        assert!(matches!(filter, Object::Name(n) if n == b"DCTDecode"));
    }

    #[test]
    fn recompress_gray_image() {
        let mut doc = Document::with_version("1.7");
        let id = make_flate_image(&mut doc, 200, 200, 1);

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.recompressed, 1);

        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let filter = stream.dict.get(b"Filter").unwrap();
        assert!(matches!(filter, Object::Name(n) if n == b"DCTDecode"));
    }

    #[test]
    fn skip_smask_image() {
        let mut doc = Document::with_version("1.7");
        let id = make_flate_image(&mut doc, 200, 200, 3);

        // Add an SMask reference
        let smask_id = doc.add_object(dictionary! { "Type" => "XObject" });
        let stream = doc.get_object_mut(id).unwrap().as_stream_mut().unwrap();
        stream.dict.set("SMask", Object::Reference(smask_id));

        let params = RecompressParams::default();
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn skip_below_threshold() {
        let mut doc = Document::with_version("1.7");
        // 10x10 image — very small compressed stream
        let id = make_flate_image(&mut doc, 10, 10, 3);

        let params = RecompressParams {
            quality: 85,
            min_size: 50_000,
        };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn skip_already_dctdecode() {
        let mut doc = Document::with_version("1.7");
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 100_i64,
            "Height" => 100_i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
            "Length" => 1000_i64,
        };
        let id = doc.add_object(Stream::new(dict, vec![0u8; 1000]));

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn skip_cmyk_colorspace() {
        let mut doc = Document::with_version("1.7");

        let pixel_count = 100 * 100 * 4;
        let pixels = vec![128u8; pixel_count];
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&pixels).unwrap();
        let compressed = encoder.finish().unwrap();

        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 100_i64,
            "Height" => 100_i64,
            "ColorSpace" => "DeviceCMYK",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "Length" => compressed.len() as i64,
        };
        let id = doc.add_object(Stream::new(dict, compressed));

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn skip_image_mask() {
        let mut doc = Document::with_version("1.7");
        let id = make_flate_image(&mut doc, 200, 200, 1);

        let stream = doc.get_object_mut(id).unwrap().as_stream_mut().unwrap();
        stream.dict.set("ImageMask", Object::Boolean(true));

        let params = RecompressParams::default();
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn skip_non_image_xobject() {
        let mut doc = Document::with_version("1.7");

        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let content = vec![0u8; 60_000];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&content).unwrap();
        let compressed = encoder.finish().unwrap();

        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "Filter" => "FlateDecode",
            "Length" => compressed.len() as i64,
        };
        let id = doc.add_object(Stream::new(dict, compressed));

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn skip_when_jpeg_larger() {
        let mut doc = Document::with_version("1.7");
        // Use constant-value pixels — flate compresses to ~20 bytes, JPEG can't beat that
        let pixels = vec![128u8; 200 * 200 * 3];
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&pixels).unwrap();
        let compressed = encoder.finish().unwrap();

        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 200_i64,
            "Height" => 200_i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "Length" => compressed.len() as i64,
        };
        let id = doc.add_object(Stream::new(dict, compressed));

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        // Constant-value FlateDecode is ~139 bytes; JPEG can't beat that
        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.recompressed, 0);
    }

    #[test]
    fn empty_object_ids_is_noop() {
        let mut doc = Document::with_version("1.7");
        let params = RecompressParams::default();
        let stats = recompress_images(&mut doc, &[], &params).unwrap();
        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }

    /// Create a FlateDecode image with ICCBased colorspace (N components).
    fn make_iccbased_flate_image(doc: &mut Document, width: u32, height: u32, n: u8) -> ObjectId {
        let pixel_count = (width * height) as usize * n as usize;
        let mut pixels = Vec::with_capacity(pixel_count);
        let mut val: u32 = 99;
        for _ in 0..pixel_count {
            val = val.wrapping_mul(1103515245).wrapping_add(12345);
            pixels.push((val >> 16) as u8);
        }

        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&pixels).unwrap();
        let compressed = encoder.finish().unwrap();

        let alternate = if n == 3 { "DeviceRGB" } else { "DeviceGray" };

        // Create ICC profile stream (minimal — just needs /N and /Alternate)
        let icc_dict = dictionary! {
            "N" => n as i64,
            "Alternate" => alternate,
            "Length" => 0_i64,
        };
        let icc_id = doc.add_object(Stream::new(icc_dict, vec![]));

        // ColorSpace array: [/ICCBased <icc-stream-ref>]
        let cs_array = doc.add_object(Object::Array(vec![
            Object::Name(b"ICCBased".to_vec()),
            Object::Reference(icc_id),
        ]));

        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => Object::Reference(cs_array),
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "Length" => compressed.len() as i64,
        };
        doc.add_object(Stream::new(dict, compressed))
    }

    #[test]
    fn recompress_iccbased_rgb_image() {
        let mut doc = Document::with_version("1.7");
        let id = make_iccbased_flate_image(&mut doc, 200, 200, 3);

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.recompressed, 1);

        // Verify filter changed to DCTDecode and colorspace to DeviceRGB
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let filter = stream.dict.get(b"Filter").unwrap();
        assert!(matches!(filter, Object::Name(n) if n == b"DCTDecode"));
        let cs = stream.dict.get(b"ColorSpace").unwrap();
        assert!(matches!(cs, Object::Name(n) if n == b"DeviceRGB"));
    }

    #[test]
    fn recompress_iccbased_gray_image() {
        let mut doc = Document::with_version("1.7");
        let id = make_iccbased_flate_image(&mut doc, 200, 200, 1);

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.recompressed, 1);

        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        let cs = stream.dict.get(b"ColorSpace").unwrap();
        assert!(matches!(cs, Object::Name(n) if n == b"DeviceGray"));
    }

    #[test]
    fn skip_iccbased_cmyk_image() {
        let mut doc = Document::with_version("1.7");
        let id = make_iccbased_flate_image(&mut doc, 200, 200, 4);

        let params = RecompressParams { quality: 85, min_size: 0 };
        let stats = recompress_images(&mut doc, &[id], &params).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.recompressed, 0);
    }
}

