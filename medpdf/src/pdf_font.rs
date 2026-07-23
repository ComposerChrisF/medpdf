//! Font discovery, resolution, and caching.
//!
//! Resolves font specifiers to concrete font data via a pipeline:
//! numeric handle → `@`-prefixed built-in → system search (font-kit) → direct file path.

use font_kit::source::SystemSource;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::error::{MedpdfError, Result};
use crate::font_data::FontData;

/// A resolved font location.
pub enum FontPath {
    /// Numeric font handle (legacy compatibility).
    Hack(u8),
    /// Standard PDF built-in font name (e.g. `Helvetica`).
    BuiltIn(String),
    /// Path to a font file on disk.
    Path(PathBuf),
    /// In-memory font data with a display name.
    Memory(Arc<Vec<u8>>, String),
}

impl FontPath {
    /// Returns a display name suitable for use as a PDF resource key.
    pub fn get_name(&self) -> String {
        match self {
            FontPath::Hack(n) => format!("F{n}"),
            FontPath::BuiltIn(s) => s.clone(),
            FontPath::Path(path) => path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown_font")
                .into(),
            FontPath::Memory(_, name) => name.clone(),
        }
    }
}

/// Caches font file reads as `Arc<Vec<u8>>`, keyed by file path.
pub struct FontCache {
    hash: HashMap<PathBuf, Arc<Vec<u8>>>,
}

impl FontCache {
    pub fn new() -> Self {
        Self {
            hash: HashMap::new(),
        }
    }

    /// Returns font data for the given path, reading from disk (and caching) if needed.
    pub fn get_data(&mut self, font_path: &FontPath) -> Result<FontData> {
        match font_path {
            FontPath::Hack(n) => Ok(FontData::Hack(*n)),
            FontPath::BuiltIn(name) => Ok(FontData::BuiltIn(name.clone())),
            FontPath::Path(path) => {
                if let Some(cached) = self.hash.get(path) {
                    return Ok(FontData::Embedded(Arc::clone(cached)));
                }
                let data = Arc::new(fs::read(path)?);
                self.hash.insert(path.clone(), Arc::clone(&data));
                Ok(FontData::Embedded(data))
            }
            FontPath::Memory(data, _) => Ok(FontData::Embedded(Arc::clone(data))),
        }
    }
}

impl Default for FontCache {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_font_path_as_number(font_path: &Path) -> Option<u8> {
    font_path.to_string_lossy().parse::<u8>().ok()
}

/// Resolves a font path to a non-system font (numeric hack, `@`-prefixed built-in, or file path).
///
/// `@`-prefixed fonts reference standard PDF fonts (e.g. `@Helvetica`, `@Courier`) without
/// embedding. Built-in fonts through PDF 1.7 include: Times-Roman, Helvetica, Courier, Symbol,
/// Times-Bold, Helvetica-Bold, Courier-Bold, ZapfDingbats, Times-Italic, Helvetica-Oblique,
/// Courier-Oblique, Times-BoldItalic, Helvetica-BoldOblique, Courier-BoldOblique.
/// Note: PDF 2.0 has *no* built-in fonts; all fonts must be embedded.
fn resolve_non_system_font(font_path: &Path) -> Option<FontPath> {
    if let Some(n) = parse_font_path_as_number(font_path) {
        return Some(FontPath::Hack(n));
    }
    let s = font_path.to_string_lossy();
    if let Some(name) = s.strip_prefix('@') {
        return Some(FontPath::BuiltIn(name.into()));
    }
    if font_path.exists() {
        return Some(FontPath::Path(font_path.into()));
    }
    None
}

/// Find a font with specific weight and style hints.
/// Falls back to `find_font` if the styled variant is not found.
pub fn find_font_with_style(
    font_path: &Path,
    weight: crate::types::FontWeight,
    style: crate::types::FontStyle,
) -> Result<FontPath> {
    if let Some(resolved) = resolve_non_system_font(font_path) {
        return Ok(resolved);
    }

    let source = SystemSource::new();
    let family_name = font_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| MedpdfError::new(format!("Invalid font path: {:?}", font_path)))?;

