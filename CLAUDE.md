# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build --release    # Build optimized binary
cargo build              # Build debug binary
cargo run -- [args]      # Run with arguments
cargo check              # Fast type checking without building
```

Never commit changes to git without permission from the user.

## Architecture Overview

pdf_merger is a CLI tool for merging, overlaying, and watermarking PDF files using the `lopdf` library.

### 5-Phase Processing Pipeline (main.rs)

1. **Merge Pages** - Parse input file/page specs, load documents, copy selected pages
2. **Apply Overlays** - Overlay content from other PDFs with resource renaming
3. **Apply Watermarks** - Add text watermarks with embedded/system fonts
4. **Padding** - Pad document to multiple of N pages
5. **Save** - Compress and write output

### Module Responsibilities

| Module | Purpose |
|--------|---------|
| `main.rs` | CLI args (clap), orchestrates pipeline |
| `pdf_overlay.rs` | Page overlay with content stream merging, resource conflict resolution |
| `pdf_watermark.rs` | Text watermark rendering with font embedding |
| `pdf_helpers.rs` | Deep object copying, PDF key constants |
| `font_helpers.rs` | TTF parsing, font metrics extraction, PDF FontDescriptor generation |
| `pdf_font.rs` | Font discovery (system/file) and caching |
| `parsing.rs` | Page spec parsing with nom (`"1-3,5,7-"`, `"all"`) |
| `error.rs` | Custom `PdfMergeError` enum with conversions |

### Key Patterns

**Resource Renaming**: When overlaying pages, resources get suffixed to prevent conflicts. `find_unique_name()` generates non-conflicting identifiers, and content streams are updated to reference renamed resources.

**Deep Copy with Reference Tracking**: `deep_copy_object()` recursively clones PDF objects using a `BTreeMap<ObjectId, ObjectId>` to maintain reference integrity and skip Parent references.

**Font Discovery Pipeline**: Numeric handle → built-in (@Helvetica, @Courier, etc.) → system search via font-kit → direct file path

**Spec Parsing**: `WatermarkSpec`, `OverlaySpec`, `PadToSpec`, `PadFileSpec` all implement `FromStr` for clap integration.

### CLI Usage

```bash
pdf_merger -o out.pdf in1.pdf "1-3" in2.pdf "all" \
  --watermark "text=DRAFT,font=@Helvetica,size=24,x=1,y=1,units=in,pages=all" \
  --overlay "file=overlay.pdf,src_page=1,target_pages=1-5" \
  --pad-to 4
```

### PDF Key Constants

`pdf_helpers.rs` defines byte-string constants for PDF dictionary keys (KEY_PAGES, KEY_RESOURCES, KEY_FONT, etc.) to prevent typos and enable type-safe key usage.
