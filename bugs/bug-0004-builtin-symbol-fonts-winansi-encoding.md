# Bug Report: built-in @Symbol/@ZapfDingbats forced to WinAnsiEncoding — renders a blank page

**Severity:** Medium (silent blank output for two advertised built-in fonts)
**Component:** `medpdf` — `src/pdf_watermark.rs:233-246` (`add_known_named_font`)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced with poppler rendering plus a control PDF (fonts subagent).

## Description

`add_known_named_font` unconditionally writes `"Encoding" => "WinAnsiEncoding"` into the font dictionary.  The README advertises `@Symbol` and `@ZapfDingbats` among the built-in fonts, but WinAnsiEncoding overrides a symbolic font’s built-in encoding with Latin glyph names (`a`, `b`, …) that do not exist in Symbol or ZapfDingbats — every character maps to nothing.

Reproduced: `add_text_params("abc", FontData::BuiltIn("Symbol"), "Symbol")` produces `BaseFont=/Symbol Encoding=/WinAnsiEncoding`; poppler renders a **completely blank page**.  A control PDF identical except with `/Encoding` omitted renders αβχ.

The embedded-font path already contains the correct logic — `determine_pdf_encoding` (`src/font_helpers.rs:188-194`) omits `/Encoding` for symbolic fonts — but the built-in path never consults it.

## Reproduction (test-ready)

1. Build a one-page document.
2. `add_text_params` with `FontData::BuiltIn("Symbol".into())`, text `"abc"`.
3. Assert the created font dictionary has **no** `/Encoding` key (currently it has `/Encoding /WinAnsiEncoding`).
4. (Visual, optional) rasterize and assert nonzero dark pixels.

## Suggested fix

In `add_known_named_font`, omit the `/Encoding` entry when `font_name` is `Symbol` or `ZapfDingbats` (the two symbolic Standard-14 fonts).

## Why the fix addresses the bug

PDF Annex D specifies that WinAnsiEncoding applies to nonsymbolic fonts only; symbolic Standard-14 fonts use their built-in encodings.  Omitting `/Encoding` restores those, matching both the spec and the crate’s own embedded-font logic.

## Note on text semantics after the fix

With built-in encoding restored, the bytes in the string select symbol glyphs by Symbol’s own code assignments (e.g. byte `0x61` = α).  A caller typing `"αβχ"` still gets `UnrepresentableText` (α is not CP1252), which is correct fail-loud behavior — but the docs should state that `@Symbol`/`@ZapfDingbats` text is interpreted as byte codes in the font’s built-in encoding.  Add one sentence to the README font section when fixing.
