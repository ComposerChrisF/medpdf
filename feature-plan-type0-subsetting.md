# Feature Plan: Type0/CID Composite-Font Subsetting

## Problem

The new composite-font path (`pdf_font_composite.rs`, currently uncommitted WIP alongside `lossy_text`) embeds the **full** font whenever watermark/AddText text contains any character outside WinAnsi (CP1252).  `subset_fonts()` in `pdf_subset.rs` explicitly skips composite entries:

```rust
// pdf_subset.rs, subset_fonts()
if entry.encoding == EncodingKind::Composite {
    continue;
}
```

Measured cost (2026-07-10, `F183-AiaIKaLa‘i` production pipeline): one kahakō vowel in watermark text switched SourceSansPro-Regular from the simple-path subset (19,960 bytes raw, 7,909 compressed) to a full-font composite embed (149,972 bytes raw, 68,857 compressed) — **+61KB per affected font, per output PDF**.  Every font face that picks up a single non-CP1252 character costs its full file size again.

Goal: composite fonts get subsetted down to used glyphs, matching the ~8KB footprint of the simple path, with **no change to consumers** — `pdf-orchestrator` and `pdf-maker` already call `medpdf::subset_fonts()` on every save, so they inherit the fix automatically.

## Current Architecture (what the implementer must know)

- **Draw time** (`pdf_watermark.rs`): text outside CP1252 routes through `add_embedded_font_composite()`, which embeds the full font bytes as `FontFile2`, builds a Type0 parent + CIDFontType2 descendant with `Encoding /Identity-H` and `CIDToGIDMap /Identity`, and encodes content-stream text as 2-byte big-endian **original-font GIDs** (`encode_text_identity()` in `pdf_font_composite.rs`; CID = GID).
- **After each draw**, `refresh_composite_maps()` rewrites the descendant’s `/W` array and the ToUnicode CMap from `entry.used_chars` — both keyed by **original GID**.
- **`CachedFontEntry`** (in `pdf_watermark.rs`) tracks everything a post-pass needs: `data` (full font bytes, `Arc<Vec<u8>>`), `used_chars: HashSet<char>`, `font_stream_id`, `descriptor_id`, `font_id` (Type0 parent), `cidfont_id: Option<ObjectId>` (CIDFontType2 descendant), `tounicode_id`, `encoding`.
- **Save time**: consumers call `subset_fonts(doc, font_cache)`; `subset_single_font()` handles the simple path via allsorts (`SubsetProfile::Custom`, `CmapTarget::Unicode`), replaces the stream in place, and prefixes `BaseFont`/`FontName` with a random 6-letter tag.  Individual failures log a warning and keep the full font — never an error.

## Design: GID-Remapping via a CIDToGIDMap Stream (no content-stream rewrite)

The content streams already carry original-font GIDs, and `/W` + ToUnicode are keyed by those same GIDs.  Rewriting content streams is the invasive option; the PDF spec provides the right lever instead: replace `CIDToGIDMap /Identity` with a **stream** that maps CID (= original GID) → new GID in the subsetted font.  Then:

- Content streams: **unchanged** (still original GIDs, now interpreted as CIDs).
- `/W` array: **unchanged** (keyed by CID).
- ToUnicode CMap: **unchanged** (keyed by CID).
- `refresh_composite_maps()`: **unchanged**.
- New work is confined to `subset_fonts()`’s composite branch: subset the font bytes, write the CIDToGIDMap stream, swap the name on the dictionaries.

The CIDToGIDMap stream format (PDF 32000-1 §9.7.4.3): `2 × (maxCID + 1)` bytes, big-endian u16 new-GID per CID, 0 (`.notdef`) for unmapped CIDs.  For a font whose largest used GID is ~2,000 that is ~4KB raw and almost nothing after FlateDecode (long zero runs); even a max-GID of 30,000 is a 60KB raw / few-hundred-byte compressed stream.  Always compress it.

### Deriving the old→new GID mapping robustly

Do **not** assume allsorts assigns new GIDs in input order (it appends composite-glyph dependencies and the ordering contract is not documented API).  Derive the mapping through the character map instead, which the existing pipeline already preserves (`CmapTarget::Unicode` keeps a working cmap in the subset):

1.  For each `ch` in `entry.used_chars`: `old_gid = orig_face.glyph_index(ch)` (ttf-parser, same lookup `encode_text_identity()` used at draw time — guaranteeing consistency with what the content streams contain).
2.  Subset with allsorts using `glyph_ids = sorted, deduped [0, old_gid…]` — same recipe as `subset_single_font()`.
3.  Parse the **subsetted** bytes with ttf-parser: `new_gid = subset_face.glyph_index(ch)`.
4.  CIDToGIDMap entry: `map[old_gid] = new_gid`.  Any `ch` that maps in step 1 but not in step 3 is a subsetter fault — fail this font (graceful fallback keeps the full embed) rather than silently rendering `.notdef`.

Note on composite glyphs (TrueType `glyf` components): allsorts pulls component dependencies into the subset automatically; they need no CIDToGIDMap entries because content streams can only reference GIDs that came from `glyph_index()` on a `char`.

## Implementation Steps

All in `medpdf/medpdf/src/`; this builds directly on the uncommitted unicode-text WIP (`pdf_font_composite.rs`, modified `pdf_watermark.rs`/`pdf_subset.rs`/`types.rs`).  Commit or land that work first — this plan layers on top of it.

