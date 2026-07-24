//! TTF/OTF font parsing, metrics extraction, and PDF font descriptor generation.

use ttf_parser::{Face, name_id};

use crate::error::MedpdfError;

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
    // Metrics below are in PDF glyph space (1000 units/em), scaled from the font's
    // own unitsPerEm — see [`glyph_space_scale`] and bug-0031. i32 (not i16) so a
    // small-upem font whose raw metric already sits near i16::MAX cannot overflow
    // when scaled up. `italic_angle` (degrees) and `stem_v` (a weight heuristic) are
    // NOT font-unit metrics and are left unscaled.
    pub font_bbox: [i32; 4],
    pub italic_angle: i16,
    pub ascent: i32,
    pub descent: i32,
    pub leading: i32,
    pub x_height: i32,
    pub stem_v: u16,
    pub cap_height: i32,
    pub font_file_key: String, // "FontFile" for Type1/MMType1; "FontFile2" for TrueType; "FontFile3" for CFF/OpenType
    // The embedded FontFile stream's own `/Subtype` (Some("OpenType") for FontFile3;
    // None for FontFile2) and whether it carries `/Length1` — see [`classify_font`]
    // and bug-0005.
    pub font_file_stream_subtype: Option<String>,
    pub font_file_emits_length1: bool,
    // CFF outlines: the composite (Type0) path refuses these for now (bug-0005).
    pub is_cff: bool,
}

/// The factor that converts a raw font-unit metric into PDF glyph space, where
/// 1000 units = 1 em (PDF 32000-1 §9.2.4). ttf-parser reports advances and
/// descriptor metrics in the font's own `unitsPerEm`, so every such value must be
/// multiplied by this before it reaches the PDF — otherwise a font whose upem ≠ 1000
/// (Arial/Verdana/most macOS TrueType are 2048) lays text out upem/1000× too wide
/// (bug-0031). This is the same formula the composite `/W` path already uses
/// (`pdf_font_composite::build_w_array`). Returns 0.0 for a degenerate upem of 0.
fn glyph_space_scale(face: &Face) -> f32 {
    let upem = face.units_per_em() as f32;
    if upem > 0.0 { 1000.0 / upem } else { 0.0 }
}

