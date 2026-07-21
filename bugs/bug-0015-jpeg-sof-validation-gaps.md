# Bug Report: JPEG SOF validation gaps — lossless SOF3 accepted, precision ignored, zero dimensions produce `inf`

**Severity:** Low-Medium (silently unrenderable or invalid output for unusual JPEG inputs)
**Component:** `medpdf-image` — `src/lib.rs:167` (SOF marker match), `src/lib.rs:172-175` (dimensions read, precision byte ignored), `src/lib.rs:518-523` (`add_image` validates only the output box), `src/lib.rs:556` (`BitsPerComponent => 8` hardcoded), `src/lib.rs:494` (`compute_fit` divides by pixel dims).
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — verified by code trace (image subagent); consequences follow directly from the DCTDecode spec and the arithmetic.  No live fixture was run (rare inputs).

## Description

Three related validation gaps in the JPEG passthrough parser:

1. **SOF3 (lossless JPEG) accepted.**  `matches!(marker, 0xC0..=0xC3)` includes `0xC3`.  PDF’s DCTDecode supports baseline/extended/progressive DCT only — a lossless JPEG passes the parser and is embedded verbatim, producing a silently unrenderable image.
2. **Precision byte never read.**  A 12-bit JPEG (SOF precision 12) is embedded with hardcoded `BitsPerComponent 8` — dictionary/data mismatch.
3. **Zero dimensions accepted.**  A crafted JPEG whose SOF claims width 0 passes; `compute_fit` divides by it, producing `inf`, which is written as the literal token `inf` in the content stream plus a `/Width 0` XObject — invalid PDF.

## Reproduction (test-ready)

1. Craft minimal JPEG headers (SOF3 marker; SOF0 with precision 12; SOF0 with width 0) — a few dozen bytes each, no full image needed since only the header is parsed.  Store fixtures in `bugs/bug-0015/`.
2. Call `add_image` with each; assert a loud `Err` in all three cases.  Today: case 1 and 2 succeed silently; case 3 produces `inf` tokens (observable by decoding the content stream).

## Suggested fix

In `parse_jpeg_sof` (and `add_image`’s validation):

- Restrict the accepted SOF markers to `0xC0..=0xC2`.
- Read the precision byte; return a clear error when it is not 8 (the `image`-crate fallback cannot decode these either, so a loud error is the honest outcome).
- Reject pixel width/height of 0.

## Why the fix addresses the bug

Each gap currently converts an unusual-but-possible input into silent wrong output; the parser already reads the SOF header, so the checks are one comparison each and turn all three into immediate, diagnosable errors.
