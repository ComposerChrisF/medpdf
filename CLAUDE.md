# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build --release            # Build optimized binaries
cargo check --workspace          # Fast type checking
cargo test --workspace           # Run all tests
```

Never commit changes to git without permission from the user.

## Workspace Structure

This is a Cargo workspace with three crates:

```
medpdf/                        # Repository root (workspace)
├── Cargo.toml                 # Workspace manifest
├── medpdf/                    # Library crate (medium-level PDF API)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # Public API and re-exports
│       ├── error.rs           # MedpdfError with Display trait
│       ├── types.rs           # Builder-pattern param types (AddTextParams, PdfColor, etc.)
│       ├── font_data.rs       # FontData enum (Hack/BuiltIn/Embedded)
│       ├── parsing.rs         # Page spec parsing with nom
│       ├── pdf_helpers.rs     # Deep copy, PDF key constants, Unit enum
│       ├── pdf_font.rs        # Font discovery and caching
│       ├── font_helpers.rs    # TTF parsing, font metrics, WinAnsi encoding
│       ├── pdf_copy_page.rs   # Page copying between documents
│       ├── pdf_delete_page.rs # Page deletion from documents
│       ├── pdf_blank_page.rs  # Blank page creation
│       ├── pdf_encryption.rs  # Document encryption (AES-256/AES-128)
│       ├── pdf_overlay.rs     # Page overlay with resource renaming
│       └── pdf_watermark.rs   # Text watermark rendering
├── medpdf-image/              # Image embedding companion crate
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs             # JPEG/PNG/etc. image embedding into PDF pages
└── pdf-test-visual/           # Visual regression test utility (publish=false)
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

## Architecture Overview

**medpdf** is a reusable library providing medium-level PDF operations over lopdf. Consumers include [pdf-merger](https://github.com/ComposerChrisF/pdf-merger) (separate repo).

### Module Responsibilities

| Crate/Module | Purpose |
|--------------|---------|
| `medpdf::error` | Custom `MedpdfError` enum with Display/Error traits |
| `medpdf::types` | Builder-pattern param types: `AddTextParams`, `DrawRectParams`, `DrawLineParams`, `PdfColor`, alignment enums |
| `medpdf::font_data` | `FontData` enum: `Hack(u8)`, `BuiltIn(String)`, `Embedded(Arc<Vec<u8>>)` |
| `medpdf::parsing` | Page spec parsing with nom (`"1-3,5,7-"`, `"all"`) |
| `medpdf::pdf_helpers` | Deep object copying, PDF key constants, Unit enum, page rotation |
| `medpdf::pdf_font` | Font discovery (system/file) and caching; re-exports `find_font`, `FontCache`, `FontPath` |
| `medpdf::font_helpers` | TTF parsing, font metrics, PDF FontDescriptor generation, canonical WinAnsi encoding table |
| `medpdf::pdf_copy_page` | `copy_page()` - copy pages between documents |
| `medpdf::pdf_delete_page` | `delete_page()` - remove pages from documents |
| `medpdf::pdf_encryption` | `encrypt_document()` - AES-256/AES-128 encryption with permission controls |
| `medpdf::pdf_blank_page` | `create_blank_page()` - add empty pages |
| `medpdf::pdf_overlay` | `overlay_page()` - merge content with resource renaming |
| `medpdf::pdf_watermark` | `add_text_params()` - text watermark rendering with color, alignment, rotation, alpha; `EmbeddedFontCache` for deduplicating embedded font objects across pages |
| `medpdf_image` | Image embedding companion crate (JPEG, PNG, etc.) |

### Key Patterns

**Resource Renaming**: When overlaying pages, resources get suffixed to prevent conflicts. `find_unique_name()` generates non-conflicting identifiers, and content streams are updated to reference renamed resources.

**Deep Copy with Reference Tracking**: `deep_copy_object()` recursively clones PDF objects using a `BTreeMap<ObjectId, ObjectId>` to maintain reference integrity and skip Parent references.

**Font Discovery Pipeline**: Numeric handle → built-in (@Helvetica, @Courier, etc.) → system search via font-kit → direct file path

**Embedded Font Caching**: Two-level caching prevents redundant work. `FontCache` caches font file reads as `Arc<Vec<u8>>` (keyed by path). `EmbeddedFontCache` caches the resulting PDF font objects (keyed by `Arc` pointer identity), so the same embedded font is only added to the document once even when applied to many pages. Embedded font streams are compressed (deflate) before insertion.

### PDF Key Constants

`medpdf::pdf_helpers` defines byte-string constants for PDF dictionary keys (KEY_PAGES, KEY_RESOURCES, KEY_FONT, etc.) to prevent typos and enable type-safe key usage.

### Known Limitations

(None at this time.)
