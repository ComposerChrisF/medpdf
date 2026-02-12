// src/spec_types.rs
// CLI argument spec types moved from main.rs for testability

use clap::ValueEnum;
use std::path::PathBuf;
use std::str::FromStr;
use medpdf::{HAlign, PdfColor, Unit, VAlign};

/// CLI wrapper for Unit that implements ValueEnum for clap
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliUnit {
    In,
    Mm,
}

impl From<CliUnit> for Unit {
    fn from(u: CliUnit) -> Unit {
        match u {
            CliUnit::In => Unit::In,
            CliUnit::Mm => Unit::Mm,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatermarkSpec {
    pub text: String,
    pub font: PathBuf,
    pub size: f32,
    pub x: f32,
    pub y: f32,
    pub units: Unit,
    pub pages: String,
    pub color: PdfColor,
    pub rotation: f32,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub strikeout: bool,
    pub underline: bool,
}

/// Parses a color string into a `PdfColor`.
///
/// Supports named colors (`black`, `white`, `red`, `blue`, `green`, `gray`/`grey`)
/// and hex formats (`#RGB`, `#RRGGBB`, `#RRGGBBAA`) with or without the `#` prefix.
fn parse_color(s: &str) -> Result<PdfColor, String> {
    match s.to_lowercase().as_str() {
        "black" => return Ok(PdfColor::BLACK),
        "white" => return Ok(PdfColor::WHITE),
        "red" => return Ok(PdfColor::RED),
        "blue" => return Ok(PdfColor::rgb(0.0, 0.0, 1.0)),
        "green" => return Ok(PdfColor::rgb(0.0, 0.5, 0.0)),
        "gray" | "grey" => return Ok(PdfColor::rgb(0.5, 0.5, 0.5)),
        _ => {}
    }

    let hex = s.strip_prefix('#').unwrap_or(s);
    let parse_hex = |h: &str| u8::from_str_radix(h, 16).map_err(|_| format!("Invalid hex color: '{s}'"));

    match hex.len() {
        3 => {
            let r = parse_hex(&hex[0..1])? * 17;
            let g = parse_hex(&hex[1..2])? * 17;
            let b = parse_hex(&hex[2..3])? * 17;
            Ok(PdfColor::from_rgb8(r, g, b))
        }
        6 => {
            let r = parse_hex(&hex[0..2])?;
            let g = parse_hex(&hex[2..4])?;
            let b = parse_hex(&hex[4..6])?;
            Ok(PdfColor::from_rgb8(r, g, b))
        }
        8 => {
            let r = parse_hex(&hex[0..2])?;
            let g = parse_hex(&hex[2..4])?;
            let b = parse_hex(&hex[4..6])?;
            let a = parse_hex(&hex[6..8])?;
            Ok(PdfColor::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0))
        }
        _ => Err(format!("Invalid color value: '{s}'. Use a named color or hex (#RGB, #RRGGBB, #RRGGBBAA).")),
    }
}

/// Splits a string by commas, treating `\,` as an escaped literal comma.
fn split_escaped_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&',') = chars.peek() {
                current.push(',');
                chars.next();
                continue;
            }
            current.push(c);
        } else if c == ',' {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

impl FromStr for WatermarkSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut text = None;
        let mut font = None;
        let mut size = None;
        let mut x = None;
        let mut y = None;
        let mut units = None;
        let mut pages = None;
        let mut color = None;
        let mut alpha = None;
        let mut rotation = None;
        let mut h_align = None;
        let mut v_align = None;
        let mut strikeout = None;
        let mut underline = None;
        for part in split_escaped_commas(s) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "text" => text = Some(value.to_string()),
                "font" => font = Some(PathBuf::from(value)),
                "size" => size = Some(value.parse::<f32>().map_err(|_| format!("Invalid size value: '{}'", value))?),
                "x" => x = Some(value.parse::<f32>().map_err(|_| format!("Invalid x value: '{}'", value))?),
                "y" => y = Some(value.parse::<f32>().map_err(|_| format!("Invalid y value: '{}'", value))?),
                "units" => units = Some(CliUnit::from_str(value, true).map_err(|e| e.to_string())?),
                "pages" => pages = Some(value.to_string()),
                "color" => color = Some(parse_color(value)?),
                "alpha" => alpha = Some(value.parse::<f32>().map_err(|_| format!("Invalid alpha value: '{}'", value))?),
                "rotation" => rotation = Some(value.parse::<f32>().map_err(|_| format!("Invalid rotation value: '{}'", value))?),
                "h_align" => h_align = Some(match value {
                    "left" => HAlign::Left,
                    "center" => HAlign::Center,
                    "right" => HAlign::Right,
                    _ => return Err(format!("Invalid h_align value: '{}'. Use left, center, or right.", value)),
                }),
                "v_align" => v_align = Some(match value {
                    "top" => VAlign::Top,
                    "center" => VAlign::Center,
                    "baseline" => VAlign::Baseline,
                    "bottom" => VAlign::Bottom,
                    _ => return Err(format!("Invalid v_align value: '{}'. Use top, center, baseline, or bottom.", value)),
                }),
                "strikeout" => strikeout = Some(value.parse::<bool>().map_err(|_| format!("Invalid strikeout value: '{}'. Use true or false.", value))?),
                "underline" => underline = Some(value.parse::<bool>().map_err(|_| format!("Invalid underline value: '{}'. Use true or false.", value))?),
                _ => return Err(format!("Unknown watermark key: '{}'", key)),
            }
        }

        // If both color and alpha are specified, alpha overrides the color's alpha channel
        let mut final_color = color.unwrap_or(PdfColor::BLACK);
        if let Some(a) = alpha {
            final_color.a = a;
        }

        Ok(WatermarkSpec {
            text: text.ok_or("Watermark 'text' is required")?,
            font: font.ok_or("Watermark 'font' is required")?,
            size: size.unwrap_or(48.0),
            x: x.ok_or("Watermark 'x' coordinate is required")?,
            y: y.ok_or("Watermark 'y' coordinate is required")?,
            units: units.map(Unit::from).unwrap_or(Unit::In),
            pages: pages.unwrap_or_else(|| "all".to_string()),
            color: final_color,
            rotation: rotation.unwrap_or(0.0),
            h_align: h_align.unwrap_or(HAlign::Left),
            v_align: v_align.unwrap_or(VAlign::Baseline),
            strikeout: strikeout.unwrap_or(false),
            underline: underline.unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OverlaySpec {
    pub file: PathBuf,
    pub src_page: u16,
    pub target_pages: String,
}

impl FromStr for OverlaySpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut file = None;
        let mut from_page = None;
        let mut pages = None;
        for part in split_escaped_commas(s) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "file" => file = Some(PathBuf::from(value)),
                "src_page" => from_page = Some(value.parse::<u16>().map_err(|_| format!("Invalid src_page value: '{}'", value))?),
                "target_pages" => pages = Some(value.to_string()),
                _ => return Err(format!("Unknown overlay key: '{}'", key)),
            }
        }
        Ok(OverlaySpec {
            file: file.ok_or("Overlay 'file' is required")?,
            src_page: from_page.ok_or("Overlay 'src_page' is required")?,
            target_pages: pages.unwrap_or_else(|| "all".to_string()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct PadToSpec {
    pub pages: u16,
}

impl FromStr for PadToSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pages = s.parse::<u16>().map_err(|e| e.to_string())?;
        Ok(PadToSpec { pages })
    }
}

#[derive(Debug, Clone)]
pub struct PadFileSpec {
    pub file: PathBuf,
    pub page: u16,
}

impl FromStr for PadFileSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut file = None;
        let mut page = None;
        for part in split_escaped_commas(s) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{part}'."))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "file" => file = Some(PathBuf::from(value)),
                "page" => page = Some(value.parse::<u16>().map_err(|e| e.to_string())?),
                _ => return Err(format!("Unknown pad-file key: '{key}'")),
            }
        }
        Ok(PadFileSpec {
            file: file.ok_or("pad-file 'file' is required")?,
            page: page.unwrap_or(1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- WatermarkSpec ---

    #[test]
    fn test_watermark_spec_minimal() {
        let spec = WatermarkSpec::from_str("text=DRAFT,font=@Helvetica,x=1,y=1").unwrap();
        assert_eq!(spec.text, "DRAFT");
        assert_eq!(spec.font, PathBuf::from("@Helvetica"));
        assert!((spec.x - 1.0).abs() < f32::EPSILON);
        assert!((spec.y - 1.0).abs() < f32::EPSILON);
        assert!((spec.size - 48.0).abs() < f32::EPSILON); // default
        assert_eq!(spec.pages, "all"); // default
        assert_eq!(spec.h_align, HAlign::Left); // default
        assert_eq!(spec.v_align, VAlign::Baseline); // default
        assert!((spec.rotation - 0.0).abs() < f32::EPSILON); // default
    }

    #[test]
    fn test_watermark_spec_full() {
        let spec = WatermarkSpec::from_str(
            "text=Hello,font=@Courier,size=24,x=2,y=3,units=mm,pages=1-3,color=#FF0000,alpha=0.5,rotation=45,h_align=center,v_align=top,strikeout=true,underline=true"
        ).unwrap();
        assert_eq!(spec.text, "Hello");
        assert!((spec.size - 24.0).abs() < f32::EPSILON);
        assert!((spec.x - 2.0).abs() < f32::EPSILON);
        assert!((spec.y - 3.0).abs() < f32::EPSILON);
        assert_eq!(spec.pages, "1-3");
        assert!((spec.color.r - 1.0).abs() < f32::EPSILON);
        assert!((spec.color.a - 0.5).abs() < f32::EPSILON);
        assert!((spec.rotation - 45.0).abs() < f32::EPSILON);
        assert_eq!(spec.h_align, HAlign::Center);
        assert_eq!(spec.v_align, VAlign::Top);
        assert!(spec.strikeout);
        assert!(spec.underline);
    }

    #[test]
    fn test_watermark_spec_missing_text() {
        let result = WatermarkSpec::from_str("font=@Helvetica,x=1,y=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("text"));
    }

    #[test]
    fn test_watermark_spec_missing_font() {
        let result = WatermarkSpec::from_str("text=DRAFT,x=1,y=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("font"));
    }

    #[test]
    fn test_watermark_spec_invalid_kv() {
        let result = WatermarkSpec::from_str("text=DRAFT,badinput,font=@Helvetica,x=1,y=1");
        assert!(result.is_err());
    }

    #[test]
    fn test_watermark_spec_unknown_key() {
        let result = WatermarkSpec::from_str("text=DRAFT,font=@Helvetica,x=1,y=1,bogus=val");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bogus"));
    }

    #[test]
    fn test_watermark_spec_color_named() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,color=red").unwrap();
        assert!((spec.color.r - 1.0).abs() < f32::EPSILON);
        assert!((spec.color.g - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_watermark_spec_color_hex_short() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,color=#F00").unwrap();
        assert!((spec.color.r - 1.0).abs() < f32::EPSILON);
        assert!((spec.color.g - 0.0).abs() < f32::EPSILON);
    }

    // --- OverlaySpec ---

    #[test]
    fn test_overlay_spec_full() {
        let spec = OverlaySpec::from_str("file=overlay.pdf,src_page=2,target_pages=1-5").unwrap();
        assert_eq!(spec.file, PathBuf::from("overlay.pdf"));
        assert_eq!(spec.src_page, 2);
        assert_eq!(spec.target_pages, "1-5");
    }

    #[test]
    fn test_overlay_spec_defaults() {
        let spec = OverlaySpec::from_str("file=overlay.pdf,src_page=1").unwrap();
        assert_eq!(spec.target_pages, "all");
    }

    #[test]
    fn test_overlay_spec_missing_file() {
        let result = OverlaySpec::from_str("src_page=1,target_pages=all");
        assert!(result.is_err());
    }

    #[test]
    fn test_overlay_spec_missing_src_page() {
        let result = OverlaySpec::from_str("file=overlay.pdf,target_pages=all");
        assert!(result.is_err());
    }

    #[test]
    fn test_overlay_spec_unknown_key() {
        let result = OverlaySpec::from_str("file=overlay.pdf,src_page=1,bogus=val");
        assert!(result.is_err());
    }

    // --- PadToSpec ---

    #[test]
    fn test_pad_to_spec_valid() {
        let spec = PadToSpec::from_str("4").unwrap();
        assert_eq!(spec.pages, 4);
    }

    #[test]
    fn test_pad_to_spec_invalid() {
        assert!(PadToSpec::from_str("abc").is_err());
        assert!(PadToSpec::from_str("-1").is_err());
    }

    // --- PadFileSpec ---

    #[test]
    fn test_pad_file_spec_full() {
        let spec = PadFileSpec::from_str("file=blank.pdf,page=3").unwrap();
        assert_eq!(spec.file, PathBuf::from("blank.pdf"));
        assert_eq!(spec.page, 3);
    }

    #[test]
    fn test_pad_file_spec_default_page() {
        let spec = PadFileSpec::from_str("file=blank.pdf").unwrap();
        assert_eq!(spec.page, 1);
    }

    #[test]
    fn test_pad_file_spec_missing_file() {
        assert!(PadFileSpec::from_str("page=1").is_err());
    }

    #[test]
    fn test_pad_file_spec_invalid_kv() {
        assert!(PadFileSpec::from_str("noequalssign").is_err());
    }

    // --- Escaped comma ---

    #[test]
    fn test_watermark_spec_escaped_comma() {
        let spec = WatermarkSpec::from_str(r"text=Hello\, World,font=@Helvetica,x=1,y=1").unwrap();
        assert_eq!(spec.text, "Hello, World");
    }
}
