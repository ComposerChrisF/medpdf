# TODO — medpdf workspace

## Current state (2026-07-16)

A deep bug hunt (2026-07-16) filed **37 verified bug reports**: `bugs/bug-0002` … `bugs/bug-0038`.  Every report is self-contained: description, test-ready reproduction, suggested fix, and why the fix is right.  IDs were assigned alphabetically by slug (per the portfolio convention); **priority lives in this file and in each report’s Severity line, not in the ID**.

**Read this file top to bottom — the ordering below is the work plan.**  Reports marked **[NEEDS CHRIS]** contain a spec-level decision; do NOT “just fix the code” for those — the report says exactly what is undecided and why.

## Step 0 — housekeeping (first commit, needs Chris’s go-ahead to commit)

- [x] Commit the 37 new bug reports (bug reports are committed when created, per the portfolio bug-reports rule).  _Done: commit `87f7bbf`._
- [x] In the same or a following commit: **delete `bugs/bug-0001-report-overlay-content-stream-stale-length.md`** — that bug was fixed in v0.10.2 (commit `5c25a85`, pinned by `tests/overlay_length_regression_tests.rs` and `tests/no_raw_stream_content_assignment.rs`); the report was renamed into `bugs/` by `f1a41c0` instead of being deleted.  Commit message should name bug-0001.

## Step 1 — decisions Chris must make (blocks the marked bugs only)

- [ ] **bug-0021 [NEEDS CHRIS]** — `parse_page_spec` silently filters/clamps out-of-range pages.  Document the contract (recommended, non-breaking) or change to error (breaking for pdf-maker/pdf-orchestrator)?
- [ ] **bug-0024 [NEEDS CHRIS]** — `place_page` (x, y) semantics for non-zero-origin MediaBox: compensate so the visible box lands at (x, y) (matches the feature plan; changes behavior + one pinned test), or keep user-space mapping and fix the three docs?
- [ ] **bug-0023 [NEEDS CHRIS]** — `place_page` and source `/Rotate`: honor it in the transform (recommended for imposition) or document it as caller responsibility?
- [ ] **bug-0013 [NEEDS CHRIS]** — image rotation pivot: README says anchor point, code rotates about box center.  Fix the README (recommended; pdf-maker ships on center) or change the code?

## Step 2 — doc-only and test-only fixes (no rulings needed; one session)

- [ ] **bug-0009** — consolidated docs drift: both failing Quick Starts, private KEY constants, stale versions (0.11.0 / 0.4.3), medpdf-image README rewrite, error-table row, CLAUDE.md modules, feature-plan status banners, AES-128 rustdoc, `EmbeddedFontCache` scope note.
- [ ] **bug-0002** — AES-256-named tests actually test AES-128; make the algorithm explicit.

## Step 3 — medpdf core fixes, in dependency order

Order matters here; later fixes reuse mechanisms and helpers from earlier ones.

1. [x] **bug-0007** (Critical, crash) — deep-copy cycle → stack-overflow abort.  Fix: reserve the dest ID in `copied_objects` **before** recursing.  Independent of everything else; do first.  _Done: `pdf_helpers.rs` two-phase copy + `tests/copy_page_cycle_regression.rs` (child-process, pins the fix)._
2. [x] **bug-0018** (High) — stop decode→re-encode of destination content in overlay/place; wrap with standalone `q`/`Q` streams instead; loud error (or correct emitter) for inline images in **source** content; add `LOPDF_INLINE_IMAGE_BUG.md` upstream-defect record at the repo root.  This builds the isolation mechanism that 3 and 6 reuse.  _Done: `isolate_dest_content_streams` (standalone wrappers, no re-encode) + `rename_source_content_streams` (rejects inline images loudly, operator-aware renaming); shared `count_q_balance` promoted to `pdf_helpers`; `LOPDF_INLINE_IMAGE_BUG.md` added; `tests/overlay_inline_image_regression.rs` (4 tests, corruption-detecting ones verified to fail on revert) + operator-aware unit tests._
3. [x] **bug-0019** (Medium-High) — source fragments must be decoded as one concatenated stream (dest side is already fixed by 0018).  _Done: `rename_source_content_streams` now concatenates all source fragments (newline-joined), decodes once, re-emits a single combined stream; `tests/overlay_split_operation_regression.rs` (overlay + place, verified to fail on per-fragment revert)._
4. [ ] **bug-0030** (High) — normalize `Reference`-valued resource sub-dicts after deep copy; deref in `add_resource_keys` and in the merge’s dest side.
5. [ ] **bug-0017** (High) — inherited `/Resources` handling in overlay (dest seed + source walk) and place (source walk).  Build one shared `resolve inherited attribute` helper — **bug-0008** consumes it too.
6. [ ] **bug-0025** (Medium-High) — `place_page` destination isolation, using 0018’s wrapper mechanism.
7. [ ] **bug-0008** (High) — `copy_page` materializes inherited MediaBox/Resources/CropBox/Rotate (uses the step-5 helper).
8. [ ] **bug-0020** (High) — `/Count` maintenance: ancestor walk on delete; +1 (never `kids.len()`) on copy/blank.
9. [ ] **bug-0016** (Low) — overlay from a contentless source page becomes a no-op.
10. [ ] **bug-0037** (High-when-triggered) — watermark font/GS key collision: collision-check with `find_unique_name`; plus the bloat notes in the report.

