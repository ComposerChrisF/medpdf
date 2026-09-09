# Plan: `placed_page_size` / `get_page_effective_size` in page-number form

## Problem

The 0.13.0 placement helpers and `place_page` address a page two different ways:

```rust
medpdf::place_page(dest, dest_page_id, source_doc, source_page_num, &params)  // 1-based u32
medpdf::placed_page_size(source_doc, source_page_id, scale, rotation)         // ObjectId
medpdf::get_page_effective_size(source_doc, source_page_id)                   // ObjectId
```

A caller that wants to _plan_ a placement and then _make_ it therefore converts between the
two in the middle — `doc.get_pages().get(&n).copied()` — which is the one piece of
bookkeeping the helpers were added to absorb.  `pdf_helpers::get_page_object_id_from_doc`
does exactly this conversion and is `pub(crate)`, so a consumer cannot even reuse ours.

Reported 2026-09-09 by the pdf-orchestrator session while adopting 0.13.0, **explicitly as
an observation and not a request**: both of its call sites already had the `ObjectId` in
hand, so the cost there was zero.  Recorded here so the asymmetry is a decision rather
than an oversight.

## Proposed Change

Three options, in increasing order of cost.  None is obviously right yet, which is why
this is a plan and not a patch.

1. **Do nothing but say so.**  Document the asymmetry and why it exists — `place_page`
   takes a page number because that is what a page-spec parses to; the helpers take an
   `ObjectId` because they are page-tree queries like `get_page_media_box` and
   `get_page_rotation`, which they sit beside and must match.  Cost: one paragraph.
2. **Make the conversion public.**  Promote `get_page_object_id_from_doc` to
   `pub fn get_page_id(doc, page_num) -> Result<ObjectId>`.  This is the smallest change
   that removes the friction, it is useful well beyond placement, and it adds no second
   spelling of anything.  It is the option this plan leans toward.
3. **Add page-number overloads** — `placed_page_size_for_page_num(doc, page_num, scale,
   rotation)`, and the same for `get_page_effective_size`.  Removes the lookup entirely
   at the cost of two more public names for two existing functions.  A page-address enum
   (`PageRef::Num(u32) | PageRef::Id(ObjectId)`) would collapse the pair, but it is a
   breaking signature change for a problem this small.

Option 3 is the only one that would need a MINOR bump; 1 and 2 are additive.

**The pdf-orchestrator session, told of the page-tree walk below, stated a preference for
option 2** (2026-09-09), and its reasoning is worth keeping: a page-number overload that
quietly walks the tree N times in an N-slot loop is a _worse_ API than the asymmetry it
removes, because the cost is invisible at the call site.  Promoting the conversion leaves
the walk explicit and hoistable.  Not a request and nothing is blocked on it — recorded in
case this plan is picked up.

## Implementation Notes

- The conversion is `doc.get_pages()`, which **walks the whole page tree on every call** —
  the existing note on `get_page_object_id_from_doc` says so.  Any page-number form should
  keep that visible in its docs, because a caller sizing N slots in a loop pays N walks;
  the `ObjectId` form lets a caller hoist the walk.  That is a real argument for keeping
  the `ObjectId` form primary whichever option lands.
- Whatever is added, the two helpers must keep deriving their answer from
  `compute_placement_transform`, so they cannot drift from what `place_page` emits.  That
  invariant is pinned by `effective_size_and_placed_size_agree_for_every_rotate` in
  `tests/place_page_rotate_and_origin_regression.rs`.
- `place_page`’s own signature is not in scope.  Changing it would break both consumers
  for no benefit.

## Why Not a Workaround

The workaround is what both consumers do today — `doc.get_pages().get(&n).copied()` at the
call site — and it is fine.  What makes this worth recording rather than fixing on the
spot is that it is an API-shape question with three defensible answers, and the cheapest
of them (option 1) is to write the reasoning down.  Filing it stops the next session from
either re-discovering the friction or silently adding option 3 because it looked obvious.

## Related

- Consumers that hit it: pdf-orchestrator (`src/pipeline/elements.rs`, `src/booklet.rs`,
  both already holding the `ObjectId`), and pdf-maker’s imposition path, which works in
  page numbers throughout.
- The helpers were added in 0.13.0 for bug-0023/bug-0024; the module docs in
  `src/pdf_place_page.rs` § “The placement contract” carry the contract they report on.
