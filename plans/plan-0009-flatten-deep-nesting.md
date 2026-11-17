# Plan: Flatten the One Block Nested More Than 6 Deep

## Problem

The workspace `clippy.toml` sets `excessive-nesting-threshold = 6` (house-lints v2, 16-Nov-2026), which turns on clippy’s `excessive_nesting`: a block more than 6 deep is flagged, counting an inline `mod`, the function body, and each `if`/`else`, loop, braced `match` arm, and closure inside it.  When the threshold was adopted, `cargo clippy --workspace --all-targets --all-features` flagged one site.  It carries a temporary `#[expect(clippy::excessive_nesting, reason = "plan-0009: …")]` so the lint is on everywhere else.  The code is correct; this is a refactor, not a bug.

| File | Line | Enclosing function | Depth | Flagged block |
|---|---|---|---|---|
| `medpdf/src/pdf_overlay.rs` | 379 | `tests::test_overlay_page_preserves_q_balance` | 7 (`mod tests`, fn body, `for item`, `if`, `if let Ok(content)`, `for op`, the `_ => {}` arm) | the empty catch-all arm of the `q`/`Q` counting `match` |

The `#[expect]` sits on that `_ => {}` arm, the narrowest node that takes an attribute.

## Proposed Change

Flatten the counting loop so no block is more than 6 deep.  The usual remedies, either of which suffices here:

- **A helper function**, e.g. `fn count_q_ops(content: &Content) -> (i32, i32)`, called from the `if let Ok(content)` branch; or
- **An iterator in place of the inner `for`/`match`**: count `q` and `Q` with `filter(..).count()` over `content.operations`, which removes the inner loop and the `match` altogether.

Remove the site’s `#[expect]` in the same change.  `cargo clippy --workspace --all-targets --all-features` must then show no `excessive_nesting` warning and no unfulfilled expectation.

## Implementation Notes

- Test code only (`#[cfg(test)]`); the library’s behavior is untouched, and the test must still assert the same `q`/`Q` balance.
- A site that is reworked has its `#[expect]` removed when it is reworked; the plan is implemented, and deleted, when no `plan-0009` expectation remains (`command grep -rn 'plan-0009' medpdf medpdf-image pdf-test-visual`).

## Why Not a Workaround

Raising the threshold or allowing the lint crate-wide would hide every future deep block to spare one test; the `#[expect]` already scopes the exception to this one arm, and is reported as unfulfilled the moment the arm is flattened without removing it.
