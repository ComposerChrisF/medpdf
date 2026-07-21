# Bug Report: `place_page` validates only `scale` for finiteness — NaN/∞ in x, y, or rotation writes invalid PDF tokens

**Severity:** Low — invalid output on caller error, but silently (garbage tokens instead of a loud `Err`)
**Component:** `medpdf` — `src/pdf_place_page.rs:32-34`
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — confirmed by trace (page-ops subagent): non-finite `x`/`y`/`rotation` flow into `Object::Real` and serialize as `NaN`/`inf`.

## Description

`place_page` checks `params.scale.is_finite()` but not `x`, `y`, or `rotation`.  A NaN or infinity in any of those (e.g. from a caller’s division by a zero page dimension — see the related medpdf-image case in bug-0015) reaches the `cm`/`re` operands and is written as the literal tokens `NaN`/`inf`, which are not valid PDF numbers.  Viewers see a corrupt content stream.

## Reproduction (test-ready)

```rust
let r = place_page(&mut dest, dest_page, &src, 1,
        &PlacePageParams::new(f64::NAN, 0.0, 1.0));
assert!(r.is_err());   // fails today: Ok, with "NaN" written into the stream
```

## Suggested fix

Extend the existing guard: validate `x`, `y`, `scale`, and `rotation` with `is_finite()`, naming the offending field in the error.

## Why the fix addresses the bug

Same rationale as the existing scale check — caller mistakes should surface as an immediate `MedpdfError`, not as a PDF that fails downstream in a viewer with no traceable cause.