1.  **`pdf_subset.rs` — dispatch.**  In `subset_fonts()`, replace the `Composite → continue` skip with a call to a new `subset_composite_font(doc, entry)`, keeping the same `Ok(saved)`/`Err(msg)` log-and-fall-back contract.
2.  **`pdf_subset.rs` — `subset_composite_font()`.**
    - Guard: TrueType outlines only for v1 of this pass.  If the font data is CFF-flavored (sfnt version `OTTO`), return `Err("CFF composite subsetting not supported; keeping full font")` — see Out of Scope.
    - Compute `glyph_ids` from `entry.used_chars` via ttf-parser `glyph_index()` (NOT allsorts cmap lookup — must match draw-time GID assignment exactly).  `.notdef` (0) always included.  Bail with `Err` if nothing maps.
    - Subset via allsorts with a composite-appropriate table profile: the existing Custom profile minus `NAME`/`OS_2` is tempting, but reuse the exact existing profile first (`CMAP, HEAD, HHEA, HMTX, MAXP, NAME, OS_2, POST, CVT, FPGM, PREP`) — proven to satisfy Acrobat from the simple-path work (v0.9.1/v0.9.2 regressions were exactly missing-table bugs).  Keeping cmap also enables the mapping derivation above.  Do not call `add_windows_cmap()` — that patch exists for WinAnsi simple fonts; Identity-H rendering never consults the embedded cmap.
    - Derive old→new GID map (§ above); build the CIDToGIDMap byte array sized `2 × (max_old_gid + 1)`.
    - Skip-if-not-smaller check, same as simple path (compare subset + CIDToGIDMap raw sizes against `original_len` for honesty).
    - Mutations, in order:
      a.  Replace `entry.font_stream_id` stream with subsetted bytes (`Length1` updated, compressed) — identical mechanics to the simple path.
      b.  Add the CIDToGIDMap stream object (compressed); set `CIDToGIDMap` in the descendant dict (`entry.cidfont_id`) to its reference, replacing the `/Identity` name.
      c.  Prefix tag: `BaseFont` on the Type0 parent (`entry.font_id`), `BaseFont` on the CIDFontType2 descendant (`entry.cidfont_id`), and `FontName` on the descriptor (`entry.descriptor_id`) — all three with the **same** tag (spec requirement for subset naming; the existing `prefix_base_font`/`prefix_font_name` helpers cover parent and descriptor; descendant needs one more call).
3.  **`pdf_font_composite.rs` — doc comment.**  Update the module header (“v1 embeds the full font…”) to describe the subsetting pass and its CIDToGIDMap mechanism.
4.  **Version bump + CHANGELOG** per the repo’s conventions (minor bump — new capability, no API change).

## Tests

Follow the existing patterns in `pdf_subset.rs::tests` (system-font loading with graceful skip) and `tests/unicode_text_tests.rs`:

- **Size reduction**: build a doc via the real watermark path with kahakō text (e.g. “Sīngers ā ē ī ō ū”), run `subset_fonts()`, assert the FontFile2 stream’s `Length1` shrank by an order of magnitude and the descendant’s `CIDToGIDMap` is now a stream reference.
- **Mapping correctness** (the critical one): for every used char, parse the subsetted font bytes and assert `map[orig_gid] == subset_face.glyph_index(ch)`, and assert the glyph’s advance width in the subset equals the original (guards against silent glyph reshuffling).
- **`/W` and ToUnicode untouched**: assert both objects are byte-identical before/after the subset pass.
- **Round-trip extraction**: save, reload with lopdf, extract text via the ToUnicode CMap, assert the kahakō characters come back (existing unicode tests likely have a helper).
- **Graceful fallback**: CFF font data (any `.otf` with `OTTO` tag) → full font kept, warning logged, `subset_fonts()` still returns `Ok`.
- **Mixed simple+composite doc**: one WinAnsi-only font and one composite font in the same doc; both subset correctly, distinct tags.

Manual acceptance (not automatable here): open the output in Preview and Acrobat Reader; kahakō text must render identically to the full-embed version.  The v0.9.1/v0.9.2 history says Acrobat is the strict viewer — test it first.

## Acceptance Criteria

- Kahakō watermark text costs roughly the same as the WinAnsi subset path (~8–10KB compressed per font), not ~69KB.
- `pdf-orchestrator` (which passes `lossy_text: false`) and `pdf-maker` need **zero changes** and produce validating PDFs (`pdf-dump --validate` clean; `pdf-dump --fonts` shows a `XXXXXX+` prefix on the composite face).
- No regression in the simple-font subsetting path (existing tests stay green).

## Out of Scope (documented deferrals)

- **CFF/OTF composite subsetting** (Bravura-class fonts): needs CIDFontType0 + `FontFile3`, a different embedding shape entirely.  Graceful fallback (full embed) is correct for now; the watermark fonts in production (`SourceSansPro`, `CrimsonText`) are TrueType.
- **Variable-font `gvar`/`fvar` retention**: draw-time rendering already uses default-instance outlines and `hmtx` advances, so dropping variation tables in the subset is consistent with current behavior.  Note it in the module docs; do not add table-retention logic.
- **Content-stream GID rewriting** (compact CID space): strictly an optimization over the CIDToGIDMap approach; the map compresses to near-zero, so there is no payoff.

## Why Not a Workaround

Consumers cannot fix this: the full-font embed happens inside medpdf’s composite path, and `subset_fonts()` is the only subsetting hook they call.  The fix locus is this crate, in the pass that already owns font-stream replacement.
