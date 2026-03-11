# Feature Plan: Recompress Images as JPEG

## Problem

Some PDF authoring tools (notably Microsoft Word on Mac) re-encode JPEG images as lossless FlateDecode (PNG-style) when exporting to PDF, even when the source image was a JPEG. This inflates file sizes significantly — a 34 KB JPEG photo becomes a 233 KB lossless stream despite being downsampled to fewer pixels.

When a single-page PDF like this is merged into many documents (e.g., a perusal score cover merged into 50+ piece PDFs), the bloat compounds: ~100 KB × 50 = 5 MB of unnecessary data, all served to website visitors downloading scores.

## Proposed CLI

```bash
# Recompress all lossless images as JPEG (default quality 85)
pdf-merger -o out.pdf in.pdf "all" --recompress-images jpeg

# Specify JPEG quality (1-100)
pdf-merger -o out.pdf in.pdf "all" --recompress-images jpeg --recompress-quality 80

# Only recompress images above a size threshold
pdf-merger -o out.pdf in.pdf "all" --recompress-images jpeg --recompress-min-size 50000

# Combine with other operations
pdf-merger -o out.pdf cover.pdf "1" score.pdf "all" --recompress-images jpeg
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

The image decoding/encoding logic belongs in `medpdf-image`, which already handles image embedding. Add a new public function like `recompress_image_to_jpeg()` that takes raw stream bytes + image metadata and returns JPEG-encoded bytes.

### Dependencies

- `jpeg-encoder` or the `image` crate (may already be in `medpdf-image`'s dependency tree for image embedding support)
- `flate2` for FlateDecode decompression (likely already a dependency via lopdf)

## Why Not Python

- This is a reusable PDF optimization that fits naturally in the merge pipeline
- It needs to work at the PDF object level (decoding streams, rewriting dictionaries), which medpdf already does
- One-off Python scripts for PDF manipulation are fragile and hard to maintain
- The feature compounds in value: any PDF passing through pdf-merger can benefit

## Testing

- Round-trip: take a PDF with a FlateDecode image, recompress, verify the image renders identically in a viewer
- Size comparison: verify the output is meaningfully smaller
- Skip cases: JPEG passthrough, 1-bit images, images with alpha, indexed color
- Quality parameter: verify different quality values produce different sizes
- Integration: combine with merge, watermark, overlay in a single command
