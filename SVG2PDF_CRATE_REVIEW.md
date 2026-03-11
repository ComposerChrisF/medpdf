# `svg2pdf` Crate Review: Security, Licensing, and Maturity

**Date:** 2026-02-17
**Crate version reviewed:** 0.13.0 (released March 4, 2025)
**Repository:** [github.com/typst/svg2pdf](https://github.com/typst/svg2pdf)
**Maintainer:** Typst GmbH (Laurenz Stampfl, Martin Haug)

---

## Executive Summary

| Area | Rating | Key Finding |
|------|--------|-------------|
| **Licensing** | Clean | All permissive (MIT/Apache-2.0/BSD-3-Clause). No copyleft. |
| **Security** | Low-Medium risk | No known CVEs. No fuzz testing is the biggest gap. |
| **Maturity** | Good, with caveats | 677K downloads, solid SVG 1.1 coverage. Uncertain long-term future post-Typst migration to `krilla`. |

**Bottom line:** Safe to adopt for near-term use. The licensing is clean, the security posture is reasonable, and the functionality is solid. The primary risk is long-term maintenance given Typst's migration to their newer `krilla` PDF backend.

---

## 1. Licensing

### svg2pdf License

**MIT OR Apache-2.0** (dual-licensed). Both `LICENSE-MIT` and `LICENSE-APACHE` files are present in the repository. Users may choose whichever license suits them.

### Dependency License Summary

| License | Crates |
|---------|--------|
| MIT OR Apache-2.0 | svg2pdf, pdf-writer, usvg (v0.45+), resvg (v0.45+), once_cell, log, ttf-parser, subsetter, siphasher, image, roxmltree, simplecss |
| MIT | fontdb, rustybuzz |
| BSD-3-Clause | tiny-skia, tiny-skia-path |
| MIT OR Zlib OR Apache-2.0 | miniz_oxide |

### Key Dependency Licenses

| Crate | License | Notes |
|-------|---------|-------|
| pdf-writer | MIT OR Apache-2.0 | Also by Typst |
| usvg 0.46 | Apache-2.0 OR MIT | Relicensed from MPL-2.0 in v0.45 |
| resvg 0.46 | Apache-2.0 OR MIT | Relicensed from MPL-2.0 in v0.45 |
| tiny-skia | BSD-3-Clause | Matches Skia's license |
| fontdb | MIT | Font database |
| ttf-parser | MIT OR Apache-2.0 | TrueType/OpenType parser |
| subsetter | MIT OR Apache-2.0 | Font subsetting, also by Typst |
| image | MIT OR Apache-2.0 | Image decoding |

### Copyleft Status

**None.** Zero copyleft (GPL/LGPL/AGPL/MPL) licenses in the current dependency tree.

**Historical note:** Prior to v0.45 (February 2025), resvg/usvg were licensed under MPL-2.0 (weak copyleft). The Linebender project relicensed them to Apache-2.0 OR MIT. Current versions used by svg2pdf (0.46) are fully permissive.

### Commercial Use

**No restrictions.** All licenses (MIT, Apache-2.0, BSD-3-Clause, Zlib) explicitly permit commercial use without royalty.

### Distribution Obligations

When distributing binaries that include svg2pdf:
1. Retain BSD-3-Clause copyright notice for tiny-skia
2. Retain MIT/Apache-2.0 notices as required
3. Do not use contributor names for endorsement (BSD "no endorsement" clause)

These are standard attribution requirements, trivially satisfied by including LICENSE files.

### Patent Concerns

**Low risk.** Both SVG and PDF are open international standards with royalty-free patent commitments. Apache-2.0 provides an explicit patent grant. No known patent claims against any crate in the tree.

### Bundled Fonts

**None.** svg2pdf does not bundle any fonts or font data. Fonts are discovered at runtime via `fontdb`. Note: when embedding system fonts into PDF output, the license of those individual fonts applies to the output PDF (a runtime concern, not a dependency concern).

---

## 2. Security

### Unsafe Code

| Crate | Unsafe Status | Notes |
|-------|--------------|-------|
| svg2pdf | **None** | No unsafe blocks in source |
| pdf-writer | `#![forbid(unsafe_code)]` | Explicitly forbidden |
| ttf-parser | `#![forbid(unsafe_code)]` | Zero-allocation, no panics, depth-limited |
| subsetter | `#![forbid(unsafe_code)]` | Minimal deps |
| roxmltree | `#![forbid(unsafe_code)]` | No panics policy |
| tiny-skia | **Minimal** | Only SIMD intrinsics and `bytemuck::Pod` |
| usvg | **Limited** | Font memory mapping only |
| fontdb | **Limited** | Memory-mapped files (inherently requires unsafe) |
| image | **Contains unsafe** | Performance-critical pixel manipulation |

**Assessment:** The direct svg2pdf code is safe Rust. Unsafe in dependencies is minimal and appropriate.

### Known Vulnerabilities

- **No RustSec advisories** for svg2pdf, usvg, resvg, tiny-skia, ttf-parser, or fontdb
- **No CVEs** filed against the Rust svg2pdf crate
- **Recommendation:** Run `cargo audit` against the specific `Cargo.lock` after adding the dependency

### XML/SVG Parsing Protections

| Attack Vector | Status | Mechanism |
|--------------|--------|-----------|
| XML Entity Expansion (Billion Laughs) | **Mitigated** | roxmltree: DTD disabled by default, 10-level depth limit, 255 refs/entity limit |
| XML External Entity Injection (XXE) | **Mitigated** | roxmltree: external entities not resolved by default, no custom resolver configured |
| Deeply nested SVG elements | **Partially mitigated** | usvg enforces max nesting depth |
| Huge/complex paths | **Partially mitigated** | tiny-skia operates on bounded pixel buffers, but no CPU time limits |
| Image bombs | **Partially mitigated** | image crate has configurable limits; unclear if svg2pdf sets strict ones |
| Processing timeouts | **Not implemented** | No timeout mechanism in svg2pdf |

### Supply Chain

- **Maintainer:** Typst GmbH, a funded German company
- **Contributors:** 12 contributors, 133 commits
- **crates.io owners:** reknih (Martin Haug), laurmaedje/LaurenzV
- **Downloads:** ~677,000 total
- **Risk:** LOW. Backed by a commercial organization that uses svg2pdf in their product.

### Dependency Tree Size

~180 transitive dependencies with all features enabled. Major contributors:
- `image` crate (many format-specific decoders)
- `resvg`/`tiny-skia` (2D rendering pipeline)

**Feature-gated reduction:** Disabling `image` and `filters` features dramatically reduces the tree.

### Fuzzing and Testing

- **Test suite:** 1,500+ SVG test files, visual regression testing via pdfium
- **Fuzz testing:** **None identified.** This is the most significant security gap. No fuzz targets, no OSS-Fuzz enrollment.
- **Recommendation:** Consider contributing fuzz targets upstream, or run your own fuzzing before processing untrusted SVGs.

### Memory Safety

No known memory safety vulnerabilities. The dependency chain is designed with safety as a priority — most core parsers use `#![forbid(unsafe_code)]`.

### Security Recommendations

1. **Run `cargo audit`** after adding the dependency
2. **Disable unused features** (`image`, `filters`) to reduce attack surface
3. **Implement resource limits** when processing untrusted SVGs: file size limits, processing timeouts, sandboxed execution
4. **Do not process untrusted SVGs without safeguards** — the lack of fuzz testing means edge-case panics are possible

---

## 3. Quality and Maturity

### Crate Metadata

| Field | Value |
|-------|-------|
| Version | 0.13.0 |
| Total downloads | ~677,000 |
| Recent downloads (90 days) | ~208,000 |
| GitHub stars | 382 |
| Open issues | 10 |
| License | MIT OR Apache-2.0 |

### Maintenance Status

- **Last release:** v0.13.0 (March 4, 2025)
- **Last commit:** January 24, 2026 (dependency bump)
- **Release cadence:** Roughly quarterly
- **Core team:** 1-2 active maintainers (Laurenz Stampfl primary, Martin Haug secondary)

### API Stability

**Pre-1.0.** Breaking changes occur roughly every 3-6 months:
- v0.11.0 (June 2024): Major API restructure — `convert_str` removed, `convert_tree` renamed to `to_pdf`
- v0.12.0 (September 2024): Conversion functions became fallible (`Result` return)

The API surface is small (2 main functions, 2 config structs, 1 error enum), so migrations are manageable despite frequent breaks.

### SVG Feature Coverage

**Supported:**
- Paths, basic shapes, complex fills
- Gradients (linear and radial)
- Patterns, clip paths, masks
- Transformations, viewbox
- Text (embedded as real text with font subsetting, since v0.11.0)
- Raster images
- Nested SVGs
- Filters (rasterized via tiny-skia, since v0.10.0)
- PDF/A output (since v0.12.0)

**Not supported:**
- Gradient `spreadMethod` attribute
- Color management for raster images
- SVG2-specific features
- `text-shadow`
- Multi-page SVGs
- Interactive/animated features (by design)

### Known Output Quality Issues

- Wrong gradient rendering with non-72 DPI
- Font fallback applies to whole text elements, not per-glyph
- Masked SVG rendering inconsistencies vs. browsers
- Filters are rasterized (lose vector quality at high zoom)

### The Typst/krilla Question

**Critical context:** As of April 2025, Typst switched its PDF backend from svg2pdf to [`krilla`](https://github.com/LaurenzV/krilla), a newer PDF creation library also by LaurenzV. `krilla` has its own SVG module (`krilla-svg`).

**Implications:**
- Typst was the primary consumer driving svg2pdf development
- svg2pdf remains independently usable (depends on usvg + pdf-writer, not Typst)
- Long-term maintenance trajectory is uncertain
- `krilla-svg` is a potential successor but is even newer (v0.6.0)

### Alternative Crates

| Crate | Approach | Trade-off |
|-------|----------|-----------|
| **svg2pdf** | Direct SVG→PDF via usvg + pdf-writer | Best current option; uncertain future |
| **krilla + krilla-svg** | High-level PDF creation with SVG support | Newer, Typst's chosen path; requires different PDF model (not lopdf) |
| **printpdf** | General PDF with SVG feature flag | Uses svg2pdf under the hood |
| **cairo-rs + librsvg** | System library bindings | Mature but requires C libraries |
| **Rasterize + embed** (resvg) | Render to pixels, embed as image | Simple but loses vector quality |

### Feature Flags for Minimal Footprint

```toml
# Minimal — text-only SVGs, no filters, no images
svg2pdf = { version = "0.13", default-features = false, features = ["text"] }

# No filter rasterization, but images allowed
svg2pdf = { version = "0.13", default-features = false, features = ["text", "image"] }

# All features (default)
svg2pdf = { version = "0.13" }
```

---

## 4. Integration Considerations for medpdf

### Compatibility with lopdf

svg2pdf uses `pdf-writer` (not `lopdf`) for PDF generation. Integration paths:

1. **Convert SVG to standalone PDF bytes via `to_pdf()`, then load with lopdf** — simplest approach, treat SVG-derived PDF as just another input document
2. **Use `to_chunk()` to get a PDF chunk, write to bytes, load with lopdf** — more control over page options
3. **Use SVG PDF as an overlay source** — convert SVG to PDF, then overlay onto target pages using existing `pdf_overlay` infrastructure

Option 1 is recommended: it requires no changes to medpdf's core PDF model and treats SVG as a pre-processing step.

### Dependency Weight

With all features: ~180 transitive deps, significant binary size increase (tiny-skia, image crate).
With minimal features (`text` only): substantially reduced.

Consider making SVG support a feature flag in medpdf to keep the default build lean.

---

## 5. Risk Summary and Recommendation

### Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Long-term maintenance (post-krilla) | Medium | Pin version; monitor krilla-svg as alternative |
| No fuzz testing | Medium | Don't process untrusted SVGs without safeguards |
| Pre-1.0 API instability | Low | Small API surface makes migrations easy |
| Dependency tree size | Low | Use feature flags to minimize |
| Licensing | None | All permissive, no copyleft |
| Known vulnerabilities | None | No CVEs or RustSec advisories |

### Recommendation

**Proceed with adoption**, with the following caveats:

1. Pin the version and wrap the integration behind a feature flag
2. Treat SVG→PDF as a pre-processing step (convert to PDF bytes, then load with lopdf)
3. If processing untrusted SVGs, implement file size limits and processing timeouts
4. Monitor the krilla-svg crate as a potential future migration target
5. Run `cargo audit` regularly

---

## Sources

- [svg2pdf on crates.io](https://crates.io/crates/svg2pdf)
- [typst/svg2pdf GitHub](https://github.com/typst/svg2pdf)
- [svg2pdf docs.rs](https://docs.rs/svg2pdf/latest/svg2pdf/)
- [RustSec Advisory Database](https://rustsec.org/advisories/)
- [linebender/resvg GitHub](https://github.com/linebender/resvg)
- [linebender/tiny-skia GitHub](https://github.com/linebender/tiny-skia)
- [roxmltree GitHub](https://github.com/RazrFalcon/roxmltree)
- [harfbuzz/ttf-parser GitHub](https://github.com/harfbuzz/ttf-parser)
- [typst/pdf-writer GitHub](https://github.com/typst/pdf-writer)
- [typst/subsetter GitHub](https://github.com/typst/subsetter)
- [LaurenzV/krilla GitHub](https://github.com/LaurenzV/krilla)
- [Typst PR #5420: Switch to krilla](https://github.com/typst/typst/pull/5420)
- [Linebender resvg stewardship blog](https://linebender.org/blog/tmix-10/)
- [W3C SVG Patent Disclosures](https://www.w3.org/Graphics/SVG/Disclosures)
- [Adobe ISO 32000-1 Public Patent License](https://www.adobe.com/pdf/pdfs/ISO32000-1PublicPatentLicense.pdf)
- [PDF Association: Adobe patent resolution](https://pdfa.org/adobe-resolves-patent-questions-on-iso-32000/)
