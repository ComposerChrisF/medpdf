# Bug Report: watermark font/ExtGState registration silently overwrites an existing page resource on key collision

**Severity:** High when triggered (silent rebinding of the page’s own text to the watermark font); trigger requires an object-id/key coincidence, which round-tripped medpdf output makes likelier
**Component:** `medpdf` — `src/pdf_watermark.rs:200-214` (`register_font_in_page_resources`, key = `F{object_id}`), `:217-231` (`GS{object_id}`), landing in `src/pdf_helpers.rs:291-294` (`handle_subdict_in_resources`/`register_in_page_resources` — unconditional `set`, no existence check)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced twice independently (orchestrator test `t1_watermark_font_key_collision_overwrites_existing_resource`; fonts subagent probe `reskey_collision.rs`: `key F9 was silently rebound from (3, 0) to (9, 0)`).

## Description

The watermark path names its resources after the new object’s id (`F{id}`, `GS{id}`) and writes them into the page’s `/Resources` sub-dictionary with a plain `set`.  If the page already has a resource under that name — `F1…Fn` naming is the most common convention in the wild, and medpdf itself emits `F{objid}` keys, so its own round-tripped output is a natural candidate — the existing binding is silently replaced.  Every original text run using that key then renders in the watermark font.

The overlay module already owns the defense (`find_unique_name`, collision-checked against collected keys); the watermark path never adopted it.

## Reproduction (test-ready)

```rust
// Arrange the page's existing Resources/Font to contain the key "F{N}" where
// N = doc.max_id + 1 (the id the watermark font dict will receive), bound to
// a Times-Roman font used by existing content.
add_text_params(&mut doc, page_id, &AddTextParams::new("DRAFT",
    FontData::BuiltIn("Helvetica".into()), "Helvetica"), &mut cache)?;
// Assert: the pre-existing key still resolves to Times-Roman — fails today
// (it now resolves to the watermark's Helvetica dict).
```

## Suggested fix

Before registering, check the page’s effective sub-dictionary for the key: if present and not already referencing this same object, derive a unique key (reuse `find_unique_name` from `pdf_overlay_helpers`, seeded with the sub-dict’s existing keys) and return that key for the `Tf`/`gs` operator.  Apply to both the Font and ExtGState registration helpers.

## Why the fix addresses the bug

The content stream only needs _some_ key; uniqueness is the actual invariant.  Checking-then-renaming preserves every pre-existing binding while the watermark still finds its font, and it reuses the collision machinery the crate already trusts in overlay.

## Related bloat notes (same functions, fix opportunistically)

- `add_known_named_font` allocates a **new** font object per draw call — duplicate built-in font dicts accumulate (bloat, not corruption).  A per-document cache keyed on the base-font name fixes it.
- `push_alpha_ops` similarly adds a fresh ExtGState per call even for identical alpha values.
- `insert_content_stream` creates its `q`/`Q` wrapper streams before discovering the page has no `/Contents`, leaving two orphan objects in that branch.
