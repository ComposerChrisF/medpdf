# Bug Report: operations split across `/Contents` fragments are silently destroyed by per-fragment re-encode

**Severity:** Medium-High — silent corruption of valid PDFs (rare-but-legal input shape)
**Component:** `medpdf` — `src/pdf_overlay_helpers.rs:145` (`modify_content_stream` decodes each fragment separately)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced (page-ops subagent test `overlay_corrupts_split_operation`).

## Description

PDF 32000-1 §7.8.2: a page’s multiple content streams are concatenated, and the division between streams may fall at any lexical-token boundary — operands in one fragment and their operator in the next is **legal**.  `modify_content_stream` decodes each fragment independently; lopdf’s content parser silently discards trailing bytes that do not form a complete operation.

Confirmed: destination fragments `"q 2 0 0 2 10 20"` + `"cm BT /F1 12 Tf ET Q"` re-encode to `"q\nq"` + `"cm\nBT\n/F1 12 Tf\nET\nQ\nQ"` — the six `cm` operands vanish and a bare zero-operand `cm` remains (invalid).  Corollary: any mid-stream token lopdf cannot parse silently truncates everything after it in that fragment.

## Reproduction (test-ready)

1. Destination page with `/Contents` = array of the two fragments above.
2. `overlay_page` (or `place_page`) anything onto it.
3. Reconstruct the concatenated destination content; assert the `cm` operation still has six operands — fails today.

## Suggested fix

- **Destination side:** resolved by bug-0018’s fix (never re-encode destination content; use standalone wrapper streams).
- **Source side (rename genuinely requires re-encoding):** concatenate all source fragments (joined with a newline) into a single buffer, decode **once**, apply renames, and re-emit as a **single** stream that replaces the fragment list.

## Why the fix addresses the bug

Concatenation is exactly the semantic the spec defines for multiple `/Contents` streams — parsing the concatenation cannot observe a split that a renderer would not observe, so no operation can straddle a parse boundary.  Emitting one combined stream also removes the need to preserve fragment boundaries at all.
