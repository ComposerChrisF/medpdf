# Bug Report: simple path never fails loudly on a missing glyph, contradicting its own docs and CLAUDE.md

**Severity:** Medium — silent character loss where three pieces of documentation promise a loud error
**Component:** `medpdf` — `src/pdf_watermark.rs:554-578` (`encode_text_for_font`, `Simple` arm)
**Category:** BOTH — the docs (function doc `pdf_watermark.rs:550-553`, `error.rs:13-18`, CLAUDE.md’s “missing glyphs fail loudly with `MedpdfError::UnrepresentableText`”) state the intended contract; the composite arm implements it; the simple arm does not.  Fix the code to match the documented contract.
**Verified:** 2026-07-16 deep review — reproduced (fonts subagent): Skia.ttf lacks a glyph for U+00AD (soft hyphen — WinAnsi-representable, so it stays on the simple path); `add_text_params("X\u{AD}Y", …, lossy_text = false)` returns `Ok`, emits byte `0xAD` with `Widths[0xAD − 32] == 0`, and the character silently disappears from output.

## Description

`encode_text_for_font`’s `Simple` arm is `Ok((utf8_to_winansi(&params.text), Literal))` with no glyph-presence check against the embedded face.  A character that is inside CP1252 but absent from the font sails through: zero-width, no glyph, silent.  The identical situation on the composite path (`encode_text_identity`) correctly returns `UnrepresentableText` (or substitutes `.notdef` with a warning under `lossy_text`).

## Reproduction (test-ready)

1. Find/bundle a face missing some CP1252 glyph (Skia.ttf misses U+00AD; alternatively subset a fixture font to guarantee a gap).
2. `add_text_params` with that character, embedded font, `lossy_text = false`.
3. Assert `Err(MedpdfError::UnrepresentableText { .. })` — fails today (`Ok`).
4. With `lossy_text = true`: assert success plus `?` substitution.

## Suggested fix

In the `Simple` arm, when the font is embedded: parse the face (it is already parsed nearby for metrics; reuse), check `face.glyph_index(ch)` per character; collect missing chars and return `UnrepresentableText` unless `lossy_text`, in which case substitute `?` and `log::warn!` — mirroring `encode_text_identity`’s contract exactly.  Built-in fonts (no face to query) keep today’s behavior.

## Why the fix addresses the bug

It implements the precise behavior the function doc, the error-type doc, and CLAUDE.md all promise, using the composite arm as the template — the two encoding paths then share one fail-loud contract.

## Related edge notes (same code area, fix opportunistically)

- **Lossy composite `.notdef` width skew:** lossy-mode missing glyphs emit GID 0, which gets no `/W` entry, so viewers advance `DW = 1000` (a full em) while `measure_text_width_with_face` counts 0 — alignment skews per substituted char.  Emitting a `/W` entry for GID 0 (advance of the face’s glyph 0) fixes both.
- **Control characters:** `char_in_winansi` accepts 0x00-0x1F/0x7F, so a `\t` or `\n` in text passes the simple path as an undefined-encoding byte (silently invisible), while the composite path throws a confusing `UnrepresentableText` for whitespace.  Decide one behavior (suggest: reject control chars loudly in both paths, naming the character).
