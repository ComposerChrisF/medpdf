# Bug Report: overlay re-encode writes content streams with a stale `/Length`

**Severity:** High — silent data loss.  Overlaid (and, by the same code path,
imposed / placed) page content is written with an incorrect `/Length`, making it
unrecoverable by spec-compliant readers.  The output even fails to round-trip
through medpdf itself.

**Component:** `medpdf` — `src/pdf_overlay_helpers.rs::modify_content_stream`

**Status:** Root-caused, one-line fix proposed below.

---

## Summary

When `modify_content_stream` rewrites a page’s content streams (renaming source
resource keys such as `/F7` → `/F7_o` and wrapping the operators in a `q` … `Q`
pair), it re-encodes the operator list and assigns the new bytes **directly to
the lopdf `Stream::content` field**.  That raw assignment does not update the
stream dictionary’s `/Length`.  Because the re-encoded body differs in length
from the original, `/Length` is now wrong.  lopdf’s reader trusts `/Length`:
it reads that many bytes, fails to find `endstream` where expected, and falls
back to storing the object as a bare dictionary — the stream body is discarded.
The overlaid content silently disappears.

## Affected code

`src/pdf_overlay_helpers.rs`, in `modify_content_stream` (around line 191):

```rust
content_stream.content = content.encode()?;   // <-- /Length is NOT updated
content_stream.compress()?;                   // tiny streams aren't compressed; /Length stays stale
```

`lopdf::Stream::content` is a `pub content: Vec<u8>` field, so this compiles and
bypasses the dictionary update that the proper setter performs.  For contrast,
lopdf’s `Stream::set_content` (verified in lopdf 0.39.0, `src/object.rs:643`)
is:

```rust
pub fn set_content(&mut self, content: Vec<u8>) {
    self.content = content;
    self.dict.set("Length", self.content.len() as i64);
}
```

The subsequent `compress()` declines to compress these small streams (the output
is left uncompressed) and does not repair `/Length`.

## Reproduction

medpdf is a library; the simplest end-to-end reproduction is through pdf-maker,
which calls the overlay path:

```
pdf-maker -o base.pdf --blank-page letter
pdf-maker -o stamp.pdf --blank-page letter \
  --watermark "text=FORMXTEXT,font=@Helvetica,size=36,x=2,y=5,units=in,color=black"
pdf-maker -o combined.pdf base.pdf all \
  --overlay "file=stamp.pdf,src_page=1,target_pages=all"

# Overlaid text is unreadable downstream:
pdf-dump combined.pdf --text          # => no FORMXTEXT, "not a stream object" warnings

# medpdf can't even round-trip its own output — the overlay content is lost:
pdf-maker -o rt.pdf combined.pdf all
grep -c FORMXTEXT rt.pdf              # => 0
```

A focused medpdf unit reproduction (recommended as the regression test): overlay
a page that draws text, save to a buffer, reload with `Document::load_mem`, and
assert the overlaid text operators survive and `/Length == content.len()` for
every uncompressed content stream.

## Evidence (combined.pdf)

The destination page’s `/Contents` becomes a five-fragment array
(`[13 0 R 7 0 R 8 0 R 9 0 R 10 0 R]`).  Object 10 in the raw file is a valid,
ordinary content stream:

```
10 0 obj
<</Length 54>>stream
q
0 0 0 rg
BT
/F7_o 36 Tf
144 360 Td
(FORMXTEXT) Tj
ET
Q
Q
endstream
endobj
```

…but its declared `/Length` is **54** while the actual body is **58–59 bytes**
(58 excluding the trailing newline before `endstream`).  The shortfall equals
the bytes the re-encode added: the `/F7` → `/F7_o` rename (`+2`) plus the
inserted `q` / `Q` wrapper.  A length-based reader therefore mis-parses it.
Object 13 shows the same class of mismatch: declared `/Length 0`, actual body
`q\nQ\n`.  Fragments 8 and 9, whose lengths happen to match, parse fine — which
is why only some of the overlay is lost.

## Root cause

A stream body was replaced via the public `content` field without re-syncing the
`/Length` entry that lopdf’s reader depends on.  Nothing later in the pipeline
recomputes `/Length`: `compress()` no-ops on these small streams, and lopdf’s
writer serializes the stale `/Length` verbatim.

## Fix

Replace the raw field assignment with the length-syncing setter:

```rust
content_stream.set_content(content.encode()?);  // sets content AND /Length
content_stream.compress()?;
```

(If a borrow conflict makes that awkward, set the length explicitly:
`let bytes = content.encode()?; let n = bytes.len(); content_stream.content = bytes; content_stream.dict.set("Length", n as i64);`.)

## Audit: are there similar bugs elsewhere in medpdf?

I scanned the crate (`medpdf/src`, `medpdf-image/src`):

- **Direct `Stream::content` field assignments:** exactly one — the line above.
  No other site mutates `.content` directly.
- **`Stream { content: … }` struct literals:** none.
- **`set_content` usages:** none anywhere (the project has never used it).
- **`Stream::new` usages:** the rest of the code (`pdf_blank_page`,
  `pdf_helpers`, `pdf_overlay`, `pdf_place_page`, `pdf_subset`, `pdf_watermark`)
  builds streams via `Stream::new`, which sets `/Length` correctly at
  construction.

So this is an isolated deviation from the established `Stream::new` pattern,
not a widespread class — but it is invisible to the type system, so it could
recur the next time someone replaces a stream body in place.

## Make the bug impossible (API / process hardening)

Two complementary measures, in order of strength:

1. **Pre-save `/Length` normalization (bulletproof).**  Add a pass that runs
   immediately before every save: for each `Object::Stream`, set
   `dict["/Length"] = content.len()` (after any compression).  This guarantees a
   correct `/Length` on output regardless of how the body was set, so even a
   future raw `.content =` mistake cannot ship a malformed file.  It is cheap and
   purely defensive.  (Consider also a `debug_assert!` form that _panics_ in
   debug/test builds on any `/Length` mismatch, so the mistake is caught at its
   source rather than silently corrected.)

2. **A single content-setter helper + a guard test.**  Provide a small
   `medpdf` helper, e.g. `set_stream_content(stream: &mut Stream, bytes: Vec<u8>)`
   that delegates to `lopdf::Stream::set_content`, and route every body
   replacement through it.  Back it with a CI/unit grep test that fails if any
   `\.content\s*=` assignment or `Stream\s*\{` literal appears in `src/`
   (lopdf exposes `content` as a public field, so clippy `disallowed-methods`
   cannot catch field access — a source-grep test is the practical enforcement).

Measure 1 alone closes the data-loss hole for good; measure 2 keeps the code
honest and discoverable.

## Not the `save_modern()` + encryption bug

This is **distinct** from the known lopdf `save_modern()` + encryption issue
(lopdf #479, guarded by pdf-maker’s `tests/lopdf_save_modern_bug.rs`, where an
object stream is created after encryption and left unencrypted).  This bug
reproduces on plain, unencrypted output and is entirely a medpdf-side `/Length`
synchronization mistake.

## Test plan

- Regression unit test in medpdf: overlay a text-bearing page, save to memory,
  reload, assert (a) the overlaid text operators are present and (b) every
  uncompressed content stream satisfies `/Length == content.len()`.
- If measure 1 is adopted, add a test that deliberately sets a wrong `/Length`
  on a stream, runs the pre-save normalization, and asserts the saved file has
  the corrected length and reloads intact.
