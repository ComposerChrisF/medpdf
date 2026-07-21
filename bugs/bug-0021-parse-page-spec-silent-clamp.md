# Bug Report: `parse_page_spec` silently filters/clamps out-of-range pages — behavior is deliberate but undocumented

**Severity:** Medium as a spec gap (the behavior class produced pdf-maker’s worst v0.13.0 bug; the library contract is invisible to consumers)
**Component:** `medpdf` — `src/parsing.rs:44-45` (rustdoc), behavior at `:65-68` and `:88-96`; `medpdf/README.md:156-178`
**Category:** SPEC BUG — **the code matches its tests and pdf-maker’s division of labor; the documentation is silent.  A fixer must NOT change the code without Chris’s explicit ruling, because erroring on out-of-range would be a breaking behavior change for published consumers.**
**Verified:** 2026-07-16 deep review — behavior confirmed by test (orchestrator `t11_parse_page_spec_silent_filter`; spec subagent independently).

## Description

Confirmed behavior:

- `parse_page_spec("99", 3)` → `Ok([])` — out-of-range singles silently dropped (“acts as a filter”, inline comment).
- `parse_page_spec("1-100", 3)` → `Ok([1, 2, 3])` — ranges silently clamped.
- `parse_page_spec("5-", 3)` → `Ok([])` — an entirely empty result, no error.

This is deliberate (inline comments; `tests/parsing_tests.rs:164-175, 256-260` pin it), and pdf-maker v0.13.0 compensates by validating on its own side — its history shows exactly why the silence is dangerous: before v0.13.0, `pdf-maker -o out.pdf two.pdf "1,99"` printed “Operation successful!” and silently wrote a 1-page PDF.  But the public rustdoc and README say only “preserving user-specified order… duplicates removed” — a docs.rs consumer will assume error-on-out-of-range and can reproduce the pdf-maker class of silent page loss.

## The decision Chris must make

1. **Document the filter/clamp contract (recommended, non-breaking):** add to the rustdoc and README: “Pages beyond `max_pages` are silently filtered/clamped, never an error; an empty result is possible.  Callers wanting strictness must validate the returned set against the request (as pdf-maker does since v0.13.0).”  Optionally add a companion strict variant (`parse_page_spec_strict`) returning `Err` on any dropped page, so consumers can opt in.
2. **Change the library to error on out-of-range (breaking):** matches the portfolio’s fail-loud principle but changes published behavior (semver-major signal) and requires re-auditing pdf-maker/pdf-orchestrator call sites first.

## Reproduction

The three assertions above, as a doc-locking test either way.

## Why the fix addresses the bug

Either resolution eliminates the under-specification that lets a consumer mis-assume the contract — the exact failure mode (spec says only “alphabetically”/“parses ranges”, code resolves the ambiguity, downstream ships a silent bug) that the portfolio rules flag as the root of the stage/pdf-orchestrator incidents.
