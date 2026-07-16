//! Error types for medpdf operations.

use std::fmt::Debug;

/// Unified error type for all medpdf operations.
#[non_exhaustive]
pub enum MedpdfError {
    Io(std::io::Error),
    LoPdf(lopdf::Error),
    FontKit(font_kit::error::SelectionError),
    Face(ttf_parser::FaceParsingError),
    Message(String),
    /// Text contained characters that cannot be rendered with the chosen font:
    /// a built-in Standard-14 font (WinAnsi-bound) asked to draw non-CP1252 text,
    /// or an embedded font that lacks a glyph for one of the characters. Carries
    /// the offending characters and the font name. Emitted instead of silently
    /// substituting `?` (fail-loudly); enable `lossy_text` to opt back into
    /// best-effort substitution.
    UnrepresentableText {
        chars: Vec<char>,
        font: String,
    },
}
impl MedpdfError {
    pub fn new<T: Into<String>>(msg: T) -> Self {
        MedpdfError::Message(msg.into())
    }
}
impl From<lopdf::Error> for MedpdfError {
    fn from(err: lopdf::Error) -> Self {
        MedpdfError::LoPdf(err)
    }
}
impl From<std::io::Error> for MedpdfError {
    fn from(err: std::io::Error) -> Self {
        MedpdfError::Io(err)
    }
}
impl From<ttf_parser::FaceParsingError> for MedpdfError {
    fn from(err: ttf_parser::FaceParsingError) -> Self {
        MedpdfError::Face(err)
    }
}
impl From<&str> for MedpdfError {
    fn from(err: &str) -> Self {
        MedpdfError::Message(err.into())
    }
}
impl From<String> for MedpdfError {
    fn from(err: String) -> Self {
        MedpdfError::Message(err)
    }
}
impl From<font_kit::error::SelectionError> for MedpdfError {
    fn from(err: font_kit::error::SelectionError) -> Self {
        MedpdfError::FontKit(err)
    }
}

impl Debug for MedpdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => f.debug_tuple("Io").field(e).finish(),
            Self::LoPdf(e) => f.debug_tuple("LoPdf").field(e).finish(),
            Self::Message(e) => f.debug_tuple("Message").field(e).finish(),
            Self::FontKit(e) => f.debug_tuple("FontKit").field(e).finish(),
            Self::Face(e) => f.debug_tuple("Face").field(e).finish(),
            Self::UnrepresentableText { chars, font } => f
                .debug_struct("UnrepresentableText")
                .field("chars", chars)
                .field("font", font)
                .finish(),
        }
    }
}

impl std::fmt::Display for MedpdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::LoPdf(e) => write!(f, "PDF error: {}", e),
            Self::Message(msg) => write!(f, "{}", msg),
            Self::FontKit(e) => write!(f, "Font error: {}", e),
            Self::Face(e) => write!(f, "Font parsing error: {}", e),
            Self::UnrepresentableText { chars, font } => {
                let list = chars
                    .iter()
                    .map(|c| format!("U+{:04X} '{}'", *c as u32, c))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "text contains character(s) not representable with font '{font}': {list}. \
                     Use an embedded system font that includes these glyphs, or enable lossy text substitution."
                )
            }
        }
    }
}

impl std::error::Error for MedpdfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::LoPdf(e) => Some(e),
            Self::FontKit(e) => Some(e),
            Self::Face(e) => Some(e),
            Self::Message(_) => None,
            Self::UnrepresentableText { .. } => None,
        }
    }
}

/// Alias for [`MedpdfError`].
pub type Error = MedpdfError;

/// Convenience result type for medpdf operations.
pub type Result<T> = std::result::Result<T, Error>;
