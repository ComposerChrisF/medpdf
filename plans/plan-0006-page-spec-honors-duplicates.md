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
  the change.
- **Order is already preserved — there is no second behavior change hiding inside this
  one.**  Checked against the built parser rather than the source: `parse_page_spec("3,1",
  10)` returns `[3, 1]`, and `parse_page_spec("1,1", 10)` returns `[1]`.  So dedup is not
  a sort-and-dedup, and removing it changes exactly one thing.  (This note previously
  speculated the opposite as a hazard to watch for; it is settled.)
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
_Scope: every `pages=` and `onPages=` attribute value in all 149 `.pdfOrch` files **on
disk**, by regex, counted and deduplicated — not a sample._

**There are two corpora here, and the difference is worth stating.**  The consumer’s own
pass covered **124** files, not 149: its `grep` is ugrep, which honors `.gitignore`, and
`~/Chris/Sibelius/.gitignore` ignores everything and whitelists only the current config
files — deliberately excluding the `~Old~`, `~AutoMake2~`, and `~BackupScores~` archive
directories.  The omission was silent: no warning, the count simply came back low.  For
the question “what do _live_ specs look like?” the tracked subset is arguably the better
arbiter, since that `.gitignore` is a curated statement of which files are the real build
instructions — but it must be **chosen and named**, not inherited from a tool’s default.
Both passes agree on the answer.

**The one value that differed is not live evidence.**  `pages="2-end"` (1 occurrence,
`F163c-HineMaTov-SATB/With Piano Reduction/#Licensed.pdfOrch`) is untracked and ignored
(`.gitignore:14`), uses the `<MergeItem>` alias pdf-orchestrator removed in v0.6.0, and
**does not parse**: `parse_page_spec("2-end", 10)` returns
`Err(… input: "end", code: Eof)` — there is no `end` keyword in `parsing.rs`, and
`config.resolve` leaves a bare token alone.  It is a stale artifact that predates two
breaking changes and would fail on both.

What survives from it is a fact about the **code path, not the corpus**: `pipeline/mod.rs`
runs `config.resolve` over the `pages` attribute before parsing, so a spec written
`pages="[$SomeVar$]"` reaches `parse_page_spec` as whatever the variable expands to.  No
file in either corpus actually does this — a search for a `[$…$]` inside any `pages=` or
`onPages=` value returns nothing — so it constrains a future “what can a page spec
contain?” question rather than this one.

**Verdict: safe to land.**  pdf-orchestrator’s session said so explicitly and asked for
nothing back.

**Deconflict before landing.**  pdf-orchestrator is mid-flight on a plan of its own —
_pdf-orchestrator_ `plan-0002`, making `--dry-run` execute the pipeline and skip only the
save.  That plan changes what the dry-run path does with a parsed page list, which is one
of the three counting sites above, so landing a `parse_page_spec` behavior change
underneath it would put two changes on the same code at once and make a regression hard
to attribute.  Check with that session before starting.  (Note the ID collision: a bare
`plan-0002` is ambiguous portfolio-wide as of 2026-09-09 — medpdf’s is multi-line
watermark text, pdf-orchestrator’s is the dry-run change.  Cite both with the repo name,
per the plans rule.)

## Why Not a Workaround

A caller cannot restore what the parser already discarded — by the time pdf-maker sees
`[1]` there is no record that the user wrote `"1,1"`.  Re-parsing the spec string in the
consumer to recover the repeats would duplicate medpdf’s page-spec grammar, including the
open-range and `"all"` forms and the bounds check, in every consumer that wants a
sequence.  The parser is the only place the information still exists.

## BLOCKER found 2026-09-10 — this plan is not sufficient on its own (medpdf `bug-0040`)

Filed by the pdf-maker session while starting its `bug-0003`, and **verified here against this
repo’s own code and test fixtures**, not inferred from the consumer side.

**Preserving duplicates in the parsed list is only half of honoring a duplicate page.**  The
other half is in this repo: `copy_page_with_cache` looks the source page up through the same
`copied_objects` map it uses for fonts and images, so the **second call for the same page
returns the already-copied page’s id** — and then appends that id to `/Kids` a second time and
increments `/Count` again (`pdf_copy_page.rs:88-108`).

Measured, with `fixtures::create_pdf_with_pages(2)`:

```
first  = (3, 0)
second = (3, 0)
get_pages().len() = 2
```

Two `/Kids` slots, one object.  And because they are one object, a per-page edit to “the second
page” edits the first — rotating only the second copy leaves the first with `/Rotate Some(90)`.
Full report and a two-test repro: **`bugs/bug-0040-copy-page-with-cache-aliases-repeated-page.md`**.

**Consequence for sequencing: do not land this plan alone.**  Today the bug is unreachable
precisely because `parse_page_spec` collapses duplicates; this plan removes that collapse and
hands both consumers’ merge loops a repeated page number, which is the input that produces the
malformed tree.  Land `bug-0040` first, or land the two together.

**The consumer audit above missed it, and the reason is worth keeping.**  The audit asked what
each call site does with the _list_ — iterate, count, or test membership — and that framing was
right for the question it was asked.  It could not surface this, because the fault is not in
what the consumers do with the list; it is in what this repo does when the same element arrives
twice.  A list-shaped audit will not find an identity-shaped bug.

**pdf-orchestrator is exposed too, and nobody has told it.**  `src/pipeline/mod.rs:451-469`
loops the parsed pages, calls `copy_page_with_cache` with a shared cache, and then calls
`apply_children_to_page` on the returned id — so `<ImportPdf pages="1,1">` would apply that
page’s children twice to one object.  That repo has no session running; whoever lands this
should file there or notify it.

## Deconflict status — checked 2026-09-10

The plan asks for a check with the pdf-orchestrator session before starting.  State as of
2026-09-10, verified by reading that repo rather than by asking: **_pdf-orchestrator_
`plan-0002` is filed but unimplemented** — `plans/plan-0002-dry-run-executes-without-saving.md`
exists, its notes are committed (`88c5ba0`, `eff1030`), and no implementation commits follow;
that repo’s recent commits are v0.16.5/v0.16.6 work on unrelated paths.  So the two changes are
**not** in flight together, which was the hazard the deconflict note existed to prevent.

Chris authorized proceeding on 2026-09-10, relayed by the pdf-maker session, which he told to
hand this work to the medpdf session directly.

## Related

- **medpdf `bug-0040`** (filed 2026-09-10) — `copy_page_with_cache` aliases a repeated page.
  **A hard prerequisite for this plan**, per the blocker section above.
- **pdf-maker `bug-0003`** — the consumer requirement; stays open until this lands.
- **medpdf `bug-0021`** (fixed, v0.12.0) — the out-of-range contract this must not
  weaken.
- Not on the critical path for pdf-maker’s `plan-0003` (`--tile`).
