# Bug Report: font-kit’s `font_index` is discarded — collection fonts (.ttc, macOS system fonts) embed the wrong face

**Severity:** High on macOS — styled system-font requests silently embed the wrong face (or an unusable blob) with `BaseFont = Unknown`
**Component:** `medpdf` — `src/pdf_font.rs:166-174` (`handle_to_font_path`), `FontPath`/`FontData` carrying no index, every `ttf_parser::Face::parse(data, 0)` call site (`font_helpers.rs:266,284`, `pdf_watermark.rs:279,564,866`, `pdf_subset.rs` via allsorts `table_provider(0)`).
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced live on this Mac (orchestrator probes `ttc_probe.rs`, `ttc_e2e.rs`).

## Description

font-kit handles carry a face index into a font collection: `Handle::Path { path, font_index }` and `Handle::Memory { bytes, font_index }`.  `handle_to_font_path` destructures both with `..` and **drops the index**.  Everything downstream then hardcodes face 0: metrics, widths, glyph lookups, and the embedded program bytes.

Live probe on this machine: `select_best_match("Helvetica", Weight::BOLD)` returns a **Memory** handle with `font_index = 1`.  medpdf resolves it to `FontPath::Memory(..)` with no index; `extract_font_name` parses face 0 and fails, so the display name falls back to `"EmbeddedFont"`.  End-to-end, `add_text_params` with that font succeeds and embeds a font dict with `BaseFont = Unknown` — face 0’s identity and metrics, not the requested bold face — and the embedded font program is the whole collection blob labeled as a plain font.  Requesting any styled variant of a macOS system font (nearly all live in `.ttc` collections) silently produces the wrong face, wrong widths, and a font program many viewers cannot use.

## Reproduction (test-ready, macOS)

```rust
let fp = medpdf::find_font_with_style(Path::new("Helvetica"),
        medpdf::FontWeight::BOLD, medpdf::FontStyle::Normal)?;
// Today: FontPath::Path/Memory with no index; face 0 is used everywhere.
// Assert (fails today): the resolved face's PostScript name contains "Bold".
```

Also assert after `add_text_params` that no embedded font dict has `BaseFont = Unknown`.

## Suggested fix

Thread the index through the type system:

1. `FontPath::Path(PathBuf)` → `FontPath::Path(PathBuf, u32)`; `FontPath::Memory(Arc<Vec<u8>>, String)` → add a `u32` index (or a small struct).  `handle_to_font_path` keeps `font_index` from both handle variants.
2. `FontData::Embedded(Arc<Vec<u8>>)` → carry the index too (e.g. `Embedded { data: Arc<Vec<u8>>, index: u32 }`), since every consumer parses from `FontData`.
3. Replace every `Face::parse(data, 0)` with the carried index; pass it to allsorts’ `table_provider(index)` in `pdf_subset.rs`.
4. When embedding, extract the single face’s tables rather than embedding the whole collection (ttf-parser exposes per-face table access; alternatively reject collections for embedding with a loud error until extraction is implemented — silent wrong-face output is the thing that must stop).
5. `EmbeddedFontCache` keying on `Arc::as_ptr` must incorporate the index, otherwise two faces from one collection alias one cache entry.

This is a public-API change (`FontPath`, `FontData` are exported) — semver-minor at least; coordinate with pdf-maker.

## Why the fix addresses the bug

The index is the identity of the face inside a collection; carrying it end-to-end makes the parsed metrics, the glyph lookups, the subsetter, and the embedded program all refer to the face font-kit actually selected — which is the face the caller asked for.
