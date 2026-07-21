# Bug Report: recompression silently and totally corrupts images using TIFF Predictor 2

**Severity:** Critical for affected inputs — total, unrecoverable image corruption reported as success
**Component:** `medpdf-image` — `src/recompress.rs:131-137` (`/Filter` checked, `/DecodeParms` never consulted) and `:170` (`decompressed_content()`); root interaction with lopdf 0.42 `src/object.rs:933-936` (`decompress_predictor` un-applies PNG predictors 10-15 only).
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced live with measured error (image subagent, scratch `bughunt/main.rs`): recompressing a Predictor-2 image yields mean absolute pixel error **85.3** (the theoretical value for unrelated uniform byte streams) versus **9.3** for correctly recompressing the same pixels at q85.

## Description

`extract_image_info` checks `/Filter /FlateDecode` but never reads `/DecodeParms`.  lopdf’s `decompressed_content()` un-applies PNG predictors (10-15) but returns **TIFF Predictor 2** data unchanged — still horizontally differenced.  Predictor 2 adds no per-row tag bytes, so the length sanity check (`recompress.rs:67-70`) passes exactly; the differenced bytes are then JPEG-encoded as if they were pixels.  Worse, `set_plain_content` (lopdf `object.rs:763-768`) removes `/DecodeParms` from the rewritten object, so no viewer can ever undo the differencing — the corruption is permanent, and the run reports `recompressed=1` success.

## Reproduction (test-ready)

1. Build an image XObject: `/Filter /FlateDecode`, `/DecodeParms << /Predictor 2 /Colors 3 /Columns 200 /BitsPerComponent 8 >>`, content = zlib(horizontally-differenced 200×200×3 pixel data).
2. Run `recompress_images` with `min_size: 0` — today: `scanned=1 recompressed=1`.
3. Decode the resulting JPEG and compare with the true (un-differenced) pixels; assert mean absolute error < 15 — fails today (≈ 85).
4. After the fix: assert the image is skipped (filter unchanged).

## Suggested fix

In `extract_image_info`, read `/DecodeParms` (resolving an indirect reference); if present with a `Predictor` other than 1 or 10-15, `return None`.  If `/DecodeParms` exists but cannot be resolved to a dictionary, also `return None` (unknown means skip, never assume raw pixels — the positive-evidence principle).

## Why the fix addresses the bug

PNG predictors (10-15) remain safely recompressible because lopdf actually decodes them and strips the parms; Predictor 2 is the case lopdf hands back undecoded, and the check converts that silent corruption into a conservative skip.  Data that cannot be proven to be raw pixels is never re-encoded.
