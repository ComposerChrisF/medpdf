# Plan: True Redaction — Remove Text From a PDF, Not Paint Over It

## Problem

medpdf can draw an opaque rectangle (`DrawRectParams`, pdf-maker’s `--draw-rect`), and that is precisely the **fake redaction** behind the well-known leaked-court-filing failures: the box hides the glyphs from a human eye while the content stream still carries them, so any text extraction — `pdf-dump --text`, copy and paste, or an AI reading the file — recovers everything underneath.

The motivating use is a financial statement (bank, card, brokerage) that must be shown to an AI with account and routing numbers removed.  The adversary is not a human eye but a text extractor _and_ a page rasterizer: an AI may be given the text layer, a rendered image of the page, or both.  Both must be clean.

The format-agnostic half of this problem — the secrets file, the matching tiers, the hold-for-review workflow, the no-echo output contract — is specified as a tool, `~/Chris/Proj/Coding/cli-specs/pii-redact-spec.md`.  This plan is the PDF-native half: the library operation that removes what that matcher selects.

## Proposed Change

A new module, `medpdf::pdf_redact`, with one entry point:

```rust
pub fn redact_document(doc: &mut Document, spec: &RedactSpec) -> Result<RedactReport>
```

**`RedactSpec`** carries targets of two kinds:

- **Glyph targets** — selected by a caller-supplied matcher over the decoded glyph sequence of each text-showing context (page content, and each Form XObject).  medpdf decodes and positions glyphs; it does not know what an account number is.  The matcher (pii-redact’s engine) returns spans to remove.
- **Area targets** — `(page, rect)` in default user space, for things no text search can find.

**What “redact” does to a target:**

1. **Removes the glyphs from the content stream.**  A `Tj`/`TJ` string is split at the target boundaries, the removed codes are dropped, and a `TJ` adjustment equal to their total advance takes their place, so every glyph after them stays exactly where it was.  `'` and `"` are normalized to `T*` + `TJ` first.
2. **Paints an opaque box** over the removed extent, in the same content stream, after the text, so the rendered page shows that something was removed.
3. **Recurses into Form XObjects.**  A shared XObject redacted for one page is redacted for every page that uses it; the report says so.  That is conservative, not a bug.

**Everything outside the content streams**, cleaned by default because each is a known leak path:

| Location | Default |
|---|---|
| `/Info` dictionary and XMP metadata | removed entirely |
| Annotations whose rect meets a target, or whose `/Contents`, `/URI`, or appearance stream matches | removed |
| AcroForm field values that match | value removed |
| Outline (bookmark) titles that match | entry removed |
| Embedded files, JavaScript, `/OpenAction` | removed |
| Images meeting an area target | policy: `remove` or `box`; there is no silent default |

**Output is a full rewrite.**  Unreferenced objects are pruned (`prune_objects`), and no incremental update is ever appended, since an appended revision leaves the old objects in the file.  Test explicitly that a secret present only in an _earlier_ revision of an incrementally saved input is absent from the output.

**`RedactReport`** counts removals per target kind, page, and XObject, and never contains matched text.  Its consumer’s stdout goes to an AI.

## The Fail-Closed Contract

Redaction is a claim of absence, so `positive-evidence-of-absence.md` governs it: **anything the engine cannot read is `Unknown`, and `Unknown` refuses the whole operation.**  `redact_document` returns an error naming each Unknown region.  It never writes a partially cleaned document.

Unknown includes:

- a font whose codes cannot be decoded to Unicode (no `ToUnicode`, unrecognized encoding), because an undecodable run can hide a match;
- a Type3 font whose glyph procedures cannot be mapped;
- a content stream that fails to parse;
- an image, when the caller supplied no image policy.

**Text drawn as vector outlines is invisible to any text search and cannot even be detected as text.**  This is the unfixable residual: document it loudly in the front end’s `--help`, and offer area targets and a rasterize mode as the answer.

Text render mode 3 (invisible, the usual OCR layer) is _not_ Unknown: its glyphs decode normally and are redacted like any other.

**Verification is part of the operation, not a courtesy.**  After rewriting, the front end:

1. re-extracts the output’s text and re-runs the matcher (every tier) — any hit is a tool failure: exit 1, and no output;
2. scans the raw decoded bytes of every stream and every string object in the output for each secret, in both literal and hex-string encodings.

The output is written to a temporary file and renamed into place only after both pass.

## Implementation Notes

- **Glyph decoding does not exist in medpdf today; pdf-dump has it** (`cmap.rs`, `encodings.rs`, `glyphlist.rs`, and the Reliable/Degraded verdict in `text.rs`).  Glyph _positioning_ exists in neither — it is pdf-dump `plan-0002`.
- **Open question: share or duplicate the decoder and positioning engine?**  Sharing (extract a crate both depend on) avoids two implementations of the most intricate code in either repo.  Duplicating keeps verification independent: if the redactor and the verifier share a decoder, a decoding bug that hides a match from one hides it from both.  A middle path: share the engine, but make step 2 of verification (the raw-byte scan) and a differential test against poppler’s `pdftotext` carry the independence.  Decide before implementation, and record why.
- **Which front end?**  The lean is that `pii-redact` accepts PDF input and calls this module, so one secrets file, one set of tiers, and one hold/review workflow cover CSV, OFX, text, and PDF alike, and pdf-maker stays a layout tool.  The alternative is a `pdf-maker --redact` flag.  Whichever it is, `pdf-maker --draw-rect`’s `--help` should gain one line saying a rectangle is not a redaction.
- **Rasterize mode** (later): render each page to an image, apply the boxes, and rebuild an image-only PDF.  It needs a renderer, which means a subprocess (`pdftoppm` or `mutool`, as `pdf-test-visual` already uses) — MuPDF is AGPL and cannot be linked into this MIT/Apache crate.  The paranoid answer to vector-outlined text.
- **Prior art:** MuPDF’s `apply_redactions` is the behavioral reference for glyph removal with position preservation.  Acrobat Pro’s Redact tool is the reference for the “sanitize document” metadata sweep.
- **Fixtures are synthetic, always.**  Real statements are the one input this feature must never show an AI.  Build fixtures with pdf-maker and hand-assembled lopdf documents: a secret split across `TJ` elements, one glyph per `Tj`, inside a shared Form XObject, in a CID font with `ToUnicode`, in `/Info`, in XMP, in an annotation `/URI`, and in an earlier incremental revision.
- **Tests assert on bytes, not exit codes** — the secret is absent from the output file in every encoding, and every glyph after a removal keeps its position, compared through pdf-dump `--text --layout --json` spans.  Confirm each test fails with the removal step disabled.

## Why Not a Workaround

The workaround available today — a black `--draw-rect` over the number — is the exact failure this plan exists to prevent, and it _looks_ correct, which is what makes it dangerous.  Extracting text and redacting that (pii-redact’s text path) covers many uses, but not the ones that need the PDF itself: a document to share, or a page an AI must see as laid out.  Glyph removal with position preservation, metadata sanitization, and fail-closed verification is library work, and it belongs beside the other content-stream operations medpdf already owns.
