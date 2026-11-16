# bug-0045: `resolve_inherited_attribute` stops at a `null` value, which the PDF spec treats as absent, and hides the value an ancestor further up holds

**Severity:** Low — wrong output on valid but uncommon input: a copied, overlaid, or watermarked page loses the fonts and images it inherits.  It needs a `/Pages` node (or the page itself) carrying an inheritable key with the value `null`, beneath an ancestor that carries a real one.

**Type:** Code bug in this repo (`medpdf/src/pdf_helpers.rs`).

**Found:** 15-Nov-2026, by the review of the bug-0042 fix (rust-reviewer F3, agreed by Fable); confirmed by `bugs/bug-0045/repro.rs` against the working tree.

## The Defect

`resolve_inherited_attribute` (`medpdf/src/pdf_helpers.rs`, the `/Parent` walk shared by copy_page, overlay, place, watermark, and `register_in_page_resources`) returns the first value it finds for the key:

```rust
if let Ok(value) = dict.get(key) {
    return Some(value.clone());
}
```

PDF 32000-1 § 7.3.9: a dictionary entry whose value is `null` is equivalent to an absent entry.  So a `/Resources null` on a `/Pages` node means that node has no `/Resources`, and inheritance continues to its parent.  The walk instead stops there and returns `Some(Null)`.  `get_page_resources`’s doc already promises “either an inline `Object::Dictionary` or an `Object::Reference`”, which a `Null` breaks.

## Reproduction

`bugs/bug-0045/repro.rs` (a test file: copy it into `medpdf/tests/` and run it with `--nocapture`).  A page under a middle `/Pages` node holding `/Resources null`, under a root `/Pages` node holding `/Resources << /Font << /F1 … >> >>`; the page’s content uses `/F1`.

```
resolve: Some(Null)
copied page /Resources: Ok(Null)
```

By the spec the page’s effective resources are the root’s, with `/F1`.  `copy_page` flattens `null` onto the copied page instead, and `/F1` no longer resolves.

Through `register_in_page_resources` (after the bug-0042 fix) the same input is an error, “Inherited /Resources not a Reference nor a Dictionary!”: loud, and correct once this bug is fixed, because a `null` will then never reach it.

## Fix Direction

In the walk, treat a `null` value as absent and keep climbing: `Ok(Object::Null)` falls through to the `/Parent` step.  One site; every caller inherits the fix.

## The Test That Pins It

The reproduction’s shape, asserting that `get_page_resources` returns the root’s dictionary and that `copy_page` puts `/F1` on the copied page.  It must fail with the fix reverted.
