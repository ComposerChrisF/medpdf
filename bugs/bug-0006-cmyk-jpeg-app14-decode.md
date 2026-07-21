# Bug Report: CMYK JPEG passthrough lacks Adobe APP14 handling (`/Decode` inversion)

**Severity:** Low-Medium (inverted colors for Adobe-produced CMYK JPEGs in some viewers/print RIPs)
**Component:** `medpdf-image` — `src/lib.rs:544-559` (4-component JPEG passthrough)
**Category:** CODE BUG — **status SUSPECTED: verify with a real fixture before implementing.**
**Verified:** 2026-07-16 deep review — code path traced (APP14 never inspected, no `/Decode` emitted); the rendering consequence is per-spec reasoning, not yet reproduced, because a genuine Adobe CMYK JPEG fixture could not be constructed with the `image` crate.

## Description

A 4-component JPEG is embedded via passthrough as `/DeviceCMYK` with no `/Decode` array, and the Adobe APP14 marker is never inspected.  Photoshop/Adobe CMYK JPEGs store inverted CMYK samples (signaled by APP14).  Viewers disagree about compensating inside DCTDecode: Acrobat and mupdf detect APP14 and invert; spec-strict decoders do not.  The standard generator practice (e.g. `img2pdf`) is to emit `/Decode [1 0 1 0 1 0 1 0]` when the APP14 “Adobe” marker is present on a 4-component JPEG.  As written, such a JPEG can render inverted.

## Required verification before fixing

Obtain a real Adobe-produced CMYK JPEG (Photoshop “Save As JPEG” from a CMYK document).  Embed it with `add_image`, then compare rendering in at least Acrobat, mupdf/poppler, and Preview against the source.  Only implement the fix if the inversion reproduces; record which viewers show it in this report first (per the bug lifecycle, amendments are committed before the fix deletes the report).

## Reproduction sketch (once fixture exists)

1. `add_image` with the CMYK JPEG (passthrough path — no alpha, DPI under `max_dpi`).
2. Assert the XObject dict for a 4-component JPEG **with APP14** contains `/Decode [1 0 1 0 1 0 1 0]` (currently absent).

## Suggested fix

While scanning JPEG markers (`parse_jpeg_sof` or a sibling pass), detect the APP14 “Adobe” marker; when present on a 4-component JPEG, set `/Decode [1 0 1 0 1 0 1 0]` on the image XObject dictionary.

## Why the fix addresses the bug

The `/Decode` array is the PDF-level mechanism to declare the sample inversion, and keying it to APP14 matches how the files were produced — viewers that already special-case APP14 are unaffected (they apply their own inversion inside the decoder and ignore double handling is avoided because `/Decode` is authoritative), while strict decoders start rendering correctly.  This is the established behavior of mature PDF generators.
