# Changelog

All notable changes to the `medpdf` crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0/).

## [Unreleased]

## [0.11.2] - 2026-07-21
### Fixed
- bug-0018: overlaying or placing a page re-encoded the destination page’s
  content stream(s), which corrupts or drops any inline image (`BI…EI`) the
  page contains — a `lopdf` 0.42 decode→encode defect (see
  `LOPDF_INLINE_IMAGE_BUG.md`).  Destination content is now isolated with
  standalone `q`/`Q` wrapper streams instead of being re-encoded, so it is
  never touched.  Source content, which must still be re-encoded to rename
  its resources, now loudly rejects inline images instead of silently
  mangling them, and its resource renaming is operator-aware.  The
  `count_q_balance` helper (previously duplicated in `pdf_watermark`) is now
  shared from `pdf_helpers`.

## [0.11.1] - 2026-07-21
### Fixed
- bug-0007: a reference cycle not passing through `/Parent` (an annotation’s
  `/P` page back-reference, a self-linking `/Dest`) made `deep_copy_object_by_id`
  recurse forever and overflow the stack — an uncatchable `SIGABRT`, not an
  `Err`.  The destination object ID is now reserved and the source→dest mapping
  recorded _before_ recursing, so such a cycle resolves to a plain
  back-reference instead.  Acyclic output is unchanged.

## [0.11.0] - 2026-07-15
### Added
- Unicode text via Type0/CIDFontType2 composite fonts, so embedded text can
  carry the full range of Unicode (Hawaiian ‘okina and kahakō, and beyond),
  not just the Standard-14 encodings.

## [0.10.3] - 2026-06-29
### Changed
- Bump `lopdf` 0.39 → 0.42 and `rand` 0.9 → 0.10 (toolchain-wide coordinated
  bump; improves AES/encryption interop).

## [0.10.2] - 2026-06-29
### Fixed
- Overlay content streams were written with a stale `/Length`, producing
  corrupt overlays in some readers.

## [0.10.1] - 2026-03-21
### Fixed
- `parse_page_spec` now preserves the user-specified page order.
- Code-review hardening: fallible byte parsing, let-chains, clippy cleanup.

## [0.10.0] - 2026-03-15
### Added
- `place_page()` for positioned and scaled page placement — the primitive
  behind downstream N-up and booklet imposition.

## [0.9.2] - 2026-02-18
### Fixed
- Font subsetting for Adobe Acrobat / Foxit compatibility (0.9.2), building on
  the OS/2-table and Windows cmap subtable fixes in 0.9.1 and the initial
  allsorts-based subsetting of embedded watermark fonts in 0.9.0.

Earlier history (0.8.x and before: PDF encryption, edition-2024 migration, the
initial image-embedding split) is in the git log.

[Unreleased]: https://github.com/ComposerChrisF/medpdf/compare/medpdf-v0.11.1...HEAD
