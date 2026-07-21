# Bug Report: image recompression ignores `/Mask`, breaking color-key transparency

**Severity:** High — silent visual corruption (transparency lost or new holes punched) reported as a successful optimization
**Component:** `medpdf-image` — `src/recompress.rs:138-141` (`extract_image_info` checks only `/SMask`)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced live (image subagent): an image with `/Mask [250 255 250 255 250 255]` was recompressed (`scanned=1 recompressed=1`), `/Mask` retained, filter now DCTDecode.

## Description

`extract_image_info` skips images with `/SMask` (soft masks) but not images with `/Mask`.  Color-key masking (`/Mask` as an array) declares exact sample ranges that render transparent.  JPEG is lossy: after recompression, pixels that were inside the key range drift out (transparency lost) and pixels near it drift in (new transparent holes).  The `/Mask` entry stays in the dictionary over now-shifted sample data — silent corruption, and the stats report a successful recompression.

A `/Mask` that is a **reference** (stencil-mask image) would technically survive lossy recompression of the base image, but skipping both forms is safe and symmetric with the SMask policy.

## Reproduction (test-ready)

1. Build a FlateDecode RGB image XObject with `/Mask [250 255 250 255 250 255]` and pixel data containing values inside and near that range.
2. Run `recompress_images` with `min_size: 0`.
3. Assert the object was **skipped** (filter still FlateDecode) — fails today (it is recompressed).

## Suggested fix

In `extract_image_info`, next to the SMask check:

```rust
if stream.dict.get(b"Mask").is_ok() { return None; }
```

## Why the fix addresses the bug

Identical rationale to the existing SMask skip: recompression must not alter anything transparency semantics depend on.  Skipping loses only an optimization opportunity; recompressing loses correctness.
