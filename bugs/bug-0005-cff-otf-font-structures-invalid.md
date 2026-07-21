# Bug Report: CFF/OTF fonts embedded with structurally invalid PDF font objects (both paths)

**Severity:** Medium-High (embedded OTF fonts rejected by conforming viewers; substitute font rendered)
**Component:** `medpdf` — `src/font_helpers.rs:236-253` (`classify_font`), `src/pdf_watermark.rs:750-758` (`add_descriptor_and_fontfile`), `:763-799` (simple path), `:805-857` (composite path)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced twice independently: orchestrator probe with `/Library/Fonts/Academico-Regular.otf` (CFF=true, glyf=false) and fonts subagent with `BellMTStd-Regular.otf` including poppler diagnostics.

## Description

For a CFF-flavored face, `classify_font` returns `("FontFile3", "Type1C")` and that pair is misused in three ways:

1. **Font dict `/Subtype /Type1C` (simple path).**  `Type1C` is a legal `/Subtype` for the FontFile3 _stream_, never for a Font dictionary — Font dict subtypes are `Type1`, `TrueType`, `Type3`, `Type0`, `MMType1` (PDF 32000-1 Table 110).  Probe output: `Font obj (7, 0): /Subtype /Type1C`.
2. **FontFile3 stream has no `/Subtype` and a meaningless `Length1`.**  FontFile3 requires `/Subtype` in its stream dict (`Type1C`, `CIDFontType0C`, or `OpenType`).  The bytes embedded are the whole sfnt-wrapped OTF file, so the correct value is `/Subtype /OpenType` (PDF 1.6+).  Probe output: `FontFile3 -> stream Subtype=None Length1=Some(59136)`.
3. **Composite path pairs `CIDFontType2` with CFF.**  `CIDFontType2` requires a TrueType (`glyf`) font program via FontFile2 with `CIDToGIDMap`; a CFF font needs `CIDFontType0` with `FontFile3 /Subtype /OpenType` (or `CIDFontType0C`).  Probe output: descendant `/Subtype /CIDFontType2` alongside `FontFile3`.

Poppler on such output prints `Syntax Error: Unknown font type` and `Mismatch between font type and embedded font file`, then renders with a substitute font — the embedded font is silently ignored.  TrueType (`glyf`) fonts are unaffected.

## Reproduction (test-ready)

1. Read any CFF-outline `.otf` (assert `Face::raw_face().table(b"CFF ")` is `Some` and no `glyf`).
2. `add_text_params` with ASCII text (simple path); assert the font dict `/Subtype` is `Type1` and the `FontFile3` stream dict contains `/Subtype /OpenType` — both fail today.
3. `add_text_params` with text containing U+0101 (composite path); assert the descendant font `/Subtype` is **not** `CIDFontType2` when the face is CFF — fails today.

## Suggested fix

In `add_descriptor_and_fontfile` and the two embedding paths, branch on the face flavor:

- **Simple path, CFF:** Font dict `/Subtype /Type1`; FontFile3 stream `/Subtype /OpenType`; drop `Length1` (it is defined for FontFile/FontFile2, not FontFile3).
- **Composite path, CFF:** either implement `CIDFontType0` + `FontFile3 /Subtype /OpenType` (correct long-term), or fail loudly with a clear “CFF composite fonts unsupported” error.  Fail-loud is acceptable for now — `feature-plan-type0-subsetting.md` already scopes composite work to TrueType.
- Keep `classify_font`’s FontFile2/TrueType behavior unchanged for `glyf` fonts.

## Why the fix addresses the bug

It makes the emitted objects match PDF 32000-1 Tables 110/127: viewers recognize the font dict subtype, find the required stream subtype, and load the embedded program instead of substituting.  `/Subtype /OpenType` avoids having to unwrap bare CFF data from the sfnt container.  Where full support is deferred (composite CFF), a loud error replaces silent substitute-font output.
