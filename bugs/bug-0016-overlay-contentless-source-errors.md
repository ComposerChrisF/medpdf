# Bug Report: `overlay_page` errors on a source page with no `/Contents` (valid blank page); `place_page` already no-ops

**Severity:** Low — valid input rejected loudly (no data loss), and inconsistent with the sibling operation
**Component:** `medpdf` — `src/pdf_overlay.rs:28` (`overlay_page.get(KEY_CONTENTS)?`)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced (orchestrator test `t12_overlay_contentless_source_errors`).

## Description

`/Contents` is optional on a page — a blank page legally omits it.  `overlay_page` does `overlay_page.get(KEY_CONTENTS)?`, so overlaying **from** a contentless page returns `Err(LoPdf(DictKey))` instead of succeeding as a no-op.  `place_page` already implements the correct behavior (`pdf_place_page.rs:46-52`: “no `/Contents`; nothing to place”, returns `Ok`).

## Reproduction (test-ready)

```rust
// overlay doc: one page dict WITHOUT a Contents key.
let r = medpdf::overlay_page(&mut dest, dest_page, &ovl, 1);
assert!(r.is_ok());   // fails today: Err(DictKey("Contents"))
```

## Suggested fix

Mirror `place_page`: match on `overlay_page.get(KEY_CONTENTS)`; on `Err`, `debug!`-log and return `Ok(())`.

## Why the fix addresses the bug

Overlaying nothing is a well-defined no-op, and the two page-composition operations should agree on it; the fix removes a spurious failure without changing any case that has content.

## Related

The same line-of-code family requires `/Resources` to be present on the source page (`pdf_overlay.rs:40`), which fails for pages with **inherited** resources — that deeper issue is bug-0017 and its fix subsumes the resources half; this report covers only the `/Contents` no-op.