/// Scales a raw i16 font-unit metric into rounded PDF glyph space (see
/// [`glyph_space_scale`]).
fn scale_metric(value: i16, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

pub(crate) fn get_name(face: &Face, name_id: u16) -> Option<String> {
    face.names()
        .into_iter()
        .filter(|name| name.name_id == name_id)
        .find_map(|name| name.to_string())
}

/// Canonical mapping table for WinAnsiEncoding bytes 0x80–0x9F to Unicode.
/// Bytes not listed here (0x81, 0x8D, 0x8F, 0x90, 0x9D) are undefined in WinAnsi.
const WINANSI_SPECIAL: [(u8, char); 27] = [
    (0x80, '\u{20AC}'), // Euro sign
    (0x82, '\u{201A}'), // single low-9 quotation mark
    (0x83, '\u{0192}'), // latin small letter f with hook
    (0x84, '\u{201E}'), // double low-9 quotation mark
    (0x85, '\u{2026}'), // horizontal ellipsis
    (0x86, '\u{2020}'), // dagger
    (0x87, '\u{2021}'), // double dagger
    (0x88, '\u{02C6}'), // modifier letter circumflex accent
    (0x89, '\u{2030}'), // per mille sign
    (0x8A, '\u{0160}'), // latin capital letter s with caron
    (0x8B, '\u{2039}'), // single left-pointing angle quotation mark
    (0x8C, '\u{0152}'), // latin capital ligature oe
    (0x8E, '\u{017D}'), // latin capital letter z with caron
    (0x91, '\u{2018}'), // left single quotation mark
    (0x92, '\u{2019}'), // right single quotation mark
    (0x93, '\u{201C}'), // left double quotation mark
    (0x94, '\u{201D}'), // right double quotation mark
    (0x95, '\u{2022}'), // bullet
    (0x96, '\u{2013}'), // en dash
    (0x97, '\u{2014}'), // em dash
    (0x98, '\u{02DC}'), // small tilde
    (0x99, '\u{2122}'), // trade mark sign
    (0x9A, '\u{0161}'), // latin small letter s with caron
    (0x9B, '\u{203A}'), // single right-pointing angle quotation mark
    (0x9C, '\u{0153}'), // latin small ligature oe
    (0x9E, '\u{017E}'), // latin small letter z with caron
    (0x9F, '\u{0178}'), // latin capital letter y with diaeresis
];

/// Maps a WinAnsiEncoding byte value to its Unicode code point.
/// Bytes 0x00–0x7F and 0xA0–0xFF map directly to the same Unicode code point.
/// Bytes 0x80–0x9F map to specific Unicode characters (smart quotes, Euro, etc.).
/// Bytes 0x81, 0x8D, 0x8F, 0x90, 0x9D are undefined in WinAnsi and return None.
fn winansi_to_unicode(byte: u8) -> Option<char> {
    match byte {
        0x00..=0x7F | 0xA0..=0xFF => Some(byte as char),
        _ => WINANSI_SPECIAL
            .iter()
            .find(|(b, _)| *b == byte)
            .map(|(_, c)| *c),
    }
}

/// Maps a Unicode codepoint to its WinAnsiEncoding byte value.
/// Returns b'?' for characters not representable in WinAnsiEncoding.
pub(crate) fn unicode_to_winansi(c: char) -> u8 {
    let cp = c as u32;
    match cp {
        0x0000..=0x007F | 0x00A0..=0x00FF => cp as u8,
        _ => WINANSI_SPECIAL
            .iter()
            .find(|(_, ch)| *ch == c)
            .map(|(b, _)| *b)
            .unwrap_or(b'?'),
    }
}

/// Whether a character is representable in WinAnsiEncoding (CP1252).
///
/// Mirrors [`unicode_to_winansi`] exactly: a char is representable iff it maps to a
/// defined WinAnsi byte (Latin-1 `0x00–0x7F` / `0xA0–0xFF`, or one of the 0x80–0x9F
/// special mappings). Used to decide whether the single-byte fast path suffices or a
/// Type0 composite font is required.
pub(crate) fn char_in_winansi(c: char) -> bool {
    let cp = c as u32;
    match cp {
        0x0000..=0x007F | 0x00A0..=0x00FF => true,
        _ => WINANSI_SPECIAL.iter().any(|(_, ch)| *ch == c),
    }
}

/// Returns the distinct characters in `text` that are NOT representable in
/// WinAnsiEncoding, in first-seen order. Empty means the WinAnsi fast path is safe.
pub(crate) fn non_winansi_chars(text: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for ch in text.chars() {
        if !char_in_winansi(ch) && !out.contains(&ch) {
            out.push(ch);
        }
    }
    out
}

/// Looks up a glyph, consulting a Microsoft "symbol" cmap subtable (platform 3, encoding
/// 0) as a fallback.
///
/// [`Face::glyph_index`] reads only Unicode subtables, so it never sees the symbol cmap
/// that symbol fonts (Wingdings, Webdings) use — those publish their glyphs at
/// `0xF000 + code` there, and every plain `glyph_index` lookup for bytes 0–255 fails
/// (bug-0010). `unicode` is the code point to try against the Unicode cmap (for a text
/// font this is the WinAnsi-mapped char); `code` is the raw single-byte code to try
/// against the symbol subtable. Unicode is tried first, so text fonts are unaffected.
pub(crate) fn glyph_index_symbol_aware(
    face: &Face,
    unicode: Option<char>,
    code: u32,
) -> Option<ttf_parser::GlyphId> {
    if let Some(ch) = unicode
        && let Some(gid) = face.glyph_index(ch)
    {
        return Some(gid);
    }
    let cmap = face.tables().cmap?;
    for sub in cmap.subtables {
        // Microsoft symbol subtable: glyphs live at 0xF000 + code, with a bare-code
        // fallback for fonts that map at the raw byte.
        if sub.platform_id == ttf_parser::PlatformId::Windows
            && sub.encoding_id == 0
            && let Some(gid) = sub
                .glyph_index(0xF000 + code)
                .or_else(|| sub.glyph_index(code))
        {
            return Some(gid);
        }
    }
    None
}

pub(crate) fn get_font_widths(face: &Face, first_char: u8, last_char: u8) -> Vec<u16> {
    debug_assert!(
        last_char >= first_char,
        "last_char ({last_char}) must be >= first_char ({first_char})"
    );
    let scale = glyph_space_scale(face);
    let mut widths = vec![0; (last_char - first_char + 1) as usize];
    for ch in first_char..=last_char {
        // Symbol-aware: a symbol font has no WinAnsi glyph for `ch` but does have one at
        // 0xF000 + ch in its (3,0) cmap, so pass the WinAnsi char (may be None) for the
        // Unicode attempt and the raw byte for the symbol attempt (bug-0010).
        let glyph_index = glyph_index_symbol_aware(face, winansi_to_unicode(ch), ch as u32);
        if let Some(glyph_index) = glyph_index {
            let advance = face.glyph_hor_advance(glyph_index).unwrap_or_else(|| {
                log::trace!(
                    "Missing glyph advance for glyph {:?} (char {})",
                    glyph_index,
                    ch
                );
                0
            });
            // /Widths are in 1000-unit glyph space, not raw font units (bug-0031).
            // Saturating f32→u16 cast: advances and scale are non-negative and a
            // real glyph never scales past u16::MAX.
            widths[(ch - first_char) as usize] = (advance as f32 * scale).round() as u16;
        }
    }
    widths
}

/// Minimum number of A-Z/a-z glyphs (out of 52) for a font to be considered
/// a text font rather than symbolic. Chosen as ~38% coverage — symbol fonts
/// like Wingdings/ZapfDingbats typically have 0 Latin letters, while even
/// incomplete text fonts tend to have the full alphabet.
const SYMBOLIC_LATIN_THRESHOLD: usize = 20;

/// Detects whether a font is symbolic (e.g., Symbol, Dingbats, Wingdings).
/// Symbol fonts don't use standard character encodings.
fn detect_is_symbolic(face: &Face) -> bool {
    // Name-based detection for known symbol fonts
    let ps_name = get_name(face, name_id::POST_SCRIPT_NAME)
        .unwrap_or_default()
        .to_lowercase();
    let symbol_indicators = ["symbol", "dingbat", "wingding", "zapf", "icon", "ornament"];
    if symbol_indicators.iter().any(|s| ps_name.contains(s)) {
        return true;
    }

    // Character coverage heuristic: check for Latin letters
    let basic_latin_count = (b'A'..=b'Z')
        .chain(b'a'..=b'z')
        .filter(|&ch| face.glyph_index(ch as char).is_some())
        .count();

    if basic_latin_count < SYMBOLIC_LATIN_THRESHOLD {
        // Symbol-aware: Webdings has no Unicode glyphs in 32..=127 (the name heuristic
        // also misses it), so a Unicode-only probe here reports "no glyphs" and the font
        // is misclassified nonsymbolic. Consulting the (3,0) symbol cmap (0xF000+code)
        // finds its glyphs, so a font with glyphs only there is correctly symbolic
        // (bug-0010).
        let has_some_glyphs = (32u8..=127)
            .any(|ch| glyph_index_symbol_aware(face, Some(ch as char), ch as u32).is_some());
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
        // Scan full range for symbol fonts, symbol-aware (0xF000+code): a plain
        // glyph_index scan finds nothing for a symbol font, collapsing the range to the
        // (32, 255) fallback with all-zero widths (bug-0010).
        let first = (0u8..=255)
            .find(|&ch| glyph_index_symbol_aware(face, Some(ch as char), ch as u32).is_some())
            .unwrap_or(32);
        let last = (0u8..=255)
            .rev()
            .find(|&ch| glyph_index_symbol_aware(face, Some(ch as char), ch as u32).is_some())
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

pub(crate) fn get_pdf_font_bbox(face: &Face) -> [i32; 4] {
    let gbbox = face.global_bounding_box();
    let scale = glyph_space_scale(face);
    [
        scale_metric(gbbox.x_min, scale),
        scale_metric(gbbox.y_min, scale),
        scale_metric(gbbox.x_max, scale),
        scale_metric(gbbox.y_max, scale),
    ]
}

/// PDF classification of an embedded font program, derived from its outline flavor.
///
/// The three PDF objects describing an embedded font must agree on the flavor (PDF
/// 32000-1 Tables 110 & 127); a mismatch makes conforming viewers reject the embedded
/// program and substitute a font (bug-0005).
struct FontClassification {
    /// The Font dictionary's `/Subtype` (Table 110): `Type1` for CFF outlines,
    /// `TrueType` for `glyf` outlines. (`Type1C` is a *stream* subtype — legal on the
    /// FontFile3 stream, never on a Font dictionary.)
    dict_subtype: &'static str,
    /// The FontDescriptor key holding the embedded program: `FontFile3` for CFF,
    /// `FontFile2` for TrueType.
    font_file_key: &'static str,
    /// The embedded stream's own `/Subtype`. FontFile3 requires one (`OpenType` for
    /// the sfnt-wrapped OTF bytes we embed); FontFile2 has none.
    stream_subtype: Option<&'static str>,
    /// Whether the stream carries `/Length1` (uncompressed length) — defined for
    /// FontFile/FontFile2, not FontFile3.
    emits_length1: bool,
    /// CFF outlines. Drives the composite-path fail-loud: a `CIDFontType2` descendant
    /// requires TrueType, so a composite CFF font is refused until `CIDFontType0` is
    /// implemented.
    is_cff: bool,
}

impl FontClassification {
    const CFF: Self = Self {
        dict_subtype: "Type1",
        font_file_key: "FontFile3",
        stream_subtype: Some("OpenType"),
        emits_length1: false,
        is_cff: true,
    };
    const TRUETYPE: Self = Self {
        dict_subtype: "TrueType",
        font_file_key: "FontFile2",
        stream_subtype: None,
        emits_length1: true,
        is_cff: false,
    };
}

/// Classifies a font face by outline flavor (CFF vs TrueType `glyf`). CFF takes
/// priority if both tables are present, and an unrecognized program is treated as
/// CFF/OpenType (its bytes are sfnt-wrapped, so `/Subtype /OpenType` describes them).
fn classify_font(face: &Face) -> FontClassification {
    let raw = face.raw_face();
    if raw.table(ttf_parser::Tag::from_bytes(b"CFF ")).is_some() {
        FontClassification::CFF
    } else if raw.table(ttf_parser::Tag::from_bytes(b"glyf")).is_some() {
        FontClassification::TRUETYPE
    } else {
        log::warn!("Font file type not recognized; assuming CFF/OpenType");
        FontClassification::CFF
    }
}

pub(crate) fn get_pdf_font_info_of_data(
    font_data: &[u8],
) -> Result<(FontPdfInfo, FontDescriptorPdfInfo), MedpdfError> {
    let face = Face::parse(font_data, 0)?;
    Ok(get_pdf_info_of_face(&face))
}

/// Measure the width of a text string in points for the given font data and size.
/// Sums glyph horizontal advances scaled by font_size / units_per_em.
pub fn measure_text_width(
    font_data: &crate::font_data::FontData,
    font_size: f32,
    text: &str,
) -> Result<f32, MedpdfError> {
    match font_data {
        crate::font_data::FontData::Hack(_) | crate::font_data::FontData::BuiltIn(_) => {
            // Rough estimate: 0.6 * font_size per character for monospace-ish fonts.
            // Count characters, not bytes, so multibyte text is not over-measured.
            Ok(text.chars().count() as f32 * font_size * 0.6)
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
                // Symbol-aware so a symbol font (Wingdings/Webdings) measures its real
                // advances instead of zero — its glyphs live in a (3,0) cmap that
                // glyph_index skips (bug-0010).
                if let Some(glyph_id) =
                    glyph_index_symbol_aware(&face, Some(ch), unicode_to_winansi(ch) as u32)
                {
                    width += face.glyph_hor_advance(glyph_id).unwrap_or_else(|| {
                        log::trace!(
                            "Missing glyph advance for glyph {:?} (char '{}')",
                            glyph_id,
                            ch
                        );
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

    let ps_name =
        get_name(face, name_id::POST_SCRIPT_NAME).unwrap_or_else(|| "Unknown".to_string());

    // Descriptor metrics come out of ttf-parser in the font's own unitsPerEm; scale
    // every one into PDF glyph space (bug-0031). Fallbacks are computed in raw units
    // first, then scaled, so upem-500-with-no-x_height still yields 500.
    let scale = glyph_space_scale(face);
    let x_height_raw = face
        .x_height()
        .unwrap_or((face.units_per_em() as f32 * 0.5) as i16);
    let cap_height_raw = face.capital_height().unwrap_or(face.ascender());
    let class = classify_font(face);

    (
        FontPdfInfo {
            base_font: ps_name.clone(),
            encoding,
            first_char: first_char.into(),
            last_char: last_char.into(),
            widths: get_font_widths(face, first_char, last_char),
            subtype: class.dict_subtype.to_string(),
        },
        FontDescriptorPdfInfo {
            font_name: ps_name,
            flags: compute_pdf_font_flags_internal(face, is_symbolic),
            font_bbox: get_pdf_font_bbox(face),
            italic_angle: face.italic_angle().round() as i16, // degrees — not scaled
            ascent: scale_metric(face.ascender(), scale),
            descent: scale_metric(face.descender(), scale),
            leading: scale_metric(face.line_gap(), scale),
            x_height: scale_metric(x_height_raw, scale),
            stem_v: guess_pdf_stem_v_for_font(face), // weight heuristic — not scaled
            cap_height: scale_metric(cap_height_raw, scale),
            font_file_key: class.font_file_key.to_string(),
            font_file_stream_subtype: class.stream_subtype.map(str::to_string),
            font_file_emits_length1: class.emits_length1,
            is_cff: class.is_cff,
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
            font_file_stream_subtype: None,
            font_file_emits_length1: true,
            is_cff: false,
        };
        let cloned = desc.clone();
        assert_eq!(cloned.font_name, desc.font_name);
        assert_eq!(cloned.flags, desc.flags);
        assert_eq!(cloned.font_bbox, desc.font_bbox);
    }
}
