# bug-0042: `register_in_page_resources` gives a page that inherits `/Resources` a new, empty dictionary, hiding the inherited ones

**Severity:** Medium — the page’s own content loses its fonts, images, and graphics states after one new resource is registered.  No caller in pdf-maker reaches it today, because every page goes through `copy_page` first, but the function is public API.

**Type:** Code bug in this repo (`medpdf/src/pdf_helpers.rs`).  The same fault `bug-0017` fixed in the overlay and place paths, left in the shared helper.

**Found:** 14-Nov-2026, by a code-quality read of the repo; confirmed by calling the exported function on a one-page fixture.

## The Defect

`medpdf/src/pdf_helpers.rs:400–429`, `register_in_page_resources`.  When the page dictionary has no `/Resources` key of its own:

```rust
Err(_) => {
    let mut dict = dictionary! {};
    let r = handle_subdict_in_resources(&mut dict, resource_key, entry_key, obj_id)?;
    page_dict.set(KEY_RESOURCES, Object::Dictionary(dict));
    (r, None)
}
```

`/Resources` is inheritable (PDF 32000-1 § 7.7.3.4).  A page without its own entry may be using one on a `/Pages` ancestor.  The helper does not look there: it creates a dictionary holding only the new entry and sets it on the page, and a page’s own `/Resources` replaces the inherited one completely.  Everything the page’s content stream names from the inherited dictionary (`/F1`, an image `/XObject`, an `/ExtGState`) no longer resolves.

`get_page_resources` (`medpdf/src/pdf_helpers.rs:160`) already walks the `/Parent` chain, and the `bug-0017` fix uses it in `pdf_overlay` and `pdf_place_page` to put a private copy of the inherited dictionary on the page before changing it.  This helper predates it.

**Callers:** `pdf_watermark` (fonts and `ExtGState` for text watermarks, `medpdf/src/pdf_watermark.rs:257`, `:274`), `medpdf-image` (`medpdf-image/src/lib.rs:524`), and any external user of the exported function.

## Reproduction

`bugs/bug-0042/inherited.pdf` is a one-page document whose `/Resources << /Font << /F1 … >> >>` sit on the `/Pages` node, with a page content stream that uses `/F1`.  `bugs/bug-0042/repro.rs` loads it, registers a new font as `/F9` on page 1, and saves:

```
page's own /Resources now: <</Font <</F9 6 0 R>>>>
```

`pdf-dump registered.pdf --page 1 --json` then lists only `/F9` in the page’s resources; `/F1`, which the page’s text uses, no longer resolves.

Through pdf-maker the same input is fine (`--watermark` on it keeps `/F1` and adds `/F7`), because the merge’s `copy_page` puts the inherited attributes on the page first (`bug-0008`).

## Fix Direction

In the no-`/Resources` branch, start from the page’s effective resources (`get_page_resources`), materialized onto the page as a private copy as the `bug-0017` fix does, instead of an empty dictionary.  Only an absent `/Resources` anywhere up the chain starts empty.

## The Test That Pins It

A test that builds a page inheriting `/Resources` from its `/Pages` parent, calls `register_in_page_resources`, and asserts **that the page’s effective `/Font` still holds the inherited `/F1`** beside the new entry, and that the shared ancestor dictionary was not changed.  It must fail with the fix reverted.
