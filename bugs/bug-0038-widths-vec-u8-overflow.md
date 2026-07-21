# Bug Report: `get_font_widths` u8 arithmetic overflows when a symbolic font spans codes 0..=255

**Severity:** Low — latent panic (debug) / index-out-of-bounds panic (release) on a specific font shape
**Component:** `medpdf` — `src/font_helpers.rs:130` (`vec![0; (last_char - first_char + 1) as usize]` with `u8` operands)
**Category:** CODE BUG — **status SUSPECTED: traced, not reproduced** (needs a face that maps both U+0000 and U+00FF while classified symbolic, so `compute_char_range` returns `(0, 255)`).
**Verified:** 2026-07-16 deep review — arithmetic confirmed by trace (fonts subagent): `255u8 - 0 + 1` overflows u8 — panic in debug builds; in release it wraps to 0, allocating an empty vec, and the subsequent `widths[(ch - first_char) as usize]` write panics out-of-bounds.

## Description

`compute_char_range` scans 0..=255 for symbolic fonts and can legitimately return `(0, 255)`.  `get_font_widths` then computes the vec length as `last_char - first_char + 1` in `u8`, which overflows for the full range.  Either build profile panics — a crash on a valid (if unusual) font, reachable through `add_text_params` with an embedded symbolic font covering code 0.

## Reproduction (test-ready)

Call `get_font_widths(&face, 0, 255)` directly with any face (the overflow is in the arithmetic, not the font): debug build panics on the subtraction expression; release panics on the first in-range write.  A unit test with `#[should_panic]` today, flipped to a success assertion (`widths.len() == 256`) after the fix.

## Suggested fix

Compute in `usize`: `let len = (last_char as usize) - (first_char as usize) + 1; let mut widths = vec![0u16; len];` and index with the same widened arithmetic.

## Why the fix addresses the bug

The count of an inclusive u8 range is up to 256, which does not fit in u8; performing the size arithmetic in `usize` removes the overflow for every possible `(first, last)` pair, including the full range.
