# Bug Report: `place_page` clip for arbitrary rotations is the AABB of the transformed MediaBox, not the MediaBox itself

**Severity:** Low-Medium — content outside the source MediaBox can leak into the corner wedges for non-90° rotations
**Component:** `medpdf` — `src/pdf_place_page.rs:163-195` (clip rect built from transformed-corner AABB)
**Category:** CODE BUG — the docs are the contract: `types.rs:324` says “clip source content to its MediaBox”, and the feature plan’s clipping section exists so pages “don’t bleed into adjacent slots”.
**Verified:** 2026-07-16 deep review — confirmed structurally (orchestrator test `t9_place_page_rotated_clip_is_aabb`: a 100×100 page rotated 45° emits an axis-aligned clip `re` of width ≈ 141.42).

## Description

With `clip = true` and a rotation that is not a multiple of 90°, the emitted clip is the axis-aligned bounding box of the four transformed MediaBox corners.  The AABB strictly contains the rotated page rect, so source content lying **outside** the MediaBox (bleed, crop-hidden artwork — the exact thing clipping exists to suppress) shows through in the four corner wedges.  For 0/90/180/270 the AABB equals the rect, so the common cases are unaffected — which is why the existing `test_place_page_rotation_45` (which checks the AABB numbers) never caught it.

## Reproduction (test-ready)

1. Source 100×100 page whose content draws a marker at (150, 50) — outside the MediaBox.
2. `place_page` with `rotation(45.0)`, `clip(true)`.
3. Rasterize (pdf-test-visual) and assert the marker is not visible — fails today.  Structural variant: decode the open stream and assert the clip is a 4-point path matching the transformed corners rather than an `re` whose width is ≈ 141.42 for a 100-pt page.

## Suggested fix

Emit the transformed MediaBox quadrilateral as the clip path instead of its AABB: `m x1 y1, l x2 y2, l x3 y3, l x4 y4, h, W n` using the four already-computed corner points (`pdf_place_page.rs:166-183` computes them and then throws away everything but min/max).

## Why the fix addresses the bug

The rotated quad **is** the MediaBox under the placement transform — clipping to it implements the documented contract exactly, for every angle, and reduces to the current rectangle for the 90°-step cases.
