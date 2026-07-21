# Bug Report: page-tree `/Count` maintenance broken for nested trees in `delete_page`, `copy_page`, and `create_blank_page`

**Severity:** High — corrupt page tree written to disk for any document with intermediate `/Pages` nodes (Acrobat writes balanced trees for documents over ~31 pages)
**Component:** `medpdf` — `src/pdf_delete_page.rs:28-47`, `src/pdf_copy_page.rs:54-61`, `src/pdf_blank_page.rs:26-32`
**Category:** BOTH — the code is wrong per PDF 32000-1 §7.7.3.2 (`/Count` = number of **leaf** pages under the node, on **every** node), and the `delete_page` doc comment (“Updates the parent `/Pages` node’s `/Kids` array and `/Count`”) prescribes the insufficient behavior, so the doc must be corrected alongside the code.
**Verified:** 2026-07-16 deep review — three repros (orchestrator test `t4_delete_page_leaves_stale_ancestor_count`; page-ops subagent tests `delete_page_mixed_kids_parent_count`, `copy_page_dest_nested_count`).

## Description

Three sites share two flavors of the same invariant violation:

1. **`delete_page` updates only the direct parent.**  With Root{ PagesA{P1, P2}, P3 } (root `/Count` 3): deleting P1 decrements PagesA’s count but leaves the root’s `/Count` at 3 while only 2 pages remain.  Count-trusting readers (most viewers — page count, random access, tree descent) see a phantom page.
2. **`delete_page` and the add operations set `/Count = kids.len()`.**  `kids.len()` counts children, not leaf pages.  Confirmed: Root Kids = [PageX, PagesA(2 leaves)], Count 3 — deleting PageX sets root `/Count` to **1** though 2 pages remain (a page vanishes in Count-trusting readers).  Symmetrically, `copy_page` appending a 4th page to a nested 3-page dest sets root `/Count` to **3** (`kids.len()` = [PagesA, P3, new]).  `create_blank_page` has the identical statement.

medpdf’s own `get_pages()` walks Kids and ignores `/Count`, which is why the suite never noticed — the corruption is only visible to spec-conforming consumers of the saved file.

## Reproduction (test-ready)

Build the nested trees above with lopdf primitives; call `delete_page(doc, 1)` / `copy_page` / `create_blank_page`; assert every ancestor’s `/Count` equals the number of leaf pages beneath it (`doc.get_pages().len()` for the root).  All three fail today.

## Suggested fix

- **Delete:** after removing the kid from the direct parent’s `/Kids`, walk the `/Parent` chain from that parent to the root, decrementing each node’s `/Count` by 1.  Never assign `kids.len()`.
- **Add (`copy_page`, `create_blank_page`):** read the target `/Pages` node’s current `/Count` and add 1 (the appended kid is always a leaf `Page`); if the tree above the insertion point can be nested (it can — the caller supplies the dest doc), walk the `/Parent` chain upward incrementing, same as delete but +1.  In the current code the new page is always attached to the **root** Pages node, so the walk is trivially the root itself — but write it as the walk so a future nested attach stays correct.
- **Docs:** fix the `delete_page` doc comment to say all ancestor counts are updated.

## Why the fix addresses the bug

The operations add or remove exactly one leaf, so every ancestor’s leaf count changes by exactly ±1 — increment/decrement along the parent chain is correct for any tree shape without recomputation, and no site ever again equates children with leaves.