    let mut properties = font_kit::properties::Properties::new();
    properties.weight = font_kit::properties::Weight(weight.0 as f32);
    properties.style = match style {
        crate::types::FontStyle::Normal => font_kit::properties::Style::Normal,
        crate::types::FontStyle::Italic => font_kit::properties::Style::Italic,
        crate::types::FontStyle::Oblique(_) => font_kit::properties::Style::Oblique,
    };

    match source.select_best_match(
        &[font_kit::family_name::FamilyName::Title(
            family_name.to_string(),
        )],
        &properties,
    ) {
        Ok(handle) => handle_to_font_path(handle),
        Err(_) => {
            // Fall back to default properties, reusing the existing SystemSource
            find_font_with_source(font_path, &source)
        }
    }
}

/// Resolves a font specifier to a [`FontPath`] using default weight/style.
pub fn find_font(font_path: &Path) -> Result<FontPath> {
    if let Some(resolved) = resolve_non_system_font(font_path) {
        return Ok(resolved);
    }
    let source = SystemSource::new();
    find_font_with_source(font_path, &source)
}

/// Extracts the PostScript name from raw font bytes using ttf_parser.
/// Falls back to "EmbeddedFont" if parsing fails.
fn extract_font_name(data: &[u8]) -> String {
    ttf_parser::Face::parse(data, 0)
        .ok()
        .and_then(|face| {
            crate::font_helpers::get_name(&face, ttf_parser::name_id::POST_SCRIPT_NAME)
        })
        .unwrap_or_else(|| "EmbeddedFont".to_string())
}

/// Converts a font-kit handle into a [`FontPath`].
///
/// font-kit handles carry a `font_index` selecting a face inside a font collection
/// (`.ttc`/`.otc`); a nonzero index means the requested face is not the first one in
/// the file. We do not yet extract a single face from a collection, so rather than
/// drop the index and silently embed face 0 — the wrong face, with the whole
/// collection blob as its font program and `BaseFont = Unknown` — we fail loudly and
/// point the caller at a single-face file (bug-0012, Plan A). Extraction is a planned
/// follow-up (Plan B). Face-0 collections are caught at the embed step instead (a
/// nonzero index is not the only way a collection reaches embedding).
fn handle_to_font_path(handle: font_kit::handle::Handle) -> Result<FontPath> {
    match handle {
        font_kit::handle::Handle::Path { path, font_index } => {
            if font_index != 0 {
                return Err(MedpdfError::new(format!(
                    "Font resolved to face {font_index} of the collection {path:?}. Embedding a \
                     specific face from a font collection (.ttc/.otc) is not yet supported; supply \
                     a single-face .ttf/.otf font file, or an @-prefixed built-in (e.g. \
                     @Helvetica). (bug-0012)"
                )));
            }
            Ok(FontPath::Path(path))
        }
        font_kit::handle::Handle::Memory { bytes, font_index } => {
            if font_index != 0 {
                return Err(MedpdfError::new(format!(
                    "Font resolved to face {font_index} of an in-memory font collection. Embedding \
                     a specific face from a collection (.ttc/.otc) is not yet supported; supply a \
                     single-face .ttf/.otf font file, or an @-prefixed built-in (e.g. @Helvetica). \
                     (bug-0012)"
                )));
            }
            let name = extract_font_name(&bytes);
            Ok(FontPath::Memory(bytes, name))
        }
    }
}

fn find_font_with_source(font_path: &Path, source: &SystemSource) -> Result<FontPath> {
    let family_name = font_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| MedpdfError::new(format!("Invalid font path: {:?}", font_path)))?;
    let properties = font_kit::properties::Properties::new();
    let handle = source.select_best_match(
        &[font_kit::family_name::FamilyName::Title(
            family_name.to_string(),
        )],
        &properties,
    )?;

    handle_to_font_path(handle)
}
