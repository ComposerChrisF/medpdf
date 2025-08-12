use std::{borrow::Cow, collections::HashMap, fs, path::Path};

use ttf_parser::{name_id, Face};

use crate::error::PdfMergeError;

#[allow(dead_code)]
pub fn get_name_id_map() -> HashMap<u16, &'static str> {
    HashMap::from([
        (name_id::COPYRIGHT_NOTICE, "Copyright Notice"),
        (name_id::FAMILY, "Family"),
        (name_id::SUBFAMILY, "Subfamily"),
        (name_id::UNIQUE_ID, "Unique ID"),
        (name_id::FULL_NAME, "Full Name"),
        (name_id::VERSION, "Version"),
        (name_id::POST_SCRIPT_NAME, "PostScript Name"),
        (name_id::TRADEMARK, "Trademark"),
        (name_id::MANUFACTURER, "Manufacturer"),
        (name_id::DESIGNER, "Designer"),
        (name_id::DESCRIPTION, "Description"),
        (name_id::VENDOR_URL, "Vendor URL"),
        (name_id::DESIGNER_URL, "Designer URL"),
        (name_id::LICENSE, "License"),
        (name_id::LICENSE_URL, "License URL"),
        (name_id::TYPOGRAPHIC_FAMILY, "Typographic Family"),
        (name_id::TYPOGRAPHIC_SUBFAMILY, "Typographic Subfamily"),
        (name_id::COMPATIBLE_FULL, "Compatible Full"),
        (name_id::SAMPLE_TEXT, "Sample Text"),
        (name_id::POST_SCRIPT_CID, "PostScript CID"),
        (name_id::WWS_FAMILY, "WWS Family"),
        (name_id::WWS_SUBFAMILY, "WWS Subfamily"),
        (name_id::LIGHT_BACKGROUND_PALETTE, "Light Background Palette"),
        (name_id::DARK_BACKGROUND_PALETTE, "Dark Background Palette"),
        (name_id::VARIATIONS_POST_SCRIPT_NAME_PREFIX, "Variations PostScript Name Prefix"),
    ])
}


#[derive(Debug, Clone)]
pub struct FontPdfInfo {
    pub base_font: String,
    pub subtype: String,
    pub encoding: String,
    pub first_char: u16,
    pub last_char: u16,
    pub widths: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct FontDescriptorPdfInfo {
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
    pub font_file_key: String,  // "FontFile" for Type1 or MMType1; "FontFile2" for TrueType; others for compated formats
    //pub embedded_font_subtype: String, // "Type1" or "Type1C" or "TrueType" or "CIDFontType0" or "CIDFontType2"
}

 
pub fn get_name<'a>(face: &Face<'a>, name_id: u16) -> Cow<'a, str> {
    face.names().into_iter().find(|name| name.name_id == name_id)
        .and_then(|name| Some(String::from_utf8_lossy(name.name)))
        .unwrap_or("<none>".into())
}

pub fn get_font_widths(face: &Face, first_char: u8, last_char: u8) -> Vec<u16> {
    let mut widths = vec![0; (last_char - first_char + 1) as usize];
    for ch in first_char..=last_char {
        let glyph_index = face.glyph_index(ch as char);
        if let Some(glyph_index) = glyph_index {
            widths[(ch - first_char) as usize] = face.glyph_hor_advance(glyph_index).unwrap_or(0);
        }
    }
    widths
}

pub fn compute_pdf_font_flags(face: &Face) -> u16 {
    let is_symbolic = false;
    let is_italic_flag = face.is_italic() || face.is_oblique();
    let flags = 
        if face.is_monospaced() { 0x0001 } else { 0x0000 } |    // Bit 1 = FixedPitch
        if is_symbolic  { 0x0004 } else { 0x0000 } |            // Bit 3 = Symbolic
        if !is_symbolic  { 0x0020 } else { 0x0000 } |           // Bit 6 = Nonsymbolic (we're assuming...)
        if is_italic_flag { 0x0040 } else { 0x0000 };           // Bit 7 = Italic (slanted strokes)
    flags
}

pub fn guess_pdf_stem_v_for_font(face: &Face) -> u16 {
    let w_temp = face.weight().to_number() as f32 / 65.0;
    let stem_v = (50.0 + w_temp * w_temp + 0.5).floor() as u16;
    // Also: let stem_v = (10.0 + 220. * ((face.weight().to_number() as f32 - 50.0) / 900.0)).floor() as u16;
    stem_v
}

pub fn get_pdf_font_bbox(face: &Face) -> [i16; 4] {
    let gbbox = face.global_bounding_box();
    [gbbox.x_min, gbbox.y_min, gbbox.x_max, gbbox.y_max]
}

pub fn get_pdf_font_file_key(face: &Face) -> String {
    if face.raw_face().table(ttf_parser::Tag::from_bytes(b"CFF ")).is_some() {
        "FontFile".to_string()
    } else if face.raw_face().table(ttf_parser::Tag::from_bytes(b"glyf")).is_some() {
        "FontFile2".to_string()
    } else {
        println!("Font file type not recognized!!!");
        "FontFile".to_string()
    }
}

pub fn get_pdf_font_subtype(face: &Face) -> String {
    if face.raw_face().table(ttf_parser::Tag::from_bytes(b"CFF ")).is_some() {
        "Type1".to_string()
    } else if face.raw_face().table(ttf_parser::Tag::from_bytes(b"glyf")).is_some() {
        "TrueType".to_string()
    } else {
        println!("Font file type not recognized!!!");
        "Type1".to_string()
    }
}

#[allow(dead_code)]
pub fn get_pdf_font_info_of_path(path: &Path) -> Result<(FontPdfInfo, FontDescriptorPdfInfo), PdfMergeError> {
    let font_data = fs::read(path)?;
    get_pdf_font_info_of_data(&font_data)
}

pub fn get_pdf_font_info_of_data(font_data: &[u8]) -> Result<(FontPdfInfo, FontDescriptorPdfInfo), PdfMergeError> {
    let face = Face::parse(&font_data, 0)?;
    Ok(get_pdf_info_of_face(&face))
}

pub fn get_pdf_info_of_face(face: &Face) -> (FontPdfInfo, FontDescriptorPdfInfo) {
    const FIRST_CHAR: u8 = 32;
    const LAST_CHAR: u8 = 255;
    (
        FontPdfInfo {
            base_font: get_name(face, name_id::POST_SCRIPT_NAME).into(),
            encoding: "MacRomanEncoding".to_string(),
            first_char: FIRST_CHAR.into(),
            last_char: LAST_CHAR.into(),
            widths: get_font_widths(face, FIRST_CHAR, LAST_CHAR),
            subtype: get_pdf_font_subtype(face),
        },
        FontDescriptorPdfInfo {
            font_name: get_name(face, name_id::POST_SCRIPT_NAME).into(),
            flags: compute_pdf_font_flags(face),
            font_bbox: get_pdf_font_bbox(face),
            italic_angle: face.italic_angle().round() as i16,
            ascent: face.ascender(),
            descent: face.descender(),
            leading: face.line_gap(),
            x_height: face.x_height().unwrap_or(0),
            stem_v: guess_pdf_stem_v_for_font(face),
            cap_height: face.capital_height().unwrap_or(0),
            font_file_key: get_pdf_font_file_key(face),
            //embedded_font_subtype: get_pdf_font_subtype(face),
        }
    )
}
