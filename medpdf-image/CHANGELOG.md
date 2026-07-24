# Changelog

All notable changes to the `medpdf-image` crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0/).

## [Unreleased]

## [0.4.5] - 2026-07-24
### Fixed
- bug-0033: `ImageFit::Stretch` downsampling computed a single scale from the
  higher-DPI axis and applied it to both, needlessly over-downsampling the
  already-compliant axis (e.g. a wide image into a short box lost several×
  resolution horizontally).  Each axis is now clamped independently;
  Contain/Cover (which share one DPI) are unchanged.
- bug-0035: the SVG Form XObject was emitted without a transparency `/Group`,
  so `DrawSvgParams::alpha` faded each painting operator separately —
  overlapping elements double-composited (darker overlaps, seams) rather than
  fading as a unit.  The form now declares `/Group << /S /Transparency >>`
  (carrying over any group svg2pdf declared); at alpha 1.0 rendering is
  unchanged.

## [0.4.4] - 2026-07-24
### Fixed
- bug-0029: image recompression ignored `/DecodeParms`, so a FlateDecode image using
  TIFF Predictor 2 (which lopdf hands back still horizontally differenced) was
  JPEG-encoded as if raw — total, permanent corruption reported as a successful
  optimization.  Recompression now consults `/DecodeParms` and only proceeds for
  no-predictor / Predictor 1 / PNG predictors 10-15; Predictor 2, any unknown value, or
  an unresolvable `/DecodeParms` is skipped.
- bug-0028: recompression skipped `/SMask` but not `/Mask`, so lossy JPEG re-encoding
  silently broke color-key (`/Mask` array) transparency.  Images carrying `/Mask` (array
  or reference) are now skipped, symmetric with the `/SMask` policy.

## [0.4.3] - 2026-06-29
### Changed
- Bump `lopdf` 0.39 → 0.42 (toolchain-wide coordinated bump); code-review
  hardening and added test coverage.

## [0.4.1] - 2026-03-16
### Added
- ICCBased colorspace support in image recompression.

## [0.4.0] - 2026-03-15
### Added
- Image recompression module — recompress embedded raster images.

## [0.2.2] - 2026-02-17
### Changed
- Bump to Rust edition 2024 (0.2.2), following the SVG-embedding feature added
  in 0.2.0 and the code-review hardening in 0.2.1.

Earlier history (0.1.x: the initial image-embedding companion crate split from
`medpdf`) is in the git log.

[Unreleased]: https://github.com/ComposerChrisF/medpdf/compare/medpdf-image-v0.4.3...HEAD
