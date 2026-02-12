use font_kit::source::SystemSource;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::error::{PdfMergeError, Result};

pub enum FontPath {
    Hack(u8),
    BuiltIn(String),
    Path(PathBuf),
}

impl FontPath {
    pub fn get_name(&self) -> String {
        match self {
            FontPath::Hack(n) => format!("F{n}"),
            FontPath::BuiltIn(s) => s.clone(),
            FontPath::Path(path) => path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown_font")
                .into(),
        }
    }
}

pub struct FontCache {
    hash: HashMap<PathBuf, Arc<Vec<u8>>>,
}

impl FontCache {
    pub fn new() -> Self {
        Self {
            hash: HashMap::new(),
        }
    }

    pub fn get_data(&mut self, font_path: &FontPath) -> Result<Arc<Vec<u8>>> {
        match font_path {
            FontPath::Hack(n) => Ok(Arc::new(vec![*n])),
            FontPath::BuiltIn(_) => Ok(Arc::new(vec![b'@'])),
            FontPath::Path(path) => {
                if let Some(cached) = self.hash.get(path) {
                    return Ok(Arc::clone(cached));
                }
                let data = Arc::new(fs::read(path)?);
                self.hash.insert(path.clone(), Arc::clone(&data));
                Ok(data)
            }
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

/// Find a font with specific weight and style hints.
/// Falls back to `find_font` if the styled variant is not found.
pub fn find_font_with_style(
    font_path: &Path,
    weight: crate::types::FontWeight,
    style: crate::types::FontStyle,
) -> Result<FontPath> {
    // Hack and BuiltIn paths don't support style selection
    if let Some(n) = parse_font_path_as_number(font_path) {
        return Ok(FontPath::Hack(n));
    }
    if font_path.to_string_lossy().starts_with("@") {
        return Ok(FontPath::BuiltIn(font_path.to_string_lossy()[1..].into()));
    }
    if font_path.exists() {
        return Ok(FontPath::Path(font_path.into()));
    }

    // Search system fonts with weight/style properties
    let source = SystemSource::new();
    let family_name = font_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| PdfMergeError::new(format!("Invalid font path: {:?}", font_path)))?;

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
        Ok(font_kit::handle::Handle::Path { path, .. }) => Ok(FontPath::Path(path)),
        Ok(_) => Err(format!("Font {font_path:?} not found as path handle").into()),
        Err(_) => {
            // Fall back to default properties
            find_font(font_path)
        }
    }
}

pub fn find_font(font_path: &Path) -> Result<FontPath> {
    if let Some(n) = parse_font_path_as_number(font_path) {
        // This is a short-hand to use a font already in this document, although not necessarily stable!
        return Ok(FontPath::Hack(n));
    }
    if font_path.to_string_lossy().starts_with("@") {
        // This is a "named" font--we special case text starting with '@' to be a valid font name
        // we can reference without embedding the font itself.  We will see the '@' in later code
        // and remove it, and reference this font by this given name (without the ampersand), and
        // without embedding the font.
        //
        // NOTE: This mechanism is primarily designed to reference the "standard" PDF fonts (e.g.
        // "Helvetica", "Courier", etc.) for debugging.  But it might be usable to reference fonts
        // already installed on a user's system.
        //
        // Built in fonts (through PDF 1.7): Times-Roman, Helvetica, Courier, Symbol, Times-Bold,
        // Helvetica-Bold, Courier-Bold, ZapfDingbats, Times-Italic, Helvetica-Oblique,
        // Courier-Oblique, Times-BoldItalic, Helvetica-BoldOblique, Courier-BoldOblique
        //
        // NOTE that for PDF 2.0 format, ther are *NO* built-in fonts (like "Helvetica", "Courier",
        // etc.), so all fonts are supposed to be embedded.
        return Ok(FontPath::BuiltIn(font_path.to_string_lossy()[1..].into()));
    }
    if font_path.exists() {
        // Full path to font, no need to search
        return Ok(FontPath::Path(font_path.into()));
    }
    // Search system fonts
    let source = SystemSource::new();
    let family_name = font_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| PdfMergeError::new(format!("Invalid font path: {:?}", font_path)))?;
    let properties = font_kit::properties::Properties::new();
    let handle = source.select_best_match(
        &[font_kit::family_name::FamilyName::Title(
            family_name.to_string(),
        )],
        &properties,
    )?;
    //.ok_or_else(|| format!("Font '{}' not found in CWD or system", family_name))?;

    if let font_kit::handle::Handle::Path { path, .. } = handle {
        Ok(FontPath::Path(path))
    } else {
        Err(format!("Font {font_path:?} not found").into())
    }
}
