# Bug Report: simple-path `/Widths` and all FontDescriptor metrics are raw font units, not the required 1000/em glyph space

**Severity:** High — visibly wrong text layout (≈2× letter spacing) for any embedded font whose unitsPerEm ≠ 1000 (Arial, Verdana, and nearly every macOS TrueType is 2048)
**Component:** `medpdf` — `src/font_helpers.rs:125-150` (`get_font_widths`, unscaled advances), `src/font_helpers.rs:308-341` (`get_pdf_info_of_face` — ascent/descent/leading/x-height/cap-height/bbox raw), emitted at `src/pdf_watermark.rs:769-776` and `:737-749`
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced and rendered (fonts subagent): Arial (upem 2048) yields `Widths['A'] = 1366` (correct: 667) and `Ascent = 1854` (correct: 905); poppler renders “DRAFT Widths Test” at 36 pt with roughly double letter spacing, running off the page.

## Description

PDF 32000-1 §9.2.4/§9.6.2.1/§9.8.1 define simple-font `/Widths` and all FontDescriptor metrics in **glyph space**, where 1000 units = 1 text-space unit.  The code emits raw `unitsPerEm`-scale values from ttf-parser.  Conforming viewers are required to prefer `/Widths` over the font program when they conflict, so every glyph is advanced upem/1000 times too far.

The composite path already does this correctly — `build_w_array` (`src/pdf_font_composite.rs:69-82`) scales by `1000.0 / upem` — but it shares the same **unscaled** FontDescriptor via `add_descriptor_and_fontfile`, so composite output has correct advances but wrong descriptor metrics.

Why the suite never caught it: the fonts used in tests (SourceSansPro, CrimsonText class) are upem-1000, where raw equals scaled; the embedded-font visual test compares the same mis-widthed document before/after subsetting; golden watermark tests use built-in Helvetica, which emits no `/Widths`.

## Reproduction (test-ready)

1. Embed a upem-2048 font (`/System/Library/Fonts/Supplemental/Arial.ttf`).
2. `add_text_params("A", …)` on a test page.
3. Read back the TrueType font dict and its descriptor: assert `Widths[('A' as usize) - first_char] == 667` and `Ascent == 905` (±1 for rounding).  Currently 1366 and 1854.

## Suggested fix

In `get_font_widths` and `get_pdf_info_of_face`, scale every emitted metric by `1000.0 / face.units_per_em() as f32` and round — exactly the formula `build_w_array` already uses.  Fields cannot overflow post-scale (scaling shrinks values for upem > 1000; for upem < 1000 the magnitudes stay in i16/u16 range for real fonts, but widen the struct fields to i32 if in doubt).  Add a regression test with a 2048-upem fixture.

## Why the fix addresses the bug

It emits the unit system the spec defines and makes the simple and composite paths consistent — the composite `/W` array is the in-repo proof of the correct formula.  With `/Widths` matching the font program’s effective advances, text extraction, selection, and rendering all align.
