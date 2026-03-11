# Font Subsetting for medpdf

## Context

Currently, when medpdf embeds a font for watermarks, it includes the **entire font file** (all glyphs, all tables). A typical font like DejaVu Sans is ~700KB; a watermark saying "DRAFT" only uses 5 glyphs (~20-30KB subsetted). Font subsetting removes unused glyphs and tables, dramatically reducing output PDF size.

## Scope Assessment

This is a **medium-large feature** touching the font embedding pipeline. The recommended approach keeps the existing WinAnsiEncoding architecture intact and treats subsetting as an optimization pass run after all watermarks are applied but before saving.

## Recommended Approach: `allsorts` crate + post-watermark subsetting pass

### Why `allsorts` over `subsetter`

| Crate | Preserves cmap? | Impact on existing code |
|-------|----------------|------------------------|
| `subsetter` (by typst) | No (removes cmap) | Requires rewriting font embedding to CIDFont/Type0 composite fonts, changing text encoding from WinAnsi bytes to 2-byte glyph ID hex strings. Massive rewrite. |
| `allsorts` (by YesLogic) | Yes (`CmapTarget::Unicode`) | Font stream swap only. Existing font dictionaries, content streams, and WinAnsiEncoding all stay the same. |

The `allsorts` approach is dramatically simpler because the PDF viewer still uses `WinAnsiEncoding → Unicode → cmap → glyph ID`, and allsorts preserves and updates the cmap with correct glyph ID mappings after renumbering.

### How It Works

1. **During watermark application**: Track which Unicode characters are used per embedded font
2. **After all watermarks**: For each embedded font, resolve used chars → glyph IDs, call `allsorts::subset::subset()`, replace the font stream in the document
3. **Content streams are never modified** — only the font file binary data changes

## Implementation Steps

### 1. Add `allsorts` dependency
**File**: `medpdf/Cargo.toml`
```toml
allsorts = "0.16"  # font subsetting
```

### 2. Extend `EmbeddedFontCache` to track character usage
**File**: `medpdf/src/pdf_watermark.rs`

Add per-font tracking of used characters and the font stream ObjectId:
```rust
struct CachedFont {
    font_id: ObjectId,           // Font dictionary ObjectId
    font_key: String,            // Resource key (e.g., "F55")
    font_stream_id: ObjectId,    // FontFile2/FontFile stream ObjectId (for replacement)
    data: Arc<Vec<u8>>,          // Original font bytes
    used_chars: HashSet<char>,   // Characters used across all pages
}
```

Add a method to record character usage from watermark text, and a method to iterate cached fonts for subsetting.

### 3. Record character usage during watermark rendering
**File**: `medpdf/src/pdf_watermark.rs` — `add_text_params()`

After rendering text to a page, record which characters were used:
```rust
font_cache.record_usage(&font_data_arc, &params.text);
```

### 4. Create `pdf_subset.rs` module with `subset_fonts()` function
**File**: `medpdf/src/pdf_subset.rs` (new)

Core logic:
```rust
pub fn subset_fonts(doc: &mut Document, font_cache: &EmbeddedFontCache) -> Result<()> {
    for cached in font_cache.embedded_entries() {
        // 1. Parse original font, map used_chars → glyph IDs
        // 2. Call allsorts::subset::subset(&provider, &glyph_ids, &Minimal, Unicode)
        // 3. Compress subsetted bytes
        // 4. Replace font stream object in doc (doc.objects[font_stream_id])
        // 5. Update Length1 in stream dict
        // 6. Prefix BaseFont with subset tag (e.g., "ABCDEF+FontName")
    }
    Ok(())
}
```

Key details:
- Generate a random 6-letter uppercase tag per font (e.g., `XHWQTL+DejaVuSans`) per PDF spec for subsetted fonts
- Update BaseFont in both the Font dictionary and FontDescriptor
- The Widths array stays unchanged (it's indexed by WinAnsi byte value, not glyph ID)
- Font dictionary structure (Type, Subtype, Encoding, FirstChar, LastChar) stays unchanged

### 5. Wire into the pipeline
**File**: `medpdf/src/lib.rs` — add `pub mod pdf_subset;` and re-export `subset_fonts`

**File**: `pdf-merger/src/main.rs`
- Return `EmbeddedFontCache` from `apply_drawing_commands()`
- Call `medpdf::subset_fonts(&mut doc, &font_object_cache)?;` between drawing commands and save
- Add a `--no-subset` CLI flag (opt-out, subsetting on by default)

### 6. Update `add_embedded_font()` to store font stream ObjectId
**File**: `medpdf/src/pdf_watermark.rs`

Currently `add_embedded_font()` creates `font_file_id` but doesn't expose it. The cache needs this ID so `subset_fonts()` can replace the stream object without traversing the Font → FontDescriptor → FontFile chain.

## Files to Modify

| File | Change |
|------|--------|
| `medpdf/Cargo.toml` | Add `allsorts` dependency |
| `medpdf/src/pdf_watermark.rs` | Extend `EmbeddedFontCache` with char tracking + font_stream_id; record usage in `add_text_params()` |
| `medpdf/src/pdf_subset.rs` | **New file** — `subset_fonts()` implementation |
| `medpdf/src/lib.rs` | Add `pub mod pdf_subset;` and re-export |
| `pdf-merger/src/main.rs` | Return cache from drawing commands; call `subset_fonts()` before save; add `--no-subset` flag |

## Verification

1. Build: `cargo build --workspace`
2. Create a test PDF with an embedded font watermark:
   ```bash
   cargo run -p pdf-merger -- -o test_subset.pdf input.pdf all \
     --watermark "text=DRAFT,font=DejaVuSans,size=72,x=300,y=400"
   ```
3. Compare file sizes with and without `--no-subset`
4. Open the subsetted PDF in multiple viewers (Preview, Chrome, Adobe) to verify text renders correctly
5. Run `cargo test --workspace`

## Edge Cases to Handle

- **Font used on 0 pages** (registered but never referenced in content) — skip subsetting
- **Built-in fonts** (`@Helvetica` etc.) — skip (not embedded)
- **Hack fonts** (numeric handles) — skip (not embedded)
- **allsorts parse failure** — fall back to keeping the full font (log warning, don't error)
- **Single-glyph subset** — still need .notdef (glyph 0), allsorts handles this
