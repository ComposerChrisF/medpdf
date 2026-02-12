use ttf_parser::{name_id, Face};

use crate::error::PdfMergeError;

#[derive(Debug, Clone)]
pub(crate) struct FontPdfInfo {
    pub base_font: String,
    pub subtype: String,
    pub encoding: Option<String>, // None for symbol fonts
    pub first_char: u16,
    pub last_char: u16,
    pub widths: Vec<u16>,
}

#[derive(Debug, Clone)]
pub(crate) struct FontDescriptorPdfInfo {
    pub font_name: String,
    pub flags: u16,
    pub font_bbox: [i16; 4],
    pub italic_angle: i16,
    pub ascent: i16,
    pub descent: i16,
    pub leading: i16,
    pub x_height: i16,
    pub stem_v: u16,
    pub cap_height: i16,
    pub font_file_key: String, // "FontFile" for Type1 or MMType1; "FontFile2" for TrueType; others for compated formats
                               //pub embedded_font_subtype: String, // "Type1" or "Type1C" or "TrueType" or "CIDFontType0" or "CIDFontType2"
}

pub(crate) fn get_name(face: &Face, name_id: u16) -> String {
    face.names()
        .into_iter()
        .filter(|name| name.name_id == name_id)
        .find_map(|name| name.to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

/// Maps a WinAnsiEncoding byte value to its Unicode code point.
/// Bytes 0x00–0x7F and 0xA0–0xFF map directly to the same Unicode code point.
/// Bytes 0x80–0x9F map to specific Unicode characters (smart quotes, Euro, etc.).
/// Bytes 0x81, 0x8D, 0x8F, 0x90, 0x9D are undefined in WinAnsi and return None.
fn winansi_to_unicode(byte: u8) -> Option<char> {
    match byte {
        0x00..=0x7F => Some(byte as char),
        0xA0..=0xFF => Some(byte as char),
        0x80 => Some('\u{20AC}'), // Euro sign
        0x82 => Some('\u{201A}'), // single low-9 quotation mark
        0x83 => Some('\u{0192}'), // latin small letter f with hook
        0x84 => Some('\u{201E}'), // double low-9 quotation mark
        0x85 => Some('\u{2026}'), // horizontal ellipsis
        0x86 => Some('\u{2020}'), // dagger
        0x87 => Some('\u{2021}'), // double dagger
        0x88 => Some('\u{02C6}'), // modifier letter circumflex accent
        0x89 => Some('\u{2030}'), // per mille sign
        0x8A => Some('\u{0160}'), // latin capital letter s with caron
        0x8B => Some('\u{2039}'), // single left-pointing angle quotation mark
        0x8C => Some('\u{0152}'), // latin capital ligature oe
        0x8E => Some('\u{017D}'), // latin capital letter z with caron
        0x91 => Some('\u{2018}'), // left single quotation mark
        0x92 => Some('\u{2019}'), // right single quotation mark
        0x93 => Some('\u{201C}'), // left double quotation mark
        0x94 => Some('\u{201D}'), // right double quotation mark
        0x95 => Some('\u{2022}'), // bullet
        0x96 => Some('\u{2013}'), // en dash
        0x97 => Some('\u{2014}'), // em dash
        0x98 => Some('\u{02DC}'), // small tilde
        0x99 => Some('\u{2122}'), // trade mark sign
        0x9A => Some('\u{0161}'), // latin small letter s with caron
        0x9B => Some('\u{203A}'), // single right-pointing angle quotation mark
        0x9C => Some('\u{0153}'), // latin small ligature oe
        0x9E => Some('\u{017E}'), // latin small letter z with caron
        0x9F => Some('\u{0178}'), // latin capital letter y with diaeresis
        _ => None, // 0x81, 0x8D, 0x8F, 0x90, 0x9D are undefined
    }
}

pub(crate) fn get_font_widths(face: &Face, first_char: u8, last_char: u8) -> Vec<u16> {
    debug_assert!(last_char >= first_char, "last_char ({last_char}) must be >= first_char ({first_char})");
    let mut widths = vec![0; (last_char - first_char + 1) as usize];
    for ch in first_char..=last_char {
        let unicode_char = match winansi_to_unicode(ch) {
            Some(c) => c,
            None => continue,
        };
        let glyph_index = face.glyph_index(unicode_char);
        if let Some(glyph_index) = glyph_index {
            widths[(ch - first_char) as usize] = face.glyph_hor_advance(glyph_index).unwrap_or_else(|| {
                log::trace!("Missing glyph advance for glyph {:?} (char {})", glyph_index, ch);
                0
            });
        }
    }
    widths
}

/// Detects whether a font is symbolic (e.g., Symbol, Dingbats, Wingdings).
/// Symbol fonts don't use standard character encodings.
fn detect_is_symbolic(face: &Face) -> bool {
    // Name-based detection for known symbol fonts
    let ps_name = get_name(face, name_id::POST_SCRIPT_NAME).to_lowercase();
    let symbol_indicators = ["symbol", "dingbat", "wingding", "zapf", "icon", "ornament"];
    if symbol_indicators.iter().any(|s| ps_name.contains(s)) {
        return true;
    }

    // Character coverage heuristic: check for Latin letters
    let basic_latin_count = (b'A'..=b'Z')
        .chain(b'a'..=b'z')
        .filter(|&ch| face.glyph_index(ch as char).is_some())
        .count();

    // If fewer than 20 of 52 Latin letters, likely symbolic
    if basic_latin_count < 20 {
        let has_some_glyphs = (32u8..=127).any(|ch| face.glyph_index(ch as char).is_some());
        if has_some_glyphs {
            return true;
        }
    }

    false
}

/// Determines the PDF encoding for a font.
/// Symbol fonts use their own encoding and return None.
fn determine_pdf_encoding(is_symbolic: bool) -> Option<String> {
    if is_symbolic {
        None // Symbol fonts use their own encoding
    } else {
        Some("WinAnsiEncoding".to_string()) // Cross-platform compatible
    }
}

/// Computes the character range for a font.
/// Symbol fonts scan for actual glyph coverage; regular fonts use 32-255.
fn compute_char_range(face: &Face, is_symbolic: bool) -> (u8, u8) {
    if is_symbolic {
        // Scan full range for symbol fonts
        let first = (0u8..=255)
            .find(|&ch| face.glyph_index(ch as char).is_some())
            .unwrap_or(32);
        let last = (0u8..=255)
            .rev()
            .find(|&ch| face.glyph_index(ch as char).is_some())
            .unwrap_or(255);
        (first, last)
    } else {
        // Standard range for regular fonts
        (32, 255)
    }
}

fn compute_pdf_font_flags_internal(face: &Face, is_symbolic: bool) -> u16 {
    let is_italic_flag = face.is_italic() || face.is_oblique();
    (if face.is_monospaced() { 0x0001 } else { 0x0000 }) |      // Bit 1 = FixedPitch
        (if is_symbolic      { 0x0004 } else { 0x0000 }) |      // Bit 3 = Symbolic
        (if !is_symbolic     { 0x0020 } else { 0x0000 }) |      // Bit 6 = Nonsymbolic
        (if is_italic_flag   { 0x0040 } else { 0x0000 }) // Bit 7 = Italic (slanted strokes)
}

pub(crate) fn guess_pdf_stem_v_for_font(face: &Face) -> u16 {
    let w_temp = face.weight().to_number() as f32 / 65.0;
    // Also: (10.0 + 220. * ((face.weight().to_number() as f32 - 50.0) / 900.0)).floor() as u16
    (50.0 + w_temp * w_temp + 0.5).floor() as u16
}

pub(crate) fn get_pdf_font_bbox(face: &Face) -> [i16; 4] {
    let gbbox = face.global_bounding_box();
    [gbbox.x_min, gbbox.y_min, gbbox.x_max, gbbox.y_max]
}

/// Classifies a font face into its PDF font file key and subtype.
/// Returns `(font_file_key, subtype)`.
fn classify_font(face: &Face) -> (&'static str, &'static str) {
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CFF "))
        .is_some()
    {
        ("FontFile", "Type1")
    } else if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"glyf"))
        .is_some()
    {
        ("FontFile2", "TrueType")
    } else {
        log::warn!("Font file type not recognized");
        ("FontFile", "Type1")
    }
}

