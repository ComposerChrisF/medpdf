# Bug Report: inherited `/Resources` mishandled by overlay and place — original fonts disconnected, wrong fonts bound, valid input rejected

**Severity:** High — silent corruption on the destination side; silent wrong-font output or spurious errors on the source side
**Component:** `medpdf` — `src/pdf_overlay_helpers.rs:249` (`merge_resources_into_dest_page`, missing-Resources arm), `src/pdf_overlay.rs:40` (source `/Resources` required on the page dict), `src/pdf_place_page.rs:72-78` (missing source `/Resources` → empty dict, no tree walk)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — three facets reproduced (page-ops subagent tests `overlay_dest_inherited_resources_shadowed`, `place_page_source_inherited_resources`, `overlay_source_inherited_resources_errors`).

## Description

`Resources` is an inheritable page attribute; real documents put it on a `/Pages` ancestor.  Per PDF inheritance, a page-level `/Resources` **replaces** (does not merge with) the inherited one.  Three confirmed failure modes:

1. **Destination page with inherited Resources — silent shadowing.**  `merge_resources_into_dest_page` treats a missing page-level `/Resources` as “start from empty” (`Err(_) => Some(Dictionary::new())` at `pdf_overlay_helpers.rs:249`).  After overlay, the page has an own `/Resources` containing only the renamed overlay keys (e.g. `["F1_o"]`).  That page-level dict now replaces the inherited one, so the page’s original `BT /F1 … Tj` no longer resolves — the pre-existing text loses its font in every conforming viewer.  The existing test `overlay_edge_tests.rs:557` masks this by asserting only `is_ok()`.
2. **Source page with inherited Resources — `place_page` binds content to wrong/missing resources.**  `place_page` substitutes an empty dict, so `key_mapping` stays empty and the placed content’s `/F1` is never renamed; it resolves against whatever the destination happens to call `F1` (confirmed: source Times-Roman text silently rendered with the destination’s Helvetica), or renders as missing-resource if the destination has no `F1`.
3. **Source page with inherited Resources — `overlay_page` errors.**  `overlay_page.get(KEY_RESOURCES)?` fails with `Err(LoPdf(DictKey("Resources")))` on a page any viewer renders fine.

## Reproduction (test-ready)

- Facet 1: destination page inherits `Font { F1: Times }` from its Pages node; overlay any page; assert the destination page’s resulting page-level `/Resources/Font` still contains `F1` → fails today (only `F1_o`).
- Facet 2: source page inherits `Font { F1: Times }`; `place_page` onto a dest whose own `F1` is Helvetica; assert the dest page’s font set gains a (renamed) Times entry and the placed content references it → fails today.
- Facet 3: same source shape through `overlay_page`; assert `Ok` → fails today.

## Suggested fix

Introduce one helper (shared with bug-0008): resolve a page’s effective `/Resources` by walking the `/Parent` chain (the same pattern as `get_page_media_box`, `pdf_helpers.rs:23-43`).  Then:

- In `merge_resources_into_dest_page`: when the destination page has no own `/Resources`, seed the new page-level dictionary with a clone of the **inherited** resources before merging the renamed overlay entries.  (`accumulate_dictionary_keys` already collects inherited names on Pages nodes, so renamed keys cannot collide with them.)
- In `overlay_page` and `place_page`: resolve the **source** page’s resources with the same walk instead of requiring/defaulting the page-dict key.

## Why the fix addresses the bug

Materializing the inherited dictionary onto the page is semantics-preserving for the page’s existing content — it is exactly what a viewer resolves — and it is the only way a page-level dict can coexist with PDF’s replace-not-merge inheritance rule.  On the source side, the walk yields the dictionary the source page actually renders with, so the rename+merge machinery then behaves identically to the own-Resources case.
