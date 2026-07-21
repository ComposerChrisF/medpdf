# Bug Report: `place_page` does not isolate destination graphics state — a leaked dest CTM displaces the placed page

**Severity:** Medium-High — silent misplacement; contradicts the function’s own documented contract
**Component:** `medpdf` — `src/pdf_place_page.rs:225-247` (dest contents appended un-neutralized); doc claim at `src/pdf_place_page.rs:22-24`
**Category:** BOTH — the docstring (“without interfering with each other or with existing destination content”) states the correct contract; the code fails to deliver it.  Fix the code to match the doc.
**Verified:** 2026-07-16 deep review — reproduced twice (orchestrator test `t8_place_page_inherits_dest_graphics_state`; page-ops subagent test `place_page_dest_leaked_ctm_not_isolated`).

## Description

Nothing requires a page’s content to leave the graphics state clean: a top-level `0.5 0 0 0.5 0 0 cm` with no `q`/`Q` is legal, and scanned-page content commonly opens with exactly such a CTM.  Because content streams concatenate, that leaked state applies to everything appended afterward.

`overlay_page` defends against this by wrapping destination content in `q`/`Q` (`pdf_overlay.rs:125`); the watermark path does too (`insert_content_stream`).  `place_page` — whose whole purpose is precise positioning — appends `[open q+cm, source…, close Q]` after the destination content with no neutralization.  Confirmed: with dest content `0.5 0 0 0.5 0 0 cm`, a placement requested at (100, 100) at scale 1.0 renders at (50, 50) at scale 0.5.

## Reproduction (test-ready)

```rust
// Dest page content: b"2 0 0 2 0 0 cm"  (balanced ops, no q/Q — legal)
place_page(&mut dest, dest_page, &src, 1, &PlacePageParams::new(100.0, 100.0, 1.0))?;
// Fragments today: [dest content untouched, open, source…, close]
// → the dangling cm still applies; placed content lands at (200,200) @2x.
```

Assert a `q`-wrapper fragment precedes the dest content and a balancing `Q` fragment follows it (before the placement’s open stream) — fails today.

## Suggested fix

Isolate destination content the same way the fixed overlay path does (bug-0018’s mechanism): prepend a standalone one-op `q` stream and insert a standalone close stream with `1 + max(0, q_imbalance)` `Q` ops after the dest content, before the placement’s open stream.  Do **not** re-encode the destination streams (that is bug-0018’s corruption).

## Why the fix addresses the bug

With the destination bracketed by `q`/`Q`, any state it leaks — CTM, clip, colors, unbalanced saves — is popped before the placement transform runs, making the documented self-containment true in both directions and unifying dest handling across overlay, watermark, and place.
