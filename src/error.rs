
use std::fmt::Debug;

pub enum PdfMergeError {
    Io(std::io::Error),
    LoPdf(lopdf::Error),
    FontKit(font_kit::error::SelectionError),
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
        }
    }
}
pub type Error = PdfMergeError;
pub type Result<T> = std::result::Result<T, Error>;
