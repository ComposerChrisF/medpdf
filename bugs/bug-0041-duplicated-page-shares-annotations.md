# bug-0041: A page copied twice shares its `/Annots` objects, and each annotation’s `/P` points at the first copy

**Severity:** Medium — narrower than `bug-0040` and not a malformed page _tree_, but the same aliasing class one level down: two pages, one annotation object, and a `/P` that is wrong on one of them.
**Type:** Code bug in this repo (`medpdf/src/pdf_copy_page.rs`).  Filed 2026-09-10 by the **pdf-maker** session, at the medpdf session’s own invitation — it flagged this residual when landing `bug-0040` and asked whether pdf-maker could reach it.  **It can, and this is the reproduction.**

## Description

With `bug-0040` fixed, copying the same source page twice through one cache correctly yields two distinct page objects.  Their `/Annots` arrays, however, still list the **same** annotation objects — the annotations are ordinary referenced objects and hit the resource cache like fonts and images do.  The first copy’s annotation carries `/P` pointing at the first copy; the second copy lists that same annotation, so its `/P` names a page the annotation is not on.

Two consequences, in increasing order of how much they matter:

1. **`/P` disagrees with `/Annots` on the second copy.**  PDF 32000-1 §12.5.2 defines `/P` as “the page object with which this annotation is associated”; an annotation reachable from two pages’ `/Annots` can satisfy that for at most one of them.
2. **The two annotations are one object, so editing either edits both** — the same property `bug-0040` was filed for.  Benign for a static `/Link`, not benign for anything stateful: a widget/form field appearing on two pages shares one value, and any later per-page annotation edit silently applies to both copies.

## Reproduction (verified 2026-09-10, medpdf 0.15.0, with the bug-0040 fix in place)

`bugs/bug-0041/repro.rs` — a pdf-maker-level probe, run from the pdf-maker repo as
`tests/annot_probe.rs`.  It builds a one-page PDF whose page carries a `/Link` annotation with
`/P` set, runs `pdf-maker -o out.pdf annot.pdf "1,1"`, and reports:

```
page objects: [(4, 0), (7, 0)]
annots per page: [[(6, 0)], [(6, 0)]]
page (4, 0) annot (6, 0) has /P = Some((4, 0))  (matches its own page: true)
page (7, 0) annot (6, 0) has /P = Some((4, 0))  (matches its own page: false)
```

The page objects differ — `bug-0040`’s fix is working — and the annotation does not.

**A medpdf-level test is the better home** and should be easy: the shape is
`bugs/bug-0040/repro.rs` with an `/Annots` array added to the fixture page, asserting that the
two copies’ `/Annots` entries are disjoint and that each annotation’s `/P` is its own page.  The
pdf-maker probe is committed here as evidence of reachability through a shipping consumer, not
as the test to keep.

## Can pdf-maker actually hit it?

Yes — that was the question asked, so answering it precisely:

- pdf-maker merges **arbitrary caller-supplied PDFs**, and `/Annots` is ordinary in them: links in program notes, anything exported from Word with hyperlinks, any form.
- Duplicating a page is now a **supported, documented** operation as of pdf-maker `bug-0003` (it is what medpdf 0.15.0 unblocked), so `"1,1"` on such a file is a normal invocation rather than an exotic one.
- pdf-maker never edits annotations itself, so it cannot trigger consequence (2) on its own.  It hands the aliased pair to whatever opens the file next.

So: reachable, low frequency, and the harm depends on the consumer.  **Not urgent** — nothing in the portfolio is known to duplicate an annotated page today — but it is the last member of the class `bug-0040` opened, and the fix shape is presumably the same one line, extended to the page’s annotation references.

## Suggested fix

The `bug-0040` fix drops the page’s own entry from `copied_objects` before the deep copy so the page node is always freshly copied.  The same reasoning applies one level in: an annotation belongs to exactly one page, so on a repeat copy its entries should be dropped too, giving each copy its own annotation objects — then set each new annotation’s `/P` to the page that now owns it.

Worth deciding explicitly, since it is a judgment call rather than a mechanical extension: **an annotation’s _appearance stream_ should stay shared** (it is a resource, and that is what the cache is for); only the annotation dictionary and its `/P` need to be per-copy.

## Why this fix addresses the bug

Same root as `bug-0040` — a cache meant to deduplicate _resources_ also deduplicating things that carry per-page identity.  Annotations are page-owned, not shared resources, so they belong on the freshly-copied side of that line; their appearance streams do not.
