# bug-0046: `recompress_images` re-encodes an image’s soft mask (or stencil `/Mask`) as lossy JPEG

**Severity:** High when triggered — silent corruption of transparency, reported as a successful optimization.  Any transparent image whose mask stream is among the ids passed in (pdf-orchestrator’s `<ImportPdf recompressImages>` passes every copied object, so this happens today).

**Type:** Code bug in this repo (`medpdf-image/src/recompress.rs`).

**Found:** 18-Nov-2026, reported by the pdf-orchestrator session (request `row:App/Rust/Pdf/medpdf/r0006`), seen by its reviewer in a scratch test.

## The Defect

`extract_image_info` skips an image that **carries** `/SMask` or `/Mask` (bug-0028), but nothing skips the **mask stream itself**.  A soft mask is a `/Subtype /Image`, `/ColorSpace /DeviceGray`, 8-bit, FlateDecode stream, so it passes every check and is re-encoded as DCTDecode.  The parent keeps its Flate data; its alpha channel becomes JPEG artefacts.  A stream used as an explicit (stencil) `/Mask` is normally `/ImageMask true` and 1-bit, so already skipped, but nothing in the code guarantees that.

## Reproduction

An RGB FlateDecode image whose `/SMask` is a Flate DeviceGray image, both ids passed to `recompress_images` with `quality=30, min_size=1`:

```
(7, 0) cs=/DeviceRGB  filter=/FlateDecode smask=8 0 R   ← parent: skipped, as intended
(8, 0) cs=/DeviceGray filter=/DCTDecode   smask=None    ← its soft mask: JPEG-encoded
```

## Fix

Before the loop, collect the id of every stream referenced as `/SMask` or (as a reference) `/Mask` by any image in the document, and skip those ids.  The scan is document-wide rather than limited to the caller’s ids: whether a stream is a mask is a property of the document, and a caller that passes the mask without its parent (a parent shared from an earlier copy) must not get it re-encoded either.

Pin: a test with the reproduction above asserting the mask’s `/Filter` stays FlateDecode, and that it fails with the skip reverted.
