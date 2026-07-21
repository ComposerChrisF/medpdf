# Bug Report: embedded symbol fonts get all-zero `/Widths` (glyph pile-up) or full misclassification

**Severity:** Medium — silent garbage output for embedded symbol fonts (Wingdings, Webdings class)
**Component:** `medpdf` — `src/font_helpers.rs:158-213` (`detect_is_symbolic`, `compute_char_range`, `get_font_widths`)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — probed with real system fonts (fonts subagent): Wingdings.ttf and Webdings.ttf.

## Description

All three helpers rely on `ttf_parser::Face::glyph_index`, which only consults Unicode cmap subtables — it skips the platform=3/encoding=0 **symbol** cmap that symbol fonts use (verified in ttf-parser 0.25.1, `Subtable::is_unicode`).  Symbol fonts map their glyphs at `0xF000 + code` in that subtable, so every `glyph_index(ch)` lookup for bytes 0-255 fails.  Two confirmed failure modes:

- **Wingdings.ttf** — correctly flagged symbolic (name heuristic), but the char-range scan finds no glyphs (falls back to `(32, 255)`), and `/Widths` comes out with **0 nonzero entries out of 224**.  Every glyph advances 0 pt: the text overprints in a pile at one x-position.
- **Webdings.ttf** — the name heuristic misses it (“webdings” contains neither “wingding” nor “dingbat”), and the coverage heuristic sees zero glyphs in 32..=127 so `has_some_glyphs` is false — classified **non-symbolic**, given `/Encoding /WinAnsiEncoding` and all-zero widths.  Silent success, garbage page.

## Reproduction (test-ready)

1. Load `/Library/Fonts/Wingdings.ttf` (or bundle a small symbol-font fixture in `bugs/bug-0010/`).
2. Call `font_helpers::get_pdf_font_info_of_data` (or go through `add_text_params` with `lossy_text(true)`).
3. Assert that `widths` contains nonzero entries for codes the font covers — fails today (all zero).
4. For Webdings: assert `encoding.is_none()` (symbolic) — fails today (`WinAnsiEncoding`).

## Suggested fix

1. In the glyph lookups used by `detect_is_symbolic`, `compute_char_range`, and `get_font_widths`: when the Unicode lookup fails, retry `0xF000 + code` (the documented OpenType/Microsoft symbol-font convention), or iterate the cmap subtables directly via `Face::tables().cmap` including non-Unicode ones.
2. In `detect_is_symbolic`, treat “no glyphs found at all in 32..=127 via Unicode cmap” as symbolic rather than requiring `has_some_glyphs`.

## Why the fix addresses the bug

The `0xF000` offset is how symbol fonts actually publish their glyphs; querying it makes the range scan, the widths, and the classification see the same glyphs a viewer’s font engine uses, so the emitted `/Widths` matches real advances and symbolic fonts stop being labeled WinAnsi.

## Note

The Symbol/ZapfDingbats **built-in** (non-embedded) variant of this problem is bug-0004.  The simple-path missing-glyph silence that lets these zero-width draws succeed without error is bug-0032.