## Step 4 — font subsystem (independent of Step 3; can interleave)

1. [ ] **bug-0031** (High) — scale `/Widths` + FontDescriptor metrics to 1000/em glyph space (the composite `/W` code shows the formula).
2. [ ] **bug-0012** (High on macOS) — carry font-kit’s `font_index` through `FontPath`/`FontData`; public-API change, coordinate with pdf-maker.  Do before 0005 (both touch embedding plumbing).
3. [ ] **bug-0005** (Medium-High) — CFF/OTF: `/Type1` font-dict subtype, FontFile3 `/Subtype /OpenType`, no `Length1`; composite CFF → loud error (or CIDFontType0).
4. [ ] **bug-0032** (Medium) — simple path fails loudly on missing glyphs (mirror the composite arm); plus the lossy-`.notdef` width and control-char notes in the report.
5. [ ] **bug-0004** (Medium) — omit `/Encoding` for built-in Symbol/ZapfDingbats.
6. [ ] **bug-0010** (Medium) — symbol-font cmap lookups via the 0xF000 convention; fix the Webdings misclassification.
7. [ ] **bug-0011** (Low) — fallback width: chars, not bytes.
8. [ ] **bug-0038** (Low) — `get_font_widths` u8 overflow → usize arithmetic.
9. [ ] **bug-0034** (Low) — `head` directory checksum computed with adjustment zeroed.

## Step 5 — place_page semantics (after Step 1 rulings)

- [ ] **bug-0024** per ruling (origin compensation or doc fix) — remember `place_page_tests.rs:375` pins today’s behavior.
- [ ] **bug-0023** per ruling (`/Rotate` in transform or documented caller responsibility).
- [ ] **bug-0027** (Low-Medium) — clip with the transformed quad, not its AABB.
- [ ] **bug-0026** (Low) — validate x/y/rotation finiteness.

## Step 6 — medpdf-image

1. [ ] **bug-0029** (Critical for affected inputs) — skip recompression when `/DecodeParms` has Predictor 2 (or is unresolvable).
2. [ ] **bug-0028** (High) — skip recompression when `/Mask` present.
3. [ ] **bug-0033** (Medium) — per-axis downsample clamping for Stretch.
4. [ ] **bug-0035** (Medium) — transparency `/Group` on the SVG Form XObject.
5. [ ] **bug-0014** (Medium) — document the max-DPI JPEG re-encode; quality parameter defaulting to 85.
6. [ ] **bug-0003** (Medium-Low) — premultiplied-alpha resampling.
7. [ ] **bug-0015** (Low-Medium) — JPEG SOF validation: reject SOF3, non-8-bit precision, zero dimensions.
8. [ ] **bug-0036** (Low-Medium) — SVG decompression failure becomes a loud error.
9. [ ] **bug-0013** per ruling — rotation-pivot doc or code.
10. [ ] **bug-0006 [NEEDS FIXTURE]** — CMYK APP14 `/Decode`: obtain a real Adobe CMYK JPEG and verify before implementing.
11. [ ] **bug-0022** (Low, pdf-test-visual) — drain child pipes concurrently with the timeout wait.

## Standing instructions for whoever picks this up

- Every fix must land with a regression test that **fails when the fix is reverted** (several reports name the exact assertion).  For bug-0007 the repro must run in a child process — the failure is a process abort.
- Fixing a bug: commit any report amendments first, then delete the report (and its `bugs/bug-NNNN/` dir, if any) in the commit that lands the fix, naming the ID (portfolio bug-reports rule).
- Both crates are published on crates.io.  Batch behavior-changing fixes into a coherent release; bug-0012 changes public API (semver).  Audit pdf-maker (and pdf-orchestrator) call sites before changing `parse_page_spec` (0021) or `place_page` semantics (0023/0024).
- Code commits go through `/commit` (the dispatcher picks `commit-rust-workspace`); doc-only commits may be bare, but still push.
