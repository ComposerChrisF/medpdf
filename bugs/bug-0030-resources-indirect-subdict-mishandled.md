# Bug Report: `Reference`-valued resource sub-dictionaries — rename skipped, merge drops them or errors, collision check blind

**Severity:** High — silent wrong-font/missing-resource output on a common Acrobat file shape; loud failure on another valid shape
**Component:** `medpdf` — `src/pdf_overlay_helpers.rs:105` (`rename_resources_in_dict` skips non-inline sub-dicts), `:271-279` (`merge_resources_into_dest_page` source-side silent skip and dest-side `as_dict_mut()?` error), `:12-18` (`add_resource_keys` skips `Reference` values when collecting existing names)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced (orchestrator tests `t5a_overlay_dest_indirect_font_subdict_errors`, `t5b_overlay_source_indirect_font_subdict_silently_dropped`; page-ops subagent `overlay_indirect_font_subdict_dropped` plus a trace of the collision facet).

## Description

A Resources entry like `/Font 10 0 R` (sub-dictionary held as an indirect reference) is valid and common — Acrobat emits it routinely.  Three code paths only handle the inline-dictionary form:

1. **Rename pass:** `rename_resources_in_dict` iterates `if let Object::Dictionary(dict) = value` — a `Reference` value is silently skipped, so none of the source’s resource keys are renamed and `key_mapping` stays empty for that category.
2. **Merge, source side:** in `merge_resources_into_dest_page`, `dict.as_dict()` fails on a `Reference`, and the `else if` silently does nothing — the source’s fonts never reach the destination page.  Net effect (confirmed): the overlay’s content still says `/F1`, which now resolves to the **destination’s** `F1` — the stamp silently renders in the wrong font, or as a missing resource if the destination has no `F1`.
3. **Merge, dest side:** when the _destination’s_ sub-dict is a `Reference`, `dest_resources.get_mut(...)?.as_dict_mut()?` fails and `overlay_page` returns `Err` on a perfectly valid destination (confirmed).
4. **Collision check:** `add_resource_keys` also skips `Reference` values, so destination names inside a referenced sub-dict never enter `keys_used`, and a renamed overlay key can collide with and overwrite an existing destination resource.

`place_page` shares all of this via the same helpers.

## Reproduction (test-ready)

- Source shape: overlay page with `/Resources << /Font 10 0 R >>` where object 10 holds `{F1: Courier}`; destination has inline `{Font: {F1: Times}}`.  After `overlay_page`: assert the destination page’s font set contains a renamed Courier entry and the overlay content references it — fails today (fonts = `[F1→Times]` only, content unrenamed).
- Dest shape: destination `/Resources << /Font <ref> >>`; assert `overlay_page` returns `Ok` — fails today with a lopdf `Err`.

## Suggested fix

1. After deep-copying the source Resources, **normalize** every `Reference`-valued sub-dict to an inline dictionary (the referenced target is already a private destination-side copy, so dereference-and-inline is safe).  The existing rename/merge logic then works unchanged.
2. In `add_resource_keys`, dereference `Object::Reference` values via `doc.get_dictionary` before collecting keys.
3. In the merge’s dest side, dereference a `Reference` sub-dict to its target dictionary and merge into that.

## Why the fix addresses the bug

Inline versus indirect is representation, not meaning — normalizing the private source copy and dereferencing on the read paths makes every path see the same dictionaries a viewer resolves, without changing the rename/merge algorithm.

## Related note

`accumulate_dictionary_keys` also returns early for page-tree nodes lacking `/Type` (`pdf_overlay_helpers.rs:37-43`) — lenient-input files can slip resource names past the collision scan.  Consider treating a node with `/Kids` as a Pages node regardless of `/Type` while editing this code.
