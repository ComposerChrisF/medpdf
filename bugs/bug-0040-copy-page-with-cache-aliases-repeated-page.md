# bug-0040: `copy_page_with_cache` called twice for the same page returns one object and lists it twice in `/Kids`

**Severity:** High — it silently produces a malformed page tree, and the two “pages” are one object, so a per-page edit to either changes both.  Nothing in the portfolio can reach it _today_; **`plan-0006` makes it reachable from an ordinary page spec in both consumers.**
**Type:** Code bug in this repo (`medpdf/src/pdf_copy_page.rs`).  Filed 2026-09-10 by the **pdf-maker** session while implementing its `bug-0003` (honor duplicate pages), which `plan-0006` unblocks.

## Description

`copy_page_with_cache` deep-copies the source page through the shared `copied_objects` map, then appends the result to the destination `/Pages` node’s `/Kids` and increments `/Count` (`pdf_copy_page.rs:88-108`).  The cache is documented as deduplicating **shared resources** — “fonts and images” — but it is keyed on source `ObjectId` and the page object is looked up through it like any other object.  So the second call for the same source page hits the cache, returns the **already-copied page’s id**, and then appends _that same id_ to `/Kids` a second time and increments `/Count` again.

The result is a `/Pages` node whose `/Kids` array holds the same reference twice with `/Count 2`.  A page tree is a tree: PDF 32000-1 §7.7.3 gives each page node exactly one `/Parent`, and the copied page’s `/Parent` is set once.  Two slots, one object, one parent.

**The concrete harm does not depend on that spec argument.**  The two entries are one object, so any per-page mutation applied to “the second page” also applies to the first — which is exactly what a consumer’s per-page loop does (a watermark, a stamp, a rotation).  See the second test below: rotating only the second copy rotates the first.

Contrast `copy_page` (no cache), which is already pinned as correct by `pdf_operations_tests.rs::test_copied_pages_are_independent` — it allocates a fresh map per call, so the same page copied twice yields two distinct ids.  **The uncached path has the behavior this one should have**; the cached path trades page identity away along with the resource duplication it meant to avoid.

## Reproduction (verified 2026-09-10, at the medpdf HEAD carrying `plan-0006` as filed)

`bugs/bug-0040/repro.rs` — copy to `medpdf/tests/bug_0040_repro.rs` and run
`cargo test -p medpdf --test bug_0040_repro -- --nocapture`.  Both tests fail today:

```
first  = (3, 0)
second = (3, 0)
get_pages().len() = 2
assertion `left != right` failed: each copy must be its own page object
```

```
after rotating only the second copy, first page /Rotate = Some(90)
assertion `left == right` failed: editing the second copy must not rotate the first
  left: Some(90)
 right: None
```

Note the second one especially: the tree already reports two pages, so a test that only counted pages would pass with the bug fully present.

## Why it matters now — `plan-0006` is what makes it reachable

Today no consumer can hit it, because `parse_page_spec` collapses duplicates before any caller sees them, and each consumer’s merge loop calls this function once per distinct page.  `plan-0006` removes exactly that collapse, so `"1,1"` reaches the merge loop as two elements — and both consumers feed each element straight to this function:

- **pdf-maker** — `src/main.rs::merge_pages` loops the expanded list and pushes each returned id into `page_ids`, which becomes the output page order and the index space for every `pages=` target.
- **pdf-orchestrator** — `src/pipeline/mod.rs:451-469` loops `pages`, calls `copy_page_with_cache` with a shared `copy_cache`, and then calls `apply_children_to_page(&mut doc, dest_page_id, …)`.  **This is the aliasing case in its worst form:** with `<ImportPdf pages="1,1">`, the children applied to the second copy are applied to the first as well, so a watermark meant for one page lands twice on one page and the other “page” is the same object.  Verified against that source, not inferred: the call site and the shared cache are both as described.

`plan-0006`’s consumer audit reached the pdf-orchestrator sequence site and concluded it “wants the new behavior”, which is true — but it checked what the list _contains_, not whether the copy layer can honor a repeat.  That is the gap this report fills, and it is why the plan should not land alone.

## Suggested fix

**Make the cached path re-copy the page node while keeping the resource sharing.**  When the source page id is already present in `copied_objects`, do not return the cached id: build a **new** page object whose dictionary is a clone of the already-copied one — the same `/Contents` and `/Resources` references, which is legal and is the whole point of the cache — set its `/Parent`, and append _that_ new id to `/Kids`.  Only the `/Page` node’s identity is fresh; nothing else is duplicated.

This keeps the deduplication the cache exists for (fonts and images are still copied once) while restoring the property its name implies: one call, one page.

Alternative considered and not recommended: a separate `copy_page_again` entry point, leaving this function alone.  It repeats the trap `plan-0006`’s own audit argued against for the page-count sites — every consumer must know to call the other function, and one that does not gets a silently malformed tree rather than a compile error.

**Whichever shape is chosen, pin it with the two tests in `bugs/bug-0040/repro.rs`**, both of which fail today.  The identity assertion alone is not enough: keep the mutation test, because it is the one that states the property a consumer actually depends on.

## Why this fix addresses the bug

The defect is that a cache meant to deduplicate _resources_ also deduplicates _pages_, in a function whose contract is “copy a page”.  Re-copying the page node at the point where the cache would have returned a hit restores the contract exactly where it is broken, and leaves every other object still shared.

## Relationship to other work

- **medpdf `plan-0006`** (parse_page_spec honors duplicates) — makes this reachable; should land together with this fix or after it, never before.  Amended 2026-09-10 to say so.
- **pdf-maker `bug-0003`** (honor duplicate pages) — the consumer requirement; blocked on both.
- **pdf-orchestrator** — has the same exposure through `<ImportPdf pages="1,1">`, plus the watermark-aliasing case above.  That repo has no session running and no report filed for it; whoever lands this should tell it, or file there.
