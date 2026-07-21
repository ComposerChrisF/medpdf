# Bug Report: README says image rotation is “around the image anchor point”; code rotates around the box center

**Severity:** Low (documented contract vs behavior; the math itself is correct)
**Component:** `medpdf-image` — `README.md:15` vs `src/lib.rs:634-636` and `src/svg.rs:265-266`
**Category:** SPEC BUG — **the two artifacts disagree and Chris must rule which is intended.  Do not silently change the code: pdf-maker’s `--draw-image rotation` behavior ships on top of it.**
**Verified:** 2026-07-16 deep review — mismatch confirmed; the rotation matrix itself verified correct (`T(c)·R·T(−c)` in PDF row-vector order rotates about `c`).

## Description

`medpdf-image/README.md` states rotation happens “around the image anchor point” — the `(x, y)` position.  The code computes the pivot as the **box center**: `cx = params.x + params.width / 2.0` (`lib.rs:634-636`), and `svg.rs:265-266` does the same for SVG placement.  For any nonzero rotation a consumer positioning by anchor gets a different placement than documented.

## Reproduction

Place a 100×100 image at `(0, 0)` with `rotation = 90`.  Under the documented anchor semantics the image would occupy x ∈ [−100, 0]; under the implemented center semantics it stays centered on `(50, 50)`.  Decode the `cm` matrix from the content stream to observe the pivot.

## Resolution needed from Chris

- **If center rotation is intended** (likely — it matches pdf-maker’s shipped behavior and is the common UX): fix the README sentence to “around the box center”.  One-line doc fix.
- **If anchor rotation is intended:** change `cx/cy` to `params.x/params.y` in **both** `lib.rs` and `svg.rs`, and treat it as a breaking behavior change for pdf-maker (`--draw-image`/`--watermark` visuals shift for every rotated placement in existing scripts).

## Why the fix addresses the bug

Either resolution makes the documented pivot and the actual pivot the same point; the ruling only decides which artifact moves.  Given downstream reliance, the doc fix is the recommended default.
