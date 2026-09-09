# Plan: `parse_page_spec` honors duplicate pages

## Problem

`medpdf::parse_page_spec` deduplicates before returning, so `"1,1"` yields `[1]`.  A
caller cannot express “emit page 1 twice” — a page repeated for a facing-page layout, or
a duplicated insert — because the repetition is gone before the caller ever sees the
list.

**Ruled by Chris, 2026-09-09** (relayed through the pdf-maker session): _“We should honor
duplicates, as that is a useful feature.”_  A page spec is a list of pages to emit, not a
set to select.  The consumer requirement is pdf-maker `bug-0003`.

## Proposed Change

**Change `parse_page_spec` itself** so it stops deduplicating; every caller receives the
ordered list with repeats.  `"1,1"` → `[1, 1]`; `"1-3,2"` → `[1, 2, 3, 2]`.

**Chris ruled this shape specifically**, over the alternative both consumers and this
repo initially preferred — a separate `parse_page_sequence` beside a still-deduplicating
`parse_page_spec`.  The argument for the separate function was that “which pages do I
want?” (a set) and “what sequence do I emit?” (a list) are two questions sharing a
syntax, and that a second function could not surprise a consumer that did not ask for the
change.

**The audit below reversed that preference.**  pdf-orchestrator sizes three counters with
`pages.len()` — section padding, `--dep`, and the dry-run page count — and all three must
agree with what the merge loop actually produces.  They agree today only because they call
the _same_ function as the merge loop.  A separate function would have left the counters
on the old one: no divergence on the day it landed, and a permanent trap for whoever later
switched the merge loop and not the three counters.  One function keeps them consistent by
construction.  Recorded because it is the opposite of what this plan would have said
before the audit, and the reasoning is not obvious from the API surface.

### What must NOT change

**The out-of-range bounds check stays exactly as it is** (medpdf `bug-0021`, v0.12.0).
It runs per element as the spec is parsed — a single page or range start beyond the
document, or an explicit range end beyond it, is an `Err` naming the page — and dedup is a
separate step over the accumulated list.  Removing the dedup therefore cannot weaken it.
`"1,1,99"` on a two-page document still errors naming page 99.  pdf-maker records this as
a contract invariant in its own `CLAUDE.md` and asked for it explicitly; repetition is
legal, out-of-range is not, and the two must stay orthogonal.

## Implementation Notes

- Breaking behavior change ⇒ **MINOR bump** (0.x convention here: MINOR marks breaking;
  precedent `bug-0021` → 0.12.0).  Both consumers are path deps and pick it up at their
  next build, so the version floor matters: whichever commit consumes the new behavior
  raises its `medpdf` requirement.
- Tests in `medpdf/tests/parsing_tests.rs` and the `parsing` unit tests assert
  deduplicated results in at least some cases; those pin today’s behavior and flip with
  the change.  Check for an ordering assumption too — dedup may currently be implemented
  by a sort-and-dedup, in which case removing it also stops reordering `"3,1"`, which is
  a second behavior change hiding inside the first and needs its own test.
- Rustdoc, the README page-spec section, and the `parse_page_spec` doc comment must state
  the sequence semantics: duplicates preserved, order as written, bounds still enforced.

## Consumer audit — done 2026-09-09, before the change rather than after

Performed by the pdf-orchestrator session and **verified here against primary evidence**
(`grep` over `pdf-orchestrator/src`, the cited lines read, and the `.pdfOrch` corpus
re-enumerated independently), per the “subagent survey findings are provenance `ai`”
clause in `strategy-defaults.md`.

- **One call into `medpdf::parse_page_spec`**, at `src/page_range.rs:17`, wrapped as
  `parse_pages`.  Ten sites reach the wrapper (6 in `validate.rs`, 1 in `main.rs`, 2 in
  `pipeline/mod.rs`, 1 in `pipeline/blank_page.rs`).  Count confirmed.
- **No destructive path exists.**  `delete_page` / `remove_page` / `delete_pages` appear
  nowhere in `pdf-orchestrator/src` — it only ever builds an output document.  The
  double-delete risk that motivated the audit does not exist there.
- **The one membership test is immune.**  `pipeline/blank_page.rs:116-117` is the
  `onPages` filter and reads `!pp.contains(&local_page_num)`; `contains` on `[1,1,2]` and
  `[1,2]` are identical.
- **The sequence site wants the new behavior.**  `pipeline/mod.rs:433` iterates the list
  and copies each page, so `<ImportPdf pages="1,1">` will import page 1 twice.
- **The counting sites track automatically** — `pipeline/mod.rs:143`, `main.rs:786`,
  `validate.rs:965` — for the reason given above.
- **The remaining `validate.rs` sites discard the value**, using it only as an
  `if let Err(e)` check.

**Corpus check.**  Across the 149 `.pdfOrch` files under `~/Chris/Sibelius`, the only
comma-bearing specs are two `onPages` values — `"6,12,19"` and `"2-5,7-11,13-18,20-"` —
both strictly ascending and disjoint, and `onPages` is the immune membership site.  Zero
repeats and zero overlapping ranges in the corpus, so no existing file’s behavior moves.
_Scope: every `pages=` and `onPages=` attribute value in all 149 files, by regex, counted
and deduplicated — not a sample._  One value the consumer’s own enumeration omitted turned
up in the independent pass: `pages="2-end"` (1 occurrence), a non-numeric token resolved
by `config.resolve` before parsing.  It carries no comma and does not affect the
conclusion, but it is worth knowing that a spec string reaching `parse_page_spec` may have
been rewritten upstream.

**Verdict: safe to land.**  pdf-orchestrator’s session said so explicitly and asked for
nothing back.

## Why Not a Workaround

A caller cannot restore what the parser already discarded — by the time pdf-maker sees
`[1]` there is no record that the user wrote `"1,1"`.  Re-parsing the spec string in the
consumer to recover the repeats would duplicate medpdf’s page-spec grammar, including the
open-range and `"all"` forms and the bounds check, in every consumer that wants a
sequence.  The parser is the only place the information still exists.

## Related

- **pdf-maker `bug-0003`** — the consumer requirement; stays open until this lands.
- **medpdf `bug-0021`** (fixed, v0.12.0) — the out-of-range contract this must not
  weaken.
- Not on the critical path for pdf-maker’s `plan-0003` (`--tile`).
