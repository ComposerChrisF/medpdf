# Bug Report: SVG conversion splices raw compressed bytes into the content stream when decompression fails

**Severity:** Low-Medium — silent garbage rendering on a hard-to-trigger path; the fallback branch is unambiguously wrong when reached
**Component:** `medpdf-image` — `src/svg.rs:446-453` (`Err(_) => buf.extend_from_slice(&stream.content)`)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — branch behavior confirmed by reading; not triggered live (svg2pdf output rarely fails decompression).

## Description

When assembling the Form XObject’s content from the svg2pdf intermediate page, a stream whose `decompressed_content()` fails has its **raw compressed bytes** appended as if they were PDF operators.  The result is a garbled or blank SVG rendered silently.  This is the unreadable-treated-as-readable shape the portfolio rules flag: an error answer used as if it were data.

## Reproduction (test-ready)

Construct the failure directly: call the internal assembly with a stream whose `/Filter` is `FlateDecode` but whose content is not valid zlib; assert the conversion returns `Err` naming the object — today it succeeds and the output contains the compressed bytes.

## Suggested fix

Propagate the error: `return Err(...)` naming the object id and the decompression failure.

## Why the fix addresses the bug

A failed conversion becomes loud and diagnosable instead of producing a silently wrong page; there is no legitimate case where compressed bytes belong in a decoded content buffer.
