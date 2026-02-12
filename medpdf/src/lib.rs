// medpdf - Medium-level PDF API over lopdf

pub mod error;
pub mod font_helpers;
pub mod parsing;
pub mod pdf_blank_page;
pub mod pdf_copy_page;
pub mod pdf_delete_page;
pub mod pdf_font;
pub mod pdf_helpers;
pub mod pdf_overlay;
pub mod pdf_watermark;
pub mod types;

// Re-exports for convenience
pub use error::{Error, PdfMergeError, Result};
pub use font_helpers::measure_text_width;
pub use parsing::parse_page_spec;
pub use pdf_blank_page::create_blank_page;
pub use pdf_copy_page::{copy_page, copy_page_with_cache};
pub use pdf_delete_page::delete_page;
pub use pdf_font::{find_font, find_font_with_style, FontCache, FontPath};
pub use pdf_helpers::{get_page_media_box, get_page_rotation, set_page_rotation, Unit};
pub use pdf_overlay::overlay_page;
pub use pdf_watermark::{add_line, add_rect, add_text_params};
pub use types::{AddTextParams, DrawLineParams, DrawRectParams, FontStyle, FontWeight, HAlign, PdfColor, VAlign};
