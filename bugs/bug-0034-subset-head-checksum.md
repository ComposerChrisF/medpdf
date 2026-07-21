# Bug Report: subsetted font’s table-directory checksum for `head` is computed over a nonzero `checkSumAdjustment`

**Severity:** Low — spec-invalid checksum; most consumers ignore table checksums, but font sanitizers can reject the subset font
**Component:** `medpdf` — `src/pdf_subset.rs:348-373` (`rebuild_ttf`: directory checksum at `:348-353`, adjustment rewritten afterwards at `:362-373`)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — confirmed by trace (fonts subagent); the OpenType requirement is unambiguous.

## Description

OpenType requires the table-directory checksum for `head` to be computed with `checkSumAdjustment` (bytes 8-11 of the table) treated as zero.  `rebuild_ttf` (reached via `add_windows_cmap`) computes the directory checksum over the head table **including its old nonzero adjustment**, then rewrites the adjustment afterwards — leaving the directory entry wrong by exactly the old adjustment value.  Validators (e.g. the OTS sanitizer used by browsers/renderers that sanitize embedded fonts) can flag or reject the font.

## Reproduction (test-ready)

1. Run the subset path on any TrueType font (watermark with an embedded TTF, then `subset_fonts`).
2. Parse the output font: recompute the `head` directory checksum per spec (adjustment zeroed) and compare with the stored directory entry — mismatch today.

## Suggested fix

When computing the directory checksum for the `head` table in `rebuild_ttf`, checksum a copy of the table with bytes 8..12 zeroed (or compute normally and subtract the old adjustment value).

## Why the fix addresses the bug

It implements the spec’s definition of the `head` checksum, making the emitted font pass checksum validation regardless of the adjustment value written later.