pub(crate) fn get_pdf_font_file_key(face: &Face) -> String {
    classify_font(face).0.to_string()
}

pub(crate) fn get_pdf_font_subtype(face: &Face) -> String {
    classify_font(face).1.to_string()
}

pub(crate) fn get_pdf_font_info_of_data(
    font_data: &[u8],
) -> Result<(FontPdfInfo, FontDescriptorPdfInfo), PdfMergeError> {
    let face = Face::parse(font_data, 0)?;
    Ok(get_pdf_info_of_face(&face))
}

/// Measure the width of a text string in points for the given font data and size.
/// Sums glyph horizontal advances scaled by font_size / units_per_em.
pub fn measure_text_width(
    font_data: &crate::font_data::FontData,
    font_size: f32,
    text: &str,
) -> Result<f32, PdfMergeError> {
    match font_data {
        crate::font_data::FontData::Hack(_) | crate::font_data::FontData::BuiltIn(_) => {
            // Rough estimate: 0.6 * font_size per character for monospace-ish fonts
            Ok(text.len() as f32 * font_size * 0.6)
        }
        crate::font_data::FontData::Embedded(data) => {
            let face = Face::parse(data, 0)?;
            let units_per_em = face.units_per_em() as f32;
            if units_per_em == 0.0 {
                return Ok(0.0);
            }
            let scale = font_size / units_per_em;
            let mut width: f32 = 0.0;
            for ch in text.chars() {
                if let Some(glyph_id) = face.glyph_index(ch) {
                    width += face.glyph_hor_advance(glyph_id).unwrap_or_else(|| {
                        log::trace!("Missing glyph advance for glyph {:?} (char '{}')", glyph_id, ch);
                        0
                    }) as f32;
                }
            }
            Ok(width * scale)
        }
    }
}

