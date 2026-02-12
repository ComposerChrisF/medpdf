use std::fmt::Debug;

pub enum PdfMergeError {
    Io(std::io::Error),
    LoPdf(lopdf::Error),
    FontKit(font_kit::error::SelectionError),
    Face(ttf_parser::FaceParsingError),
    Message(String),
}
impl PdfMergeError {
    pub fn new<T: Into<String>>(msg: T) -> Self {
        PdfMergeError::Message(msg.into())
    }
}
impl From<lopdf::Error> for PdfMergeError {
    fn from(err: lopdf::Error) -> Self {
        PdfMergeError::LoPdf(err)
    }
}
impl From<std::io::Error> for PdfMergeError {
    fn from(err: std::io::Error) -> Self {
        PdfMergeError::Io(err)
    }
}
impl From<ttf_parser::FaceParsingError> for PdfMergeError {
    fn from(err: ttf_parser::FaceParsingError) -> Self {
        PdfMergeError::Face(err)
    }
}
impl From<&str> for PdfMergeError {
    fn from(err: &str) -> Self {
        PdfMergeError::Message(err.into())
    }
}
impl From<String> for PdfMergeError {
    fn from(err: String) -> Self {
        PdfMergeError::Message(err)
    }
}
impl From<font_kit::error::SelectionError> for PdfMergeError {
    fn from(err: font_kit::error::SelectionError) -> Self {
        PdfMergeError::FontKit(err)
    }
}

impl Debug for PdfMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => f.debug_tuple("Io").field(e).finish(),
            Self::LoPdf(e) => f.debug_tuple("LoPdf").field(e).finish(),
            Self::Message(e) => f.debug_tuple("Message").field(e).finish(),
            Self::FontKit(e) => f.debug_tuple("FontKit").field(e).finish(),
            Self::Face(e) => f.debug_tuple("Face").field(e).finish(),
        }
    }
}

impl std::fmt::Display for PdfMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::LoPdf(e) => write!(f, "PDF error: {:?}", e),
            Self::Message(msg) => write!(f, "{}", msg),
            Self::FontKit(e) => write!(f, "Font error: {:?}", e),
            Self::Face(e) => write!(f, "Font parsing error: {:?}", e),
        }
    }
}

impl std::error::Error for PdfMergeError {}

pub type Error = PdfMergeError;
pub type Result<T> = std::result::Result<T, Error>;
