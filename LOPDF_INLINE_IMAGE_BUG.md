# lopdf 0.42 corrupts inline images on content-stream re-encode

This repository carries a workaround for a defect in the `lopdf` crate (present
through 0.42).  This file exists so the workaround has something durable to point
at: it must outlive any individual `bugs/bug-NNNN` report, and it is cited from
the source code.  (Same convention as pdf-orchestrator’s `LOPDF_SAVE_MODERN_BUG.md`.)

## The defect

`lopdf::content::Content` cannot round-trip an **inline image** (`BI … ID <data>
EI`) through `Content::decode` → `Content::encode`.  Two distinct failure modes,
depending on whether lopdf’s parser understands the image:

- **Parseable inline image** (e.g. `/CS /RGB`, uncompressed): `decode` yields a
  `BI` operation whose single operand is an `Object::Stream`.  `encode` writes
  operands through `Writer::write_object`, which serializes a stream as
  `<<dict>>stream\n…\nendstream` — emitted _before_ the `BI` operator.  That is
  not valid content-stream syntax, and a renderer breaks from that point onward.
- **Unparseable-to-lopdf inline image** (`/CS /G`, a legal colorspace
  abbreviation lopdf’s `image_data_stream` does not resolve; or any filtered
  image such as `/F /AHx` or `/Fl`): the parser skips ahead to `EI` and returns a
  `BI` operation with **no operands**.  The image data is silently dropped, and a
  bare orphan `BI` remains in the re-encoded stream.

Either way, a decode→encode round trip on a content stream that contains an
inline image corrupts or deletes that image, with no error raised.

## How medpdf compensates

The trigger is _re-encoding_ a content stream that holds an inline image.  medpdf
avoids that in two places (both in `src/pdf_overlay_helpers.rs`):

- **Destination content is never re-encoded.**  `isolate_dest_content_streams`
  adds isolation by prepending a standalone `q` stream and appending a standalone
  `Q` stream, leaving the page’s own content streams byte-for-byte untouched.  So
  merely overlaying or stamping a page can no longer damage its inline images —
  nor those of any other page that shares the same content streams by reference.
- **Source content, which must be re-encoded to rename its resources, is
  screened first.**  `rename_source_content_streams` detects any `BI` operation
  and returns a loud `MedpdfError` instead of silently mangling the image.  The
  message tells the caller to convert the inline image to an image XObject in the
  source PDF.  A diagnosable limitation beats silent corruption.

If a future lopdf release round-trips inline images correctly, the source-side
rejection can be relaxed (and this file, plus the check it guards, retired).  A
regression test would need to confirm the round trip before removing the guard.

## Upstream

Worth filing against lopdf if not already tracked: `Content::encode` should emit
an inline image as `BI <dict entries> ID <raw data> EI`, and `Content::decode`
should preserve enough to reconstruct it (the raw image bytes and the entry
dictionary), for every colorspace and filter — not only those its image decoder
happens to understand.

## Provenance

Surfaced 2026-07-16 (medpdf `bug-0018`), reproduced across `/CS /RGB`, `/CS /G`,
and filtered inline images, including corruption of a _destination_ page’s own
untouched inline image by an unrelated overlay.
