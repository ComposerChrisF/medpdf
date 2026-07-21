# Bug Report: overlay/place decode→re-encode round trip corrupts or deletes inline images — including in untouched destination content

**Severity:** High — silent corruption of the destination page’s own content; merely stamping a page destroys its inline images
**Component:** `medpdf` — `src/pdf_overlay_helpers.rs:130-203` (`modify_content_stream`), called on destination streams with `key_mapping = None` at `pdf_overlay.rs:125` and on source streams at `pdf_overlay.rs:95` / `pdf_place_page.rs:131`.  Root interaction: lopdf 0.42 `parser/mod.rs:627-658` and `content.rs::encode` + `writer.rs:503-525`.
**Category:** CODE BUG in medpdf (unnecessary lossy round trip) sitting on an upstream lopdf defect (see “Upstream” below).
**Verified:** 2026-07-16 deep review — reproduced in three modes (orchestrator test `t6_overlay_corrupts_inline_image_in_dest_content`; page-ops subagent tests covering `/CS /RGB`, `/CS /G`, and filtered images).

## Description

`modify_content_stream` round-trips every content stream through lopdf’s `Content::decode` / `Content::encode`.  For inline images (`BI … ID <data> EI`) that round trip is broken in lopdf 0.42:

- **Parseable inline image** (e.g. `/CS /RGB`, uncompressed): decode yields a `BI` operation whose operand is an `Object::Stream`; encode writes operands via `Writer::write_object`, which serializes a Stream as `<<dict>>stream\n…\nendstream` **before** the `BI` operator — invalid content-stream syntax.  Rendering breaks from that point in the stream.
- **Unparseable-to-lopdf inline image** (`/CS /G` — a legal abbreviation lopdf’s `image_data_stream` does not resolve; or any filtered image `/F /AHx`, `/Fl`): the parser skips to `EI` and returns a `BI` operation with **no operands** (`parser/mod.rs:657`) — the image is silently deleted, and a bare orphan `BI` remains in the re-encoded stream.

The aggravating medpdf-side fact: for **destination** streams the round trip has no purpose except adding a `q`/`Q` wrapper (`key_mapping` is `None`).  So watermark-stamping a page corrupts that page’s own untouched inline images.  Additionally, when the destination’s `/Contents` is an array, `resolve_contents_to_ref_array` returns those stream objects by reference (`pdf_overlay_helpers.rs:314-331`), so the in-place mutation also corrupts any **other page sharing those content streams**.

## Reproduction (test-ready)

```rust
// Dest page content: q BI /W 1 /H 1 /CS /G /BPC 8 ID <1 byte> EI Q
medpdf::overlay_page(&mut dest, dest_page, &ovl, 1)?;   // stamp anything
// Decode dest's first content fragment: valid "BI … ID … EI" is gone —
// either stream/endstream garbage or a bare `BI` with the image data deleted.
```

Assert the fragment still contains a syntactically valid inline image; it fails today.

## Suggested fix

1. **Destination side (complete fix): stop re-encoding.**  Wrap destination content with two tiny standalone streams instead — prepend a one-op `q` stream and append a close stream containing `1 + max(0, imbalance)` `Q` ops.  This is exactly the mechanism `insert_content_stream` (watermark path) and `place_page`’s transform streams already use.  The q-imbalance count can be computed from a read-only decode (`count_q_balance` already exists) without ever writing the decoded form back.
2. **Source side (where renaming genuinely requires rewriting):** detect `BI` operations during the rewrite and either (a) fail loudly (“overlay source contains inline images; unsupported until lopdf’s encoder handles them”), or (b) re-emit them correctly by writing `BI <dict entries> ID <data> EI` explicitly.  Silent corruption is the only unacceptable option.
3. **Record the upstream defect** at the repo root (e.g. `LOPDF_INLINE_IMAGE_BUG.md`, the same convention as pdf-orchestrator’s `LOPDF_SAVE_MODERN_BUG.md`), cited from the workaround code, since it must outlive any individual bug report.  Consider filing it against lopdf upstream.

## Why the fix addresses the bug

The destination rewrite has zero functional need — PDF concatenates `/Contents` fragments, so isolation wrappers work as separate streams; removing the lossy round trip eliminates the whole corruption class (including the shared-stream mutation) rather than chasing lopdf’s parser gaps one colorspace at a time.  On the source side, a loud error converts silent corruption into a diagnosable limitation until a correct emitter exists.

## Related

- bug-0019 (operations split across `/Contents` fragments) shares the same call site; fix 1 above also resolves its destination-side facet.
- Minor, same function: operand renaming is operator-blind (`pdf_overlay_helpers.rs:152-161`) — any `Name` operand matching a mapped resource key is renamed, including BDC/BMC tags or colorspace names that merely coincide with a source resource key.  Contrived in practice; worth a guard (only rename operands of resource-consuming operators: `Tf`, `Do`, `gs`, `cs`/`CS`, `scn`/`SCN`, `sh`, `BDC`/`DP` property names) while editing this code.
