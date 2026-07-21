# Bug Report: “JPEG pass-through without re-encoding” is false above `max_dpi`; re-encode uses hardcoded quality 75

**Severity:** Medium — silent generation loss at below-recompress-default quality, contradicting the documented contract
**Component:** `medpdf-image` — `src/lib.rs:32` (enum doc), `src/lib.rs:353-374` (re-encode), `src/lib.rs:104` (default `max_dpi: 300`); `README.md:9`.
**Category:** BOTH — the docs omit the exception (spec side), and the re-encode quality is a hardcoded 75 with no control (code side).  Neither requires a design ruling.
**Verified:** 2026-07-16 deep review — path confirmed (image subagent).

## Description

The `ImageFormat::Jpeg` doc (“embedded as DCTDecode without re-encoding”) and the README’s pass-through claim hold only when the image’s effective DPI at the placed size is ≤ `max_dpi` (default 300).  Above that, the JPEG is decoded, resized, and re-encoded via `image::ImageFormat::Jpeg` — which uses the `image` crate’s default JPEG quality of **75**, below even this crate’s own `RecompressParams` default of 85, with no caller control.  The only signal is a `log::info!` about dimensions; nothing mentions a lossy re-encode.

## Reproduction (test-ready)

1. Take any JPEG whose pixel size makes its effective DPI exceed 300 at the destination box (e.g. 2000 px wide placed into a 2-inch box → 1000 DPI).
2. `add_image` with default params.
3. Assert the written XObject’s data differs from the input bytes (re-encoded) and observe the quality drop; with the fix, assert the encoder was invoked with quality 85 (or the configured value).

## Suggested fix

1. Code: use `image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality)` where `quality` is a new `DrawImageParams` field defaulting to **85** (matching `RecompressParams`).
2. Docs: state the `max_dpi` exception on the `ImageFormat::Jpeg` enum doc and in the README (“pass-through unless downsampling is required by `max_dpi`; downsampled JPEGs are re-encoded at `jpeg_quality`”).
3. Consider raising the log line to `log::warn!` or at least including “re-encoding (lossy)” in the message.

## Why the fix addresses the bug

The contract becomes true as written (pass-through, with a documented and controllable exception), and the silent quality floor rises to the crate’s own established default instead of an implementation accident of the `image` crate.
