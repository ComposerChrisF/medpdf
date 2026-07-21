# Bug Report: consolidated documentation drift — READMEs, feature plans, CLAUDE.md, rustdoc

**Severity:** Medium (published-crate documentation; the flagship examples fail; several claims are false)
**Component:** documentation only — `README.md`, `medpdf/README.md`, `medpdf-image/README.md`, `CLAUDE.md`, `feature-plan-type0-subsetting.md`, `feature-plan-unicode-text.md`, `medpdf/src/pdf_encryption.rs` rustdoc, `medpdf/src/pdf_watermark.rs` rustdoc.
**Category:** SPEC BUG — in every item below the **documentation is the defective artifact; the code is correct and must not be changed to match the docs.**  (Exception: item 2 offers an optional code alternative.)
**Verified:** 2026-07-16 deep review — every item confirmed by compile checks, live runs, or direct comparison with the code (spec-drift subagent, plus orchestrator checks).

One fixer session can clear this whole report; the items are grouped by file.

## 1.  Both Quick Start examples fail at runtime (root `README.md:21-38`, `medpdf/README.md:28-54`)

The examples build a fresh `Document::with_version("1.5")` and immediately call `copy_page` / `create_blank_page`.  Both functions require an existing `/Root` catalog with a `/Pages` node (`pdf_copy_page.rs:47`, `pdf_blank_page.rs:23`); a fresh lopdf document has neither.  Confirmed: the example fails with `PDF error: missing required dictionary key "Root"`.  Every test fixture in the repo builds the catalog+Pages skeleton first; the README skips it.  **Fix:** add the ~10-line skeleton to both examples.  (Optional follow-up feature, not part of this report: a `medpdf::new_document()` helper.)

## 2. `medpdf/README.md:239-243` “PDF Key Constants” example does not compile

It imports `KEY_PAGES` and `KEY_FONT`, which are `pub(crate)` (`pdf_helpers.rs:9,15`).  Public constants are exactly `KEY_RESOURCES`, `KEY_CONTENTS`, `KEY_EXTGSTATE`, `KEY_XOBJECT` (re-exported in `lib.rs:38-42`).  Compile check fails with E0603.  **Fix:** list only the four public constants — or, if Chris prefers, make `KEY_PAGES`/`KEY_FONT` public; the README as written is the false claim.

## 3.  Stale version pins in all three READMEs

- Root `README.md:17-18`: `medpdf = "0.10.0"`, `medpdf-image = "0.4.1"`.
- `medpdf/README.md:23`: `medpdf = "0.10.0"`.
- `medpdf-image/README.md:21,24`: `medpdf-image = "0.2.2"` (twice).

True versions: medpdf **0.11.0**, medpdf-image **0.4.3**.  Commit `17d161c` fixed exactly this once; it re-drifted.  **Fix:** update, and consider caret-style pins (`"0.11"`) to reduce future churn; a release-checklist line item would stop the recurrence.

## 4. `medpdf-image/README.md:29-52` Quick Start is written against a long-gone API — five compile errors

`draw_image` does not exist (entry point is `add_image(doc, page_id, params)`, params **by value**, `lib.rs:517`); the struct literal omits the required `image_data` field; `width`/`height` are `f32`, not `Option<f32>`; `max_dpi` is `f32`, not integer.  Also stale: “Fit modes … when both width and height are specified” (both are now mandatory), and the Features list omits the entire `recompress` module (v0.4.0) and the `svg` module’s public API.  **Fix:** rewrite around `DrawImageParams::new(image_data, x, y, w, h)` + builder methods + `add_image`; mention `recompress` and `svg`.

## 5. `medpdf/README.md:247` claims `deep_copy_object()`/`deep_copy_object_by_id()` are `pub(crate)`

They are `pub` and re-exported (`pdf_helpers.rs:128,149`, `lib.rs:38-42`).  **Fix:** reword to “public helpers (re-exported), used internally by …”.

## 6. `medpdf/README.md:64-73` error table omits `UnrepresentableText`

The variant (`error.rs:19-22`, added v0.11.0) is precisely the one consumers must handle for Unicode text.  **Fix:** add the row; note the enum is `#[non_exhaustive]`.

## 7. `CLAUDE.md` structure listing omits `pdf_subset.rs` and medpdf-image’s `recompress.rs`/`svg.rs`

The workspace tree and module-responsibility table predate those modules.  **Fix:** add the rows.

## 8. `feature-plan-type0-subsetting.md:5` calls the composite path “currently uncommitted WIP”

It landed in `cf7fc76` (v0.11.0, 2026-07-15).  The plan’s technical content is still valid.  **Fix:** replace the WIP framing with “landed in v0.11.0” (same banner style `feature-plan-place-page.md` received in `17d161c`).

## 9. `feature-plan-unicode-text.md` still describes the Unicode bug as open

The feature shipped in v0.11.0.  **Fix:** add a “Status: Implemented in v0.11.0” banner, noting consumer (pdf-maker / pdf-orchestrator) pass-through status if not yet picked up.

## 10. `pdf_encryption.rs:41` rustdoc: “Defaults to AES-256 with all permissions granted”

Commit `1b2dc09` deliberately changed the default to `Aes128` (lopdf AES-256 corruption bug — re-verified still present in lopdf 0.42: AES-256 output rasterizes blank) but missed this sentence.  The “all permissions” half is correct (`lopdf::Permissions::default()` is `all()`).  **Fix:** “Defaults to AES-128 (see `EncryptionAlgorithm`; AES-256 is avoided until the lopdf corruption bug is fixed) with all permissions granted.”

## 11. `EmbeddedFontCache` docs are silent that the cache is document-scoped

Keying is `(Arc::as_ptr, EncodingKind)` with no document identity (`pdf_watermark.rs:40-78`).  Reusing one cache across two `Document`s takes the cache-hit path and registers the first document’s `ObjectId` into the second document’s resources — a dangling reference, silently.  **Fix:** document “one cache per `Document`; never reuse across documents” on the type (a debug assertion or document-identity key is an optional hardening follow-up).

## Why these fixes address the bug

Every item makes a written claim match verified behavior, with the code (which tests pin) as the source of truth.  None of the fixes touch runtime behavior, so they can land as one doc-only commit.
