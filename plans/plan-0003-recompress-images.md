# Plan: Recompress Images as JPEG

## Problem

Some PDF authoring tools (notably Microsoft Word on Mac) re-encode JPEG images as lossless FlateDecode (PNG-style) when exporting to PDF, even when the source image was a JPEG. This inflates file sizes significantly — a 34 KB JPEG photo becomes a 233 KB lossless stream despite being downsampled to fewer pixels.

When a single-page PDF like this is merged into many documents (e.g., a perusal score cover merged into 50+ piece PDFs), the bloat compounds: ~100 KB × 50 = 5 MB of unnecessary data, all served to website visitors downloading scores.

### Not All Images Are Photos

In music publishing, PDFs often contain a mix of photographic images (cover art, composer headshots) and graphic design elements (logos, ornamental borders, notation fragments). JPEG is lossy and introduces artifacts in sharp edges, text, and flat-color graphics — these must remain lossless (FlateDecode/PNG-equivalent). The caller must be able to decide which images get recompressed and which don't.

## Proposed CLI

```bash
# Recompress all eligible lossless images as JPEG (default quality 85)
pdf-maker -o out.pdf in.pdf "all" --recompress-images jpeg

# Specify JPEG quality (1-100)
pdf-maker -o out.pdf in.pdf "all" --recompress-images jpeg --recompress-quality 80

# Only recompress images above a size threshold
pdf-maker -o out.pdf in.pdf "all" --recompress-images jpeg --recompress-min-size 50000

# Combine with other operations
pdf-maker -o out.pdf cover.pdf "1" score.pdf "all" --recompress-images jpeg
```

### Behavior

- Scans all image XObjects in the output document after merge
- Identifies images using lossless compression (FlateDecode) with RGB or grayscale color spaces
- Re-encodes them as DCTDecode (JPEG) at the specified quality
- Preserves image dimensions, color space, and placement — only the stream encoding changes
- Skips images that are already JPEG-encoded (DCTDecode)
- Skips images with alpha channels (JPEG doesn't support transparency)
- Skips 1-bit and indexed color images (JPEG is inappropriate for these)
- Reports what it did: "Recompressed 1 image: obj 12 (355×403, 232823 → ~35000 bytes)"

### Page Targeting

The `--recompress-images` flag applies to all pages in the output document (after merge). No per-page targeting needed — this is a document-wide optimization pass.

## API Design

### Primary API: Per-Image (medpdf-image)

The core function operates on a single image object, giving the caller full control over which images to recompress:

```rust
/// Metadata about a PDF image XObject, extracted from its dictionary.
pub struct PdfImageInfo {
    pub object_id: ObjectId,
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    pub filter: ImageFilter,         // FlateDecode, DCTDecode, etc.
    pub color_space: ImageColorSpace, // RGB, Gray, CMYK, Indexed, etc.
    pub has_alpha: bool,              // true if /SMask is present
    pub stream_size: usize,          // compressed stream size in bytes
}

/// Inspect all image XObjects in a document.
pub fn find_images(doc: &Document) -> Vec<PdfImageInfo>;

/// Recompress a single image XObject in-place from FlateDecode to DCTDecode.
/// Returns the old and new stream sizes, or an error if the image is not
/// eligible (already JPEG, has alpha, 1-bit, indexed, CMYK, etc.).
pub fn recompress_image_to_jpeg(
    doc: &mut Document,
    object_id: ObjectId,
    quality: u8,
) -> Result<RecompressResult, MedpdfError>;

pub struct RecompressResult {
    pub object_id: ObjectId,
    pub width: u32,
    pub height: u32,
    pub old_size: usize,
    pub new_size: usize,
}
```

This lets callers:
- List images, inspect their properties, and decide which to recompress
- Apply different quality settings per image
- Filter by size threshold, page, color space, or any other criteria
- Report results per image

### Convenience API: Document-Wide (medpdf-image)

A helper that iterates all images and recompresses eligible ones, for callers that want simple all-or-nothing behavior:

```rust
pub struct RecompressOptions {
    pub quality: u8,           // default 85
    pub min_size: Option<usize>, // skip images with streams below this size
}

/// Recompress all eligible lossless images in the document to JPEG.
/// Returns a list of results for each image that was recompressed.
pub fn recompress_all_images(
    doc: &mut Document,
    options: &RecompressOptions,
) -> Result<Vec<RecompressResult>, MedpdfError>;
```

pdf-maker's `--recompress-images` flag calls this convenience wrapper. pdf-orchestrator can use either API depending on the level of control needed.

## Implementation Notes

### New Phase in the Pipeline

This would be a new phase between the current Phase 4 (Padding) and Phase 5 (Save):

1. Merge Pages
2. Apply Overlays
3. Apply Watermarks
4. Padding
5. **Recompress Images** ← new
6. Save

### Key Steps

1. Walk all page resource dictionaries, find `/XObject` entries with `/Subtype /Image`
2. For each image, check `/Filter` — skip if already `/DCTDecode`
3. Check `/BitsPerComponent` (skip if not 8), check for `/SMask` (skip if present — has alpha)
4. Decode the image stream (inflate FlateDecode data) to raw pixel bytes
5. Encode raw pixels as JPEG using a Rust JPEG encoder (e.g., `image` crate or `jpeg-encoder`)
6. Replace the stream data and update `/Filter` to `/DCTDecode`, remove `/DecodeParms` if present
7. Handle ICC color profiles: the JPEG can embed the ICC profile, or the `/ColorSpace` reference can stay as-is (PDF viewers apply it regardless of stream encoding)

### Crate Placement

The image inspection and recompression logic belongs in `medpdf-image`, which already handles image embedding. The per-image function is the primary API; the document-wide function is a convenience wrapper in the same crate.

### Dependencies

- `jpeg-encoder` or the `image` crate (may already be in `medpdf-image`'s dependency tree for image embedding support)
- `flate2` for FlateDecode decompression (likely already a dependency via lopdf)

## Why Not Python

- This is a reusable PDF optimization that fits naturally in the merge pipeline
- It needs to work at the PDF object level (decoding streams, rewriting dictionaries), which medpdf already does
- One-off Python scripts for PDF manipulation are fragile and hard to maintain
- The feature compounds in value: any PDF passing through pdf-maker can benefit

## Testing

- Round-trip: take a PDF with a FlateDecode image, recompress, verify the image renders identically in a viewer
- Size comparison: verify the output is meaningfully smaller
- Skip cases: JPEG passthrough, 1-bit images, images with alpha, indexed color
- Quality parameter: verify different quality values produce different sizes
- Selective recompression: use `find_images()` + `recompress_image_to_jpeg()` to recompress only specific images, verify others are untouched
- Integration: combine with merge, watermark, overlay in a single command
