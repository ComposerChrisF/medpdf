# Bug Report: `place_page` ignores the source page’s `/Rotate` — rotated scans impose sideways

**Severity:** Medium — silent wrong output for the feature’s core use case (imposition of scanned/rotated pages)
**Component:** `medpdf` — `src/pdf_place_page.rs` (no call to `get_page_rotation(source_doc, …)` anywhere; the helper exists at `src/pdf_helpers.rs:83`)
**Category:** SPEC BUG — **the spec is the defective artifact: `feature-plan-place-page.md:101` lists “Source pages with existing transforms/rotations” as a required case but defines no semantics.  Chris must rule before code changes; do not just bolt rotation handling on.**
**Verified:** 2026-07-16 deep review — confirmed by trace (page-ops subagent): nothing reads source `/Rotate`; the `cm` is built purely from params.

## Description

A page with `/Rotate 90` is displayed rotated by every viewer — that is what the page “looks like”.  `place_page` copies raw content and builds its transform only from `PlacePageParams`, so a landscape scan imposed via N-up/booklet is placed in its unrotated orientation: visibly sideways relative to the source document, and the slot math is wrong too because the effective width/height swap under 90/270 is not applied.

## Reproduction (test-ready)

1. Source page: MediaBox 612×792, `/Rotate 90`, content drawing a marker near the MediaBox origin.
2. `place_page` at `(0, 0)`, scale 1.
3. Decode the emitted transform stream: no rotation component appears (assert the `cm` is `[1 0 0 1 0 0]`) even though the source page displays rotated.  Define the post-ruling expectation accordingly.

## The decision Chris must make

1. **Honor `/Rotate` (recommended for the imposition use case):** compose the 90°-step rotation about the MediaBox into the placement transform so what gets placed is the page as displayed, and swap effective width/height for 90/270 in any caller-facing size reasoning.  This changes output for existing callers whose sources carry `/Rotate` (today they get the raw orientation).
2. **Document that `/Rotate` is the caller’s responsibility:** state it in the `place_page` docstring and the feature plan, and ensure the caller can retrieve it (`get_page_rotation` is already public).  pdf-maker’s booklet/N-up layer would then need its own handling.

Either way, `feature-plan-place-page.md` must be updated to state the chosen semantics — its current silence is the root defect.

## Why the fix addresses the bug

Both resolutions replace an undefined behavior with a written contract; option 1 additionally makes the primitive’s output match what users see in a viewer, which is what imposition consumers almost always want.
