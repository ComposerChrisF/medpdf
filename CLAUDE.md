# medpdf — Project Instructions

## Build Commands

```bash
cargo build --release            # Build optimized binaries
cargo check --workspace          # Fast type checking
cargo test --workspace           # Run all tests
```

The `visual_*` tests go through `pdf-test-visual`, which shells out to `pdftoppm` (poppler, `brew install poppler`) or `mutool`; without one they fail with `RasterizerNotFound` — a missing tool, not a regression.

Publishing to crates.io is publish-only through cargo-release steps; `PUBLISHING.md` is the script and `scripts/publish-status.py` says when.  Versions are owned by verset / the `/commit-*` skills, never by cargo-release.

## Workspace Structure

A Cargo workspace of three crates: `medpdf/` (the library, published), `medpdf-image/` (the image companion, published; `svg` is an optional feature), and `pdf-test-visual/` (`publish = false`, the visual-regression harness).  The per-module map is § Module Responsibilities below — the single copy; `ls medpdf/src` is current.

## Architecture Overview

**medpdf** is a reusable library providing medium-level PDF operations over lopdf.  Consumers include [pdf-maker](https://github.com/ComposerChrisF/pdf-maker) and pdf-orchestrator (separate repos, both path-deps on this checkout).

### Module Responsibilities

| Crate/Module | Purpose |
|--------------|---------|
| `medpdf::error` | Custom `MedpdfError` enum with Display/Error traits |
| `medpdf::types` | Builder-pattern param types: `AddTextParams`, `DrawRectParams`, `DrawLineParams`, `PlacePageParams`, `PdfColor`, alignment enums |
| `medpdf::font_data` | `FontData` enum: `Hack(u8)`, `BuiltIn(String)`, `Embedded(Arc<Vec<u8>>)` |
| `medpdf::parsing` | Page spec parsing with nom (`"1-3,5,7-"`, `"all"`) |
| `medpdf::pdf_helpers` | Deep object copying, PDF key constants, Unit enum, page rotation, `get_page_effective_size()` (MediaBox extents swapped under `/Rotate` 90/270) |
| `medpdf::pdf_font` | Font discovery (system/file) and caching; re-exports `find_font`, `FontCache`, `FontPath` |
| `medpdf::font_helpers` | TTF parsing, font metrics, PDF FontDescriptor generation, canonical WinAnsi encoding table |
| `medpdf::pdf_copy_page` | `copy_page()` - copy pages between documents |
| `medpdf::pdf_delete_page` | `delete_page()` - remove pages from documents |
| `medpdf::pdf_encryption` | `encrypt_document()` - AES-256/AES-128/RC4-128 encryption with permission controls |
| `medpdf::pdf_blank_page` | `create_blank_page()` - add empty pages |
| `medpdf::pdf_overlay` | `overlay_page()` - merge content with resource renaming |
| `medpdf::pdf_overlay_helpers` | Shared helpers for overlay/place-page: resource key collection, renaming, content stream normalization |
| `medpdf::pdf_place_page` | `place_page()` - place a source page by its **visible box** at a given position, scale, and rotation (arbitrary angle; the source's `/Rotate` is honored and the MediaBox origin compensated out) with optional clipping (default: enabled); `placed_page_size()` reports the footprint a placement will occupy, from the same transform |
| `medpdf::pdf_watermark` | `add_text_params()` - text watermark rendering with color, alignment, rotation, alpha; `EmbeddedFontCache` for deduplicating embedded font objects across pages; picks the WinAnsi simple-font fast path or the Type0 composite path per call |
| `medpdf::pdf_font_composite` | Type0/CIDFontType2 composite-font pieces (Identity-H GID encoding, `/W` widths, ToUnicode CMap) for text with characters outside WinAnsiEncoding |
| `medpdf::pdf_subset` | Post-watermark font subsetting via allsorts: shrinks embedded fonts to used glyphs, tagging `/BaseFont` and `/FontName`; `subset_fonts()` |
| `medpdf_image` | Image embedding companion crate (JPEG, PNG, etc.) |
| `medpdf_image::recompress` | `recompress_images()` - re-encodes qualifying FlateDecode image XObjects as DCTDecode (JPEG) to shrink Word-style bloated PDFs |
| `medpdf_image::svg` | SVG embedding via svg2pdf (optional `svg` feature): converts SVG to a Form XObject; `add_svg()`, `load_svg()` |

### Key Patterns

**Resource Renaming**: When overlaying or placing pages, resources get suffixed to prevent conflicts (`_o` for overlay, `_p` for place-page). `find_unique_name()` generates non-conflicting identifiers, and content streams are updated to reference renamed resources.

**Deep Copy with Reference Tracking**: `deep_copy_object()` recursively clones PDF objects using a `BTreeMap<ObjectId, ObjectId>` to maintain reference integrity and skip Parent references.

**Font Discovery Pipeline**: Numeric handle → built-in (@Helvetica, @Courier, etc.) → system search via font-kit → direct file path

**Embedded Font Caching**: Two-level caching prevents redundant work. `FontCache` caches font file reads as `Arc<Vec<u8>>` (keyed by path). `EmbeddedFontCache` caches the resulting PDF font objects (keyed by `(Arc` pointer identity`, EncodingKind)`), so the same embedded font is only added to the document once per encoding even when applied to many pages.  Embedded font streams are compressed (deflate) before insertion.

**Unicode Text (WinAnsi vs Type0)**: `add_text_params()` chooses per call.  Text representable in WinAnsiEncoding (CP1252) uses a single-byte simple Type1/TrueType font (the fast path, subsettable via `subset_fonts`).  Text with any character outside CP1252 — Hawaiian ‘okina/kahakō, etc. — with an embedded font switches to a Type0/CIDFontType2 composite font (Identity-H, full font embedded, `/W` widths + ToUnicode CMap for extraction).  The `/W` and ToUnicode are refreshed after each composite draw so the font stays valid without any finalize pass.  Built-in Standard-14 fonts and missing glyphs fail loudly with `MedpdfError::UnrepresentableText` (unless `AddTextParams::lossy_text` restores `?`/`.notdef` substitution).  One face may be embedded twice (simple + composite) when a page mixes both.

### PDF Key Constants

`medpdf::pdf_helpers` defines byte-string constants for PDF dictionary keys to prevent typos and enable type-safe key usage; the public ones are `KEY_RESOURCES`, `KEY_CONTENTS`, `KEY_EXTGSTATE`, `KEY_XOBJECT`, the rest `pub(crate)`.

### Known Limitations

- **Type0 composite fonts are not subsetted.** The full font file is embedded whenever Unicode text needs the composite path, so a Unicode watermark enlarges the PDF by the full font size.  Subsetting composite fonts (a GID-remapping `CIDToGIDMap` pass that leaves content-stream GIDs untouched) is a planned follow-up — `plans/plan-0004-type0-subsetting.md`.  The WinAnsi simple-font path is still subsetted normally by `subset_fonts`.
- **No complex-script shaping.** The composite path emits one glyph per Unicode scalar via the cmap (no ligatures, combining-mark composition, or bidi).  Precomposed forms (e.g. kahakō `ā` = U+0101) render correctly; decomposed sequences do not compose.
