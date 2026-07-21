# Bug Report: `place_page` (x, y) semantics undefined for non-zero-origin MediaBox — plan, param docs, and code disagree

**Severity:** Medium — misplacement in imposition for cropped/offset sources; three artifacts contradict each other
**Component:** `medpdf` — `src/pdf_place_page.rs:140-207` (`tx = params.x`, no origin compensation); spec artifacts: `feature-plan-place-page.md:74-82`, `src/types.rs:315-318`; behavior locked by `tests/place_page_tests.rs:375`
**Category:** SPEC BUG — **three artifacts disagree; Chris must rule which contract is intended.  A fixer must not silently change either side: the existing test pins today’s code behavior, and pdf-maker ships on it.**
**Verified:** 2026-07-16 deep review — divergence confirmed by trace and by test (orchestrator `t10_place_page_nonzero_mediabox_origin_offset`; page-ops subagent cross-check against the existing test).

## Description

For a source page whose MediaBox origin is not (0, 0) — e.g. `[100, 100, 200, 200]`, common for cropped documents — the three artifacts say different things:

- **`feature-plan-place-page.md`** (the spec): the clip formula `{x} {y} {scaled_width} {scaled_height} re` implies the page’s visible box lands exactly at `(x, y)`.
- **`types.rs` param docs:** “X offset in points (from left edge of destination page)” — ambiguous, but naturally read as “where the page goes”.
- **Code + test:** the `cm` carries `tx = params.x` with no `−scale·x0` compensation, so source **user space** (0, 0) maps to `(x, y)` and the visible MediaBox corner lands at `(x + s·x0, y + s·y0)`.  Confirmed: MediaBox `[50,100,662,892]` at scale 0.5 placed at (0,0) puts the visible corner at (25, 50); `place_page_tests.rs:375` asserts exactly that.

Any N-up/booklet caller computing slot positions from page dimensions (the feature’s stated purpose) misplaces non-zero-origin pages within their slots.

## Reproduction (test-ready)

Source MediaBox `[100, 100, 200, 200]`, `place_page` at `(0, 0)`, scale 1: decode the transform stream and observe `cm = [1 0 0 1 0 0]` — the visible content (all at source coords ≥ 100) lands at (100, 100) on the destination, not (0, 0).

## The decision Chris must make

1. **Compensate (matches the plan and the imposition use case, recommended):** translate by `params.x − s·x0′, params.y − s·y0′` where `(x0′, y0′)` is the transformed MediaBox minimum corner, so the placed page’s visible box lands at `(x, y)` for any origin and rotation.  Requires updating `place_page_tests.rs:375` (it pins the old behavior) and checking pdf-maker call sites — behavior change for non-zero-origin sources only.
2. **Keep user-space mapping:** then fix the spec artifacts — `feature-plan-place-page.md`, the `types.rs` field docs, and the `place_page` docstring must all state “maps source user-space (0, 0) to (x, y); a non-zero-origin MediaBox lands offset by scale × origin”, so callers can compensate themselves.

## Why the fix addresses the bug

The defect is that a caller cannot predict where a page lands from `(x, y, scale)` without reading the source’s MediaBox origin — whichever ruling lands, all three artifacts will state one contract and a test will pin it.
