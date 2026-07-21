# Bug Report: alignment fallback width counts UTF-8 bytes, not characters

**Severity:** Low — mis-centered/mis-aligned text and wrong underline length for non-ASCII text with built-in fonts
**Component:** `medpdf` — `src/pdf_watermark.rs:284` (`compute_text_metrics` fallback arm)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced (fonts subagent): concrete `Td` operand comparison.

## Description

`compute_text_metrics` estimates text width for non-embedded fonts as `params.text.len() as f32 * params.font_size * 0.6` — `len()` is the **byte** length.  The sibling function `font_helpers::measure_text_width` (`font_helpers.rs:278-282`) deliberately uses `text.chars().count()` with a comment saying exactly why (“Count characters, not bytes, so multibyte text is not over-measured”).  The fallback arm predates that fix and was missed.

Confirmed: `"Café"` (4 chars, 5 bytes) at 12 pt, built-in Helvetica, `HAlign::Center` at x=100 emits `Td [82, 100]` (bytes-based dx = −18) instead of dx = −14.4 (chars-based).  Every non-ASCII character skews centering/right-alignment and underline/strikeout length by `0.6 × font_size` per extra byte.

## Reproduction (test-ready)

1. `add_text_params("Café", FontData::BuiltIn("Helvetica"), …)` with `h_align(HAlign::Center)`, `font_size(12.0)`, `position(100.0, 100.0)`.
2. Decode the page content stream, find the `Td` operation, assert its x operand is `100.0 − (4.0 × 12.0 × 0.6) / 2.0 = 85.6`.  Currently 82.0.

## Suggested fix

Change `params.text.len()` to `params.text.chars().count()` at `pdf_watermark.rs:284`.

## Why the fix addresses the bug

It makes the fallback consistent with `measure_text_width`’s explicit, commented intent: the 0.6-em heuristic is per character, and characters — not encoding bytes — occupy horizontal space.