pub(crate) fn get_pdf_info_of_face(face: &Face) -> (FontPdfInfo, FontDescriptorPdfInfo) {
    let is_symbolic = detect_is_symbolic(face);
    let (first_char, last_char) = compute_char_range(face, is_symbolic);
    let encoding = determine_pdf_encoding(is_symbolic);

    (
        FontPdfInfo {
            base_font: get_name(face, name_id::POST_SCRIPT_NAME),
            encoding,
            first_char: first_char.into(),
            last_char: last_char.into(),
            widths: get_font_widths(face, first_char, last_char),
            subtype: get_pdf_font_subtype(face),
        },
        FontDescriptorPdfInfo {
            font_name: get_name(face, name_id::POST_SCRIPT_NAME),
            flags: compute_pdf_font_flags_internal(face, is_symbolic),
            font_bbox: get_pdf_font_bbox(face),
            italic_angle: face.italic_angle().round() as i16,
            ascent: face.ascender(),
            descent: face.descender(),
            leading: face.line_gap(),
            x_height: face.x_height().unwrap_or((face.units_per_em() as f32 * 0.5) as i16),
            stem_v: guess_pdf_stem_v_for_font(face),
            cap_height: face.capital_height().unwrap_or(face.ascender()),
            font_file_key: get_pdf_font_file_key(face),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_pdf_font_info_of_data_invalid_data() {
        let result = get_pdf_font_info_of_data(&[]);
        assert!(result.is_err(), "Empty data should fail");
    }

    #[test]
    fn test_get_pdf_font_info_of_data_random_bytes() {
        let result = get_pdf_font_info_of_data(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(result.is_err(), "Random bytes should fail");
    }

    #[test]
    fn test_font_pdf_info_clone() {
        let info = FontPdfInfo {
            base_font: "TestFont".to_string(),
            subtype: "TrueType".to_string(),
            encoding: Some("WinAnsiEncoding".to_string()),
            first_char: 32,
            last_char: 255,
            widths: vec![600; 224],
        };
        let cloned = info.clone();
        assert_eq!(cloned.base_font, info.base_font);
        assert_eq!(cloned.encoding, info.encoding);
        assert_eq!(cloned.widths.len(), info.widths.len());
    }

    #[test]
    fn test_font_pdf_info_no_encoding_for_symbol() {
        let info = FontPdfInfo {
            base_font: "Symbol".to_string(),
            subtype: "Type1".to_string(),
            encoding: None,
            first_char: 0,
            last_char: 255,
            widths: vec![600; 256],
        };
        assert!(info.encoding.is_none());
    }

    #[test]
    fn test_font_descriptor_pdf_info_clone() {
        let desc = FontDescriptorPdfInfo {
            font_name: "TestFont".to_string(),
            flags: 0x0020,
            font_bbox: [-100, -200, 1000, 800],
            italic_angle: 0,
            ascent: 800,
            descent: -200,
            leading: 0,
            x_height: 500,
            stem_v: 80,
            cap_height: 700,
            font_file_key: "FontFile2".to_string(),
        };
        let cloned = desc.clone();
        assert_eq!(cloned.font_name, desc.font_name);
        assert_eq!(cloned.flags, desc.flags);
        assert_eq!(cloned.font_bbox, desc.font_bbox);
    }
}
