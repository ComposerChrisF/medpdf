# Changelog

All notable changes to the `medpdf` crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0/).

## [Unreleased]

## [0.11.6] - 2026-07-22
### Fixed
- bug-0025: `place_page` appended its placement (open `q`+`cm`, source
  content, close `Q`) directly after the destination page’s own content
  without neutralizing it, so a destination page whose content leaked
  graphics state — a top-level `cm` with no matching `q`/`Q`, common in
  scanned pages — had that leaked state displace the placed page.  The
  destination content is now bracketed with standalone `q`/`Q` wrapper
  streams (`isolate_dest_content_streams`, bug-0018’s mechanism, which
  never re-encodes the destination streams) before the placement is
  appended, matching how `overlay_page` and the watermark path already
  isolate destination state.

## [0.11.5] - 2026-07-22
### Fixed
- bug-0017: `/Resources` is inheritable — real-world documents put it on a
  `/Pages` ancestor rather than on the leaf page dict (PDF 32000-1 §7.7.3.4).
  `overlay_page` and `place_page` used to read only the page’s own
  `/Resources` entry, so a page relying on an inherited dict was treated as
  having no resources at all.  Both operations now resolve the effective
  `/Resources` by walking the `/Parent` chain.  A destination page with no
  resources of its own gets the inherited dict materialized onto it — as a
  private copy, so a shared ancestor sub-dict is never mutated in place — and
  a source page’s effective resources are resolved the same way before
  overlay/place proceeds.

### Added
- `pdf_helpers::get_page_resources` — shared helper that resolves a page’s
  effective `/Resources` by walking the `/Parent` chain; also intended for
  reuse by the bug-0008 inheritance fix.

## [0.11.4] - 2026-07-22
### Fixed
- bug-0030: a resource-type sub-dict (`/Font 10 0 R`) held as an indirect
  reference — routine output from Acrobat — was invisible to the collision
  scan, left un-renamed by the rename pass, and either dropped or errored by
  the merge, which only understood the inline form.  Deep-copied source
  `/Resources` are now normalized to inline sub-dicts right after the copy
  (`normalize_resource_subdicts`); `add_resource_keys` dereferences a
  reference so its keys enter the collision scan; and the merge’s read/write
  split dereferences an indirect **destination** sub-dict and merges into its
  target instead of erroring on it.  Affects both `overlay_page` and
  `place_page`.

## [0.11.3] - 2026-07-21
### Fixed
- bug-0019: per PDF 32000-1 §7.8.2, a page’s content is the concatenation of
  its `/Contents` fragments, and a split may fall at any token boundary — not
  just between operations.  `rename_source_content_streams` used to decode
  each source fragment independently, so an operation straddling a fragment
  boundary was silently dropped instead of rendered.  Fragments are now
  newline-joined and decoded once as a single concatenated stream, renamed,
  and re-emitted as one combined stream (reusing the first fragment’s object
  and removing the rest).  The destination-side facet of this class of bug
  was already fixed by bug-0018.

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
