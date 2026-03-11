# TODO: Open-Source & crates.io Publishing

## Context

medpdf is a shared library used by multiple tools (pdf-merger, pdf-orchestrator, and potentially others). It should live in its own repo so no single consumer "owns" it. pdf-merger is a standalone CLI tool that happens to use medpdf.

## Repo Structure

| Repo | Crates | Purpose |
|------|--------|---------|
| **`medpdf`** | medpdf, medpdf-image, pdf-test-visual | Shared PDF library ecosystem |
| **`pdf-merger`** | pdf-merger (CLI only) | Standalone tool, depends on medpdf via crates.io |

## Current Status: ~75% ready

`medpdf` already passes `cargo publish --dry-run`. No source code changes needed — this is entirely a metadata/packaging task.

### Already done
- [x] Dual license (MIT OR Apache-2.0) on all crates
- [x] Descriptions on medpdf and medpdf-image
- [x] LICENSE files in medpdf/ and pdf-merger/
- [x] README files for medpdf and pdf-merger
- [x] All external dependencies properly versioned
- [x] `cargo doc` builds cleanly with `-D warnings`
- [x] No sensitive data in the codebase
- [x] Good test coverage including visual regression tests

---

## Phase 1: Split the repo

*Fresh-start approach: pdf-merger gets a new repo, this repo becomes medpdf. Git history stays here since most commits touch both medpdf and pdf-merger.*

### Create the new pdf-merger repo
- [ ] Create `~/Chris/App/Rust/Pdf/pdf-merger-standalone/` (or final location)
- [ ] `git init`
- [ ] Copy from current repo: `pdf-merger/src/`, `pdf-merger/Cargo.toml`, `pdf-merger/README.md`, `pdf-merger/LICENSE-MIT`, `pdf-merger/LICENSE-APACHE`
- [ ] Restructure as a standalone crate (move src/, Cargo.toml to root — no workspace needed)
- [ ] Initial commit

### Clean up this repo (becomes medpdf)
- [ ] Remove `pdf-merger/` directory
- [ ] Remove `"pdf-merger"` from workspace members in root `Cargo.toml`
- [ ] Clean up root-level files that are pdf-merger-specific (if any)
- [ ] Commit the removal

---

## Phase 2: Prepare the medpdf repo for publishing

### Blocking (required for `cargo publish`)

- [ ] Add version to medpdf-image's path dep
  - `medpdf-image/Cargo.toml`: `medpdf = { path = "../medpdf" }` → `medpdf = { version = "0.9.2", path = "../medpdf" }`
- [ ] Mark pdf-test-visual as `publish = false`
  - `pdf-test-visual/Cargo.toml`: add `publish = false`

### Strongly recommended

- [ ] Add `repository` to medpdf and medpdf-image Cargo.toml files
- [ ] Copy LICENSE-MIT and LICENSE-APACHE into medpdf-image/
- [ ] Write README.md for medpdf-image (short: description, usage, link to medpdf)
  - Also add `readme = "README.md"` to medpdf-image/Cargo.toml
- [ ] Add `keywords` and `categories` to medpdf and medpdf-image
  - medpdf: `keywords = ["pdf", "lopdf", "merge", "watermark", "overlay"]`, `categories = ["multimedia::encoding"]`
  - medpdf-image: `keywords = ["pdf", "image", "embed", "png", "jpeg"]`, `categories = ["multimedia::encoding"]`
- [ ] Write root README.md for the GitHub landing page (workspace overview)

### Publish

```bash
cargo publish -p medpdf
cargo publish -p medpdf-image
```

---

## Phase 3: Prepare the pdf-merger repo for publishing

*Do this after medpdf is published on crates.io.*

### Blocking (required for `cargo publish`)

- [ ] Replace path deps with crates.io versions
  - `medpdf = "0.9.2"` and `medpdf-image = "0.2.2"`
- [ ] Add `description` to Cargo.toml
  - `description = "CLI tool for merging, watermarking, and manipulating PDF files"`

### Strongly recommended

- [ ] Add `repository` pointing to the pdf-merger GitHub repo
- [ ] Add `keywords` and `categories`
  - `keywords = ["pdf", "merge", "watermark", "cli"]`, `categories = ["command-line-utilities"]`

### Publish

```bash
cargo publish
```

---

## Nice-to-have (either repo, not blocking)

- [ ] CHANGELOG.md — version history
- [ ] GitHub Actions CI — automated testing on push/PR
- [ ] `rust-version = "1.85"` — edition 2024 requires Rust 1.85+; declaring MSRV helps users
- [ ] CONTRIBUTING.md — guidelines for external contributors
- [ ] `examples/` directory — standalone runnable examples

---

## Post-publish features

- [ ] Add `--draw-svg` CLI command (SVG insertion)
  - medpdf-image already supports SVG via the `svg` feature flag (`svg2pdf` + `usvg`)
  - Exposes: `add_svg`, `load_svg`, `load_svg_bytes`, `load_svg_str`, `DrawSvgParams`, `SvgOptions`
  - pdf-merger needs to: enable `medpdf-image/svg` in Cargo.toml, add `--draw-svg` arg (similar to `--draw-image`), parse `DrawSvgSpec` with `FromStr`

- [ ] Booklet imposition (`--booklet`)
  - Arrange pages for saddle-stitch booklet printing (e.g., letter pages onto tabloid sheets)
  - Automatic page ordering for fold-and-staple (e.g., for 8 pages: sheet 1 front = pages 8,1; sheet 1 back = pages 2,7; etc.)
  - Special handling for back cover page(s) (e.g., keep back cover in position, insert blanks as needed)
  - Likely needs medpdf support for page scaling/placement onto a larger target page
  - Applies to pdf-merger and pdf-orchestrator

- [ ] N-up page layouts (`--nup`)
  - Lay out multiple input pages onto a single output page (starting with 2-up)
  - Support for replicating a single page to fill all slots (e.g., 2-up or 4-up flyers from one page)
  - Configurable output page size, gutters/margins, and page ordering (left-to-right, top-to-bottom)
  - Likely needs medpdf support for page scaling/placement (shared with booklet)
  - Applies to pdf-merger and pdf-orchestrator
