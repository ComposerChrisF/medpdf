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
        for part in s.split(',') {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() != 2 { return Err(format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part)); }
            let key = kv[0].trim();
            let value = kv[1].trim();
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
        for part in s.split(',') {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() != 2 { return Err(format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part)); }
            let key = kv[0].trim();
            let value = kv[1].trim();
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
        for part in s.split(',') {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() != 2 { return Err(format!("Invalid key-value pair: '{part}'.")); }
            let key = kv[0].trim();
            let value = kv[1].trim();
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
