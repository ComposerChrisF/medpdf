# Changelog

All notable changes to the `medpdf-image` crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0/).

## [Unreleased]

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
