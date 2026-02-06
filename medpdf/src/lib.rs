// medpdf - Medium-level PDF API over lopdf

pub mod error;
pub mod parsing;
pub mod pdf_helpers;
pub mod pdf_font;
pub mod font_helpers;
pub mod pdf_copy_page;
pub mod pdf_blank_page;
pub mod pdf_overlay;
pub mod pdf_watermark;

// Re-exports for convenience
pub use error::{Error, PdfMergeError, Result};
pub use pdf_helpers::Unit;
pub use parsing::parse_page_spec;
pub use pdf_copy_page::{copy_page, copy_page_with_cache};
pub use pdf_blank_page::create_blank_page;
pub use pdf_overlay::overlay_page;
pub use pdf_watermark::add_text;
