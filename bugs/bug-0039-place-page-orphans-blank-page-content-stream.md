# bug-0039: `create_blank_page` + `place_page` leak one orphaned zero-byte stream per destination page

**Severity:** Low (file bloat and object-graph hygiene; no incorrect rendering observed)
**Component:** `medpdf` — `src/pdf_blank_page.rs` and `src/pdf_place_page.rs`, in combination.  Neither is obviously at fault alone, which is the interesting part.
**Category:** CODE BUG — **status: reproduced from the consumer side, mechanism NOT confirmed.**  The surface reading of `place_page` contradicts the obvious explanation; see “Why the obvious mechanism does not hold”.
**Filed:** 2026-09-09 by the pdf-maker session, from the `plan-0003` (`--tile`) prerequisite review.  Consumer-side record: **pdf-maker bug-0007**.

## Description

Every destination page produced by the `create_blank_page` → `place_page` sequence leaves an
unreachable zero-byte stream object in the saved document.  The orphan count equals the
destination page count exactly.

The sequence is pdf-maker’s imposition path (`src/imposition.rs::impose_pages`), and it is the
sequence any imposition consumer would write: make a sheet, place one or more source pages onto
it.

```rust
let dest_page_id = medpdf::create_blank_page(doc, sheet_w, sheet_h)?;
for placement in &sheet.placements {
    medpdf::place_page(doc, dest_page_id, &source_doc, placement.source_page, &params)?;
}
```

`doc.compress()` does not garbage-collect unreachable objects, so the orphans ship in the file.

## Reproduction (verified 2026-07-16 and re-verified 2026-09-09 on medpdf 0.12.0 / pdf-maker v0.13.2)

```bash
pdf-maker -o four.pdf --blank-page "w=612,h=792,count=4"
pdf-maker -o out1.pdf four.pdf all --nup "n=4"                  # 1 sheet
pdf-maker -o out2.pdf four.pdf all --booklet "flip=short_edge"  # 2 sheets
pdf-dump out1.pdf --validate    # 1 orphan stream (+ 1 ObjStm false positive)
pdf-dump out2.pdf --validate    # 2 orphan streams (+ 1 ObjStm false positive)
```

Observed: `out1` orphans object 5; `out2` orphans objects 5 and 17 — each a plain `/Length 0`
stream.  A medpdf-side reproduction should be cheaper and tighter: call `create_blank_page`,
then `place_page` once onto it, save, walk references from the trailer, and assert every
non-`/ObjStm` object is reachable.

**Do not chase the `/ObjStm` warning.**  Every `save_modern` output additionally draws one
“Object N is unreachable from trailer” warning for its object-stream container.  That is a
**pdf-dump validator false positive** — `/ObjStm` containers are referenced from the xref
stream, not the object graph — and it is pdf-dump’s defect to fix in its own repo.  The signal
here is the _plain_ zero-byte streams.

## Why the obvious mechanism does not hold

pdf-maker bug-0007 originally proposed: `create_blank_page` attaches an empty `/Contents`
stream, `place_page` rewrites `/Contents`, the original is orphaned.  Reading the current code
from the outside, that does not appear to be what happens, and the discrepancy is the reason
this is filed rather than fixed:

- `create_blank_page` sets `"Contents" => Object::Reference(content_id)` where `content_id` is
  `Stream::new(dictionary! {}, vec![])` — well-formed, `/Length 0`.  (Confirmed separately: the
  empty streams in a saved four-page blank document reload cleanly and `pdf-dump --strict`
  reports no errors, so this is not a malformed-object problem.)
- `place_page` does **not** replace `/Contents`.  It reads the destination’s current contents,
  resolves them to a ref array (`resolve_contents_to_ref_array(dest_doc, None, …)`,
  `pdf_place_page.rs:325-341`), passes that through `isolate_dest_content_streams`, appends the
  open/source/close references, and writes the array back (`:357`).
- `isolate_dest_content_streams` (`pdf_overlay_helpers.rs:366-400`) _wraps_ rather than
  replaces: it adds standalone `q` and `Q` streams around the destination’s own streams and
  re-emits the originals untouched — explicitly so, per the bug-0018 comment.

On that reading the blank page’s empty stream stays referenced from the final array, and no
orphan should appear.  It does appear, once per sheet.  **The reference is being dropped
somewhere between `resolve_contents_to_ref_array` and the array that is written back** — the
prime suspect being how a single-`Reference` `/Contents` (which is what `create_blank_page`
writes, not an array) is normalized, and whether an empty stream is silently skipped on that
path.  This report deliberately stops at “not confirmed” rather than guessing: it was filed by
a session in the consumer repo, with no medpdf test harness in hand.

## Suggested fix — after the mechanism is confirmed, not before

Whichever layer is actually dropping the reference:

1. **Do not create the doomed object.**  Give `create_blank_page` a content-less variant (or
   have it omit `/Contents` entirely — a page without `/Contents` is legal, and `place_page`
   already handles a source page with none), so imposition sheets never allocate a stream that
   something later discards.
2. **Reclaim it at replacement.**  Wherever the reference is dropped, remove the object rather
   than leaving it in `doc.objects`.
3. **Sweep before serialization** (consumer-side fallback): garbage-collect unreachable objects
   in the save path.  Broader, but it touches every save, and it treats the symptom.

Option 1 or 2 fixes the actual leak; the consumer record (pdf-maker bug-0007) lists 3 as its
own fallback if this turns out not to be medpdf’s to fix after all.  Whichever lands, **pin the
invariant, not the mechanism**: after a `create_blank_page` + `place_page` round-trip, every
non-`/ObjStm` object is reachable from the trailer.

## Why this matters more than “Low” suggests

pdf-maker `plan-0003` adds `--tile`, which splits one large page across many sheets — the first
operation whose sheet count is _derived_ rather than stated.  It is built on this exact
sequence, and it multiplies the leak by the sheet count: a twelve-sheet conference banner leaks
twelve objects.  The plan is pdf-maker’s current top priority, so this is worth settling before
that code is written rather than after.

## Related

- **pdf-maker bug-0007** — the consumer-side record; stays open until this lands.
- **pdf-maker bug-0017** — a spurious `ERROR ... missing the Length entry` on every imposition
  run, briefly and wrongly attributed to `create_blank_page`.  It is pdf-maker’s own metadata
  stream bypassing `Stream::new`.  **Not medpdf’s**, recorded here only so the retraction
  travels with the report it was retracted from.
