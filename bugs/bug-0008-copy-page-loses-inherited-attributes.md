# Bug Report: `copy_page` silently loses inherited page attributes (MediaBox, Resources, Rotate, CropBox)

**Severity:** High — silent wrong output: copied pages can lose their size, their fonts, and their rotation, with no error.
**Component:** `medpdf` — `src/pdf_copy_page.rs:40-63` (`copy_page_with_cache`), interacting with the deliberate `/Parent` skip in `src/pdf_helpers.rs:164`.
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced twice independently (orchestrator test `t3_copy_page_loses_inherited_mediabox_and_resources`; page-ops subagent test `copy_page_inherited_attributes` including `/Rotate 90` loss).

## Description

PDF page attributes `Resources`, `MediaBox`, `CropBox`, and `Rotate` are inheritable: they may live on an ancestor `/Pages` node instead of the page dict (PDF 32000-1 §7.7.3.4).  Real producers use this (pdfTeX and others commonly put `MediaBox` on the root `Pages` node).

`deep_copy_object` skips `/Parent` — correct, otherwise the whole source tree would be copied — but nothing materializes the inherited values onto the copied page.  The copied page arrives in the destination with only its own keys.  Confirmed: a source page inheriting `MediaBox`/`Resources`/`Rotate 90` from its `Pages` node copies over with keys `["Type", "Contents", "Parent"]` only.  The result has no MediaBox anywhere (invalid page; viewers guess letter size), its content references `/F1` with no resources (text disappears), and the rotation is dropped — all silently.

## Reproduction (test-ready)

```rust
// Source: MediaBox + Resources {Font F1} + Rotate 90 set on the PAGES node;
// the page dict has none of them, content is "BT /F1 12 Tf (hello) Tj ET".
assert_eq!(medpdf::get_page_media_box(&src, page_id), Some([0.0, 0.0, 300.0, 400.0])); // sanity
let new_page = medpdf::copy_page(&mut dest, &src, 1)?;
// All three asserts FAIL today:
assert!(medpdf::get_page_media_box(&dest, new_page).is_some());
assert!(dest.get_dictionary(new_page)?.get(b"Resources").is_ok());
assert_eq!(medpdf::get_page_rotation(&dest, new_page), 90);
```

## Suggested fix

In `copy_page_with_cache`, after `deep_copy_object_by_id` and before re-parenting: walk the **source** page’s `/Parent` chain and, for each inheritable attribute (`Resources`, `MediaBox`, `CropBox`, `Rotate`) absent from the copied page dict, copy the nearest inherited value down onto the page (deep-copying reference values through the same `copied_objects` map).  The existing `get_page_media_box` (`pdf_helpers.rs:23-43`) shows the walk pattern; generalize it into a small `resolve_inherited(doc, page_id, key)` helper — `overlay_page`/`place_page` need the same helper for bug-0017.

## Why the fix addresses the bug

Skipping `/Parent` is correct but the inherited values live only up that chain; flattening them onto the leaf page is the standard materialization and is exactly what PDF inheritance semantics require for the page to mean the same thing under a new parent.  Copying “what the viewer would resolve” makes the copied page render identically regardless of the source tree shape.
