# Bug Report: deep copy has no cycle protection — `copy_page` aborts the process on ordinary annotated pages

**Severity:** Critical — process-killing crash (stack overflow, SIGABRT), not a catchable `Err`, on valid and common input.
**Component:** `medpdf` — `src/pdf_helpers.rs:128-147` (`deep_copy_object_by_id`), reached via `copy_page` / `copy_page_with_cache` (`src/pdf_copy_page.rs:49-50`), `overlay_page`, and `place_page` (any caller of the deep copy).
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced twice independently (orchestrator: `/Dest` self-link; page-ops subagent: annotation `/P` back-reference).  Both runs abort with `thread 'main' has overflowed its stack / fatal runtime error: stack overflow, aborting`, exit code 134.

## Description

`deep_copy_object_by_id` inserts the source→dest ID mapping into `copied_objects` only **after** the full recursive copy of the object returns (`pdf_helpers.rs:145`).  While an object is being copied, it is invisible to its own descendants, so any reference cycle that does not pass through a `/Parent` key (the only skipped key, `pdf_helpers.rs:164`) recurses forever and overflows the stack.

Real-world triggers are ordinary, not exotic:

- An annotation carrying `/P` (page back-reference) — Acrobat writes `/P` into virtually every annotation it creates.  Page → `/Annots` → annot → `/P` → page.
- A link annotation whose `/Dest` (or `/A` → `/D`) array names its own page — “back to top” internal links.

Related non-crash defect from the same mechanism: an annotation `/Dest` pointing at a **different** page silently deep-copies that entire page and its subtree into the destination as orphan objects — copying one page of a heavily cross-linked document can drag in most of the document as bloat.

## Reproduction (test-ready)

```rust
// Source doc: one page whose annotation references the page itself.
let annot_id = src.add_object(dictionary! {
    "Type" => "Annot", "Subtype" => "Link",
    "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
    "P" => Object::Reference(page_id),          // or "Dest" => [page_id /Fit]
});
// page dict gets "Annots" => [annot_id]; page in the tree as usual.
let mut dest = /* fresh doc with catalog+Pages */;
medpdf::copy_page(&mut dest, &src, 1);          // never returns: stack overflow abort
```

Test harness note: run the repro in a child process (or `#[should_panic]` will NOT work — the abort kills the whole test runner).  A `std::process::Command` invocation asserting a non-zero, signal-class exit status is the reliable shape.  After the fix, the same call must return `Ok` and the test can assert the copied annotation’s `/P` reference points at the **copied** page ID.

## Suggested fix

Two-phase copy in `deep_copy_object_by_id`:

```rust
if let Some(&new_id) = copied_objects.get(&source_object_id) { return Ok(new_id); }
let new_id = dest_doc.new_object_id();
copied_objects.insert(source_object_id, new_id);      // visible BEFORE recursion
let new_obj = deep_copy_object(dest_doc, source_doc, source_doc.get_object(source_object_id)?, copied_objects)?;
dest_doc.objects.insert(new_id, new_obj);
Ok(new_id)
```

## Why the fix addresses the bug

The map already exists to answer “have I copied this object?” — inserting the reservation before recursing makes the in-progress object answer that question for its own descendants, turning any cycle into a plain back-reference.  For acyclic graphs the output is identical (same objects, same topology), so no other behavior changes.

## Non-goals / related

The whole-document-drag-in via `/Dest` links (bloat, not a crash) is only partially improved by this fix (each referenced page is at least copied once, not repeatedly).  If bloat matters, a follow-up can skip or shallow-copy annotation destination references — that is a design decision, not part of this fix.
