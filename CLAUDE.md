# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build --release            # Build optimized binaries
cargo build --release -p pdf-merger  # Build CLI only
cargo check --workspace          # Fast type checking
cargo test --workspace           # Run all tests
```

Never commit changes to git without permission from the user.

## Workspace Structure

This is a Cargo workspace with two crates:

```
pdf_merger/                    # Repository root (workspace)
├── Cargo.toml                 # Workspace manifest
├── medpdf/                    # Library crate (medium-level PDF API)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # Public API and re-exports
│       ├── error.rs           # PdfMergeError with Display trait
│       ├── parsing.rs         # Page spec parsing with nom
│       ├── pdf_helpers.rs     # Deep copy, PDF key constants, Unit enum
│       ├── pdf_font.rs        # Font discovery and caching
│       ├── font_helpers.rs    # TTF parsing, font metrics
│       ├── pdf_copy_page.rs   # Page copying between documents
│       ├── pdf_delete_page.rs # Page deletion from documents
│       ├── pdf_blank_page.rs  # Blank page creation
│       ├── pdf_overlay.rs     # Page overlay with resource renaming
│       └── pdf_watermark.rs   # Text watermark rendering
└── pdf-merger/                # CLI crate
    ├── Cargo.toml
    └── src/
        ├── main.rs            # CLI args (clap), orchestrates pipeline
        └── spec_types.rs      # CLI spec types with FromStr (WatermarkSpec, etc.)
```

## Architecture Overview

**medpdf** is a reusable library providing medium-level PDF operations over lopdf.
**pdf-merger** is a CLI tool that uses medpdf for merging, overlaying, and watermarking PDFs.

### 5-Phase Processing Pipeline (pdf-merger/src/main.rs)

1. **Merge Pages** - Parse input file/page specs, load documents, copy selected pages
2. **Apply Overlays** - Overlay content from other PDFs with resource renaming
3. **Apply Watermarks** - Add text watermarks with embedded/system fonts
4. **Padding** - Pad document to multiple of N pages
5. **Save** - Compress and write output

### Module Responsibilities

| Crate/Module | Purpose |
|--------------|---------|
| `medpdf::error` | Custom `PdfMergeError` enum with Display/Error traits |
| `medpdf::parsing` | Page spec parsing with nom (`"1-3,5,7-"`, `"all"`) |
| `medpdf::pdf_helpers` | Deep object copying, PDF key constants, Unit enum, page rotation |
| `medpdf::pdf_font` | Font discovery (system/file) and caching; re-exports `find_font`, `FontCache`, `FontPath` |
| `medpdf::font_helpers` | TTF parsing, font metrics, PDF FontDescriptor generation |
| `medpdf::pdf_copy_page` | `copy_page()` - copy pages between documents |
| `medpdf::pdf_delete_page` | `delete_page()` - remove pages from documents |
| `medpdf::pdf_blank_page` | `create_blank_page()` - add empty pages |
| `medpdf::pdf_overlay` | `overlay_page()` - merge content with resource renaming |
| `medpdf::pdf_watermark` | `add_text_params()` - text watermark rendering with color, alignment, rotation, alpha |
| `pdf_merger::main` | CLI args (clap), orchestrates pipeline (`pdf-merger` crate) |
| `pdf_merger::spec_types` | CLI spec types with FromStr for clap integration (`pdf-merger` crate) |

### Key Patterns

**Resource Renaming**: When overlaying pages, resources get suffixed to prevent conflicts. `find_unique_name()` generates non-conflicting identifiers, and content streams are updated to reference renamed resources.

**Deep Copy with Reference Tracking**: `deep_copy_object()` recursively clones PDF objects using a `BTreeMap<ObjectId, ObjectId>` to maintain reference integrity and skip Parent references.

**Font Discovery Pipeline**: Numeric handle → built-in (@Helvetica, @Courier, etc.) → system search via font-kit → direct file path

**Spec Parsing**: `WatermarkSpec`, `OverlaySpec`, `PadToSpec`, `PadFileSpec` all implement `FromStr` for clap integration.

### CLI Usage

```bash
pdf-merger -o out.pdf in1.pdf "1-3" in2.pdf "all" \
  --watermark "text=DRAFT,font=@Helvetica,size=24,x=1,y=1,units=in,color=#FF0000,alpha=0.5,rotation=45,h_align=center,pages=all" \
  --overlay "file=overlay.pdf,src_page=1,target_pages=1-5" \
  --pad-to 4
```

### PDF Key Constants

`medpdf::pdf_helpers` defines byte-string constants for PDF dictionary keys (KEY_PAGES, KEY_RESOURCES, KEY_FONT, etc.) to prevent typos and enable type-safe key usage.

### Known Limitations

**Overlay resource key deduplication is shallow.** `accumulate_dictionary_keys()` in `medpdf::pdf_overlay` only scans the root `/Pages` node for existing resource names when generating unique keys for overlay resources. It does not recurse into child `/Pages` nodes or individual `/Page` nodes. If a destination page defines its own `/Resources` with keys that differ from the root, an overlay could introduce a name collision. In practice this is rare — most PDF generators place shared resources on the root `/Pages` node — but it could occur with documents that have per-page resource dictionaries. A proper fix requires recursing the full page tree to collect all resource keys.
