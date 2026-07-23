# Changelog

All notable changes to the `medpdf` crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0/).

## [Unreleased]

## [0.11.14] - 2026-07-23
### Fixed
- bug-0032: the WinAnsi (simple) font path never checked glyph presence, so
  a CP1252-representable character the embedded font lacked was emitted as
  a zero-width byte and vanished silently — contradicting the loud contract
  the composite path already honored.  `add_text_params` now verifies each
  printable character has a glyph (fail-loud `UnrepresentableText`, or `?` +
  warning under `lossy_text`); built-in fonts keep prior behavior.  Also:
  `build_w_array` always emits a `/W` entry for GID 0 so a lossy-substituted
  `.notdef` advances the font’s real width instead of `DW=1000`.  Control
  characters (`\n`/`\t`/…) are deferred (not rejected) but now warned about,
  since medpdf renders a single line; multi-line analysis in
  `feature-plan-multiline-watermark-text.md`.

## [0.11.13] - 2026-07-23
### Fixed
- bug-0005: CFF-flavored (`.otf`) fonts were embedded with structurally
  invalid PDF objects — a Font dict `/Subtype /Type1C` (illegal on a Font
  dict), a FontFile3 stream missing `/Subtype /OpenType` and carrying a
  meaningless `/Length1`, and the composite path pairing `CIDFontType2` with
  a CFF program — so conforming viewers rejected the embedded font program
  and silently substituted a fallback.  `classify_font` now returns a
  `FontClassification` (dict_subtype/font_file_key/stream_subtype/
  emits_length1/is_cff): CFF ⇒ Font-dict `/Type1` + FontFile3
  `/Subtype /OpenType`, no `/Length1`; TrueType (`glyf`) unchanged
  (`/TrueType` + FontFile2 + `/Length1`).  `add_descriptor_and_fontfile`
  emits the stream `/Subtype`/`/Length1` per flavor; the composite
  (Type0/CIDFontType2) path fails loud on a CFF face rather than emitting an
  invalid combination (CIDFontType0 not yet implemented).
  `tests/cff_otf_font_structure_regression.rs` added (CFF simple structure,
  composite fail-loud, TrueType-unchanged guard; the two CFF tests verified
  to fail on revert).

## [0.11.12] - 2026-07-23
### Fixed
- bug-0012: font-kit’s `font_index` (the selected face inside a `.ttc`/`.otc`
  font collection) was dropped on the path from font discovery to embedding,
  so a styled macOS system font backed by a collection silently embedded the
  _wrong_ face — face 0 — with `BaseFont=Unknown` and the whole collection
  blob as the font program.  Fixed via loud-fail guards rather than a public
  API change (an audit of pdf-maker and pdf-orchestrator found neither
  constructs or matches the font enums, so threading the index through the
  API would buy nothing): `handle_to_font_path` now returns `Result` and
  errors on a nonzero face index at resolution; `add_text_params` refuses a
  `.ttc`/`.otc` collection blob at embed time via
  `ttf_parser::fonts_in_collection`.  New regression test
  `tests/font_collection_rejected_regression.rs`, verified to fail on
  revert.

## [0.11.11] - 2026-07-22
### Fixed
- bug-0031: the simple-font path emitted `/Widths` and every FontDescriptor
  metric in the embedded font’s raw unitsPerEm, but PDF glyph space is
  1000 units/em — so any embedded font whose upem was not 1000 (Arial,
  Verdana, and most macOS TrueType faces use 2048) laid text out
  upem/1000× too wide.  `font_helpers` now scales every advance and
  descriptor metric by `1000/upem` via a new `glyph_space_scale` +
  `scale_metric`, the same formula the composite `/W` path already used;
  `FontDescriptorPdfInfo`’s metric fields widened `i16` → `i32` so a
  small-upem font cannot overflow when scaled up.  New regression test
  `tests/font_metrics_scaling_regression.rs`, verified to fail on revert.

## [0.11.10] - 2026-07-22
### Fixed
- bug-0037: the watermark path named its `/Font`/`/ExtGState` resource keys
  after the new object’s id (`F{id}`, `GS{id}`) and wrote them into the
  page’s resources with an unconditional `set`.  Since `F{objid}` is exactly
  the naming scheme medpdf itself emits, a page containing prior medpdf
  output could already bind that key to a different object — the write
  silently rebound the page’s existing text to the watermark font.
  `unique_resource_key` now checks the page’s effective (inherited-aware)
  `/Font`/`/ExtGState` sub-dictionary before registering: on a collision
  with a _different_ object it preserves the existing binding and derives
  a collision-free key (`F{id}_w`) via the same `find_unique_name`
  machinery `overlay_page` already uses; the no-collision case is
  byte-stable.

## [0.11.9] - 2026-07-22
### Fixed
- bug-0016: `overlay_page` errored with `Err(DictKey("Contents"))` when the
  overlay/source page had no `/Contents`.  `/Contents` is optional on a page
  (PDF 32000-1 §7.7.3.3) — a blank page legally omits it — and the sibling
  `place_page` already treated this as a no-op.  The `/Contents` lookup is
  now a match that returns `Ok(())` (debug-logged) on absence instead of
  propagating the error, mirroring `place_page`.

## [0.11.8] - 2026-07-22
### Fixed
- bug-0020: page-tree `/Count` maintenance was broken for documents with
  intermediate `/Pages` nodes (PDF 32000-1 §7.7.3.2 requires `/Count` on
  every `/Pages` node to equal the number of leaf pages beneath it).
  `delete_page` updated only the direct parent’s `/Count`, leaving every
  ancestor above it stale; `delete_page`, `copy_page`, and
  `create_blank_page` all assigned `/Count = kids.len()`, which counts
  children rather than leaves and is wrong under any intermediate node.
  A new `pdf_helpers::adjust_ancestor_counts` walks the `/Parent` chain
  applying ±1 (adding/removing one leaf changes every ancestor’s count by
  exactly one), cyclic-chain guarded; all three operations now use it.

## [0.11.7] - 2026-07-22
### Fixed
- bug-0008: `Resources`, `MediaBox`, `CropBox`, and `Rotate` are inheritable
  page attributes (PDF 32000-1 §7.7.3.4) and may live only on a source
  `/Pages` ancestor rather than the leaf page dict.  `copy_page`’s deep copy
  skips `/Parent`, so a copied page relying on any of these inherited
  values silently lost it — no size, no fonts, no rotation, under its new
  parent.  `copy_page_with_cache` now walks the source page’s `/Parent`
  chain and materializes each inherited attribute the copied page lacks
  onto the leaf page, deep-copying reference values through the shared
  `copied_objects` map.

### Added
- `pdf_helpers::resolve_inherited_attribute` — generalizes the bug-0017
  `/Resources` inheritance walk to any inheritable page attribute (key
  parameter); `get_page_resources` is now a thin convenience wrapper over
  it, and `copy_page` reuses it for `Resources`/`MediaBox`/`CropBox`/`Rotate`.

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
