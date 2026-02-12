// src/spec_types.rs
// CLI argument spec types moved from main.rs for testability

use clap::ValueEnum;
use std::path::PathBuf;
use std::str::FromStr;
use medpdf::{HAlign, PdfColor, Unit, VAlign};

/// CLI wrapper for Unit that implements ValueEnum for clap
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliUnit {
    Pt,
    In,
    Mm,
    Cm,
}

impl From<CliUnit> for Unit {
    fn from(u: CliUnit) -> Unit {
        match u {
            CliUnit::Pt => Unit::Pt,
            CliUnit::In => Unit::In,
            CliUnit::Mm => Unit::Mm,
            CliUnit::Cm => Unit::Cm,
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
    pub layer_over: bool,
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
/// Commas inside double-quoted regions are treated as literal (not delimiters).
/// Quote characters are preserved in the output so callers can detect and strip them.
fn split_escaped_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            in_quotes = !in_quotes;
            current.push(c);
        } else if c == '\\' && !in_quotes {
            if let Some(&',') = chars.peek() {
                current.push(',');
                chars.next();
                continue;
            }
            current.push(c);
        } else if c == ',' && !in_quotes {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

/// Processes escape sequences in text values:
/// - `\uXXXX` → Unicode character (4 hex digits, BMP)
/// - `\U{XXXXX}` → Unicode character (1-6 hex digits, full range)
/// - `\n` → newline, `\t` → tab
/// - `\\` → literal backslash
/// - Any other `\X` → left as-is (backward compatible)
fn unescape_text(s: &str) -> Result<String, String> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.peek() {
            Some(&'n') => { chars.next(); result.push('\n'); }
            Some(&'t') => { chars.next(); result.push('\t'); }
            Some(&'\\') => { chars.next(); result.push('\\'); }
            Some(&'u') => {
                chars.next(); // consume 'u'
                let mut hex = String::with_capacity(4);
                for _ in 0..4 {
                    match chars.next() {
                        Some(h) if h.is_ascii_hexdigit() => hex.push(h),
                        Some(h) => return Err(format!("Invalid Unicode escape: \\u{hex}{h} (expected 4 hex digits)")),
                        None => return Err(format!("Incomplete Unicode escape: \\u{hex} (expected 4 hex digits)")),
                    }
                }
                let code = u32::from_str_radix(&hex, 16)
                    .map_err(|_| format!("Invalid hex in Unicode escape: \\u{hex}"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| format!("Invalid Unicode code point: \\u{hex}"))?;
                result.push(ch);
            }
            Some(&'U') => {
                chars.next(); // consume 'U'
                match chars.next() {
                    Some('{') => {}
                    _ => { result.push('\\'); result.push('U'); continue; }
                }
                let mut hex = String::with_capacity(6);
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(h) if h.is_ascii_hexdigit() && hex.len() < 6 => hex.push(h),
                        Some(h) if h.is_ascii_hexdigit() => return Err(format!("Unicode escape too long: \\U{{{hex}{h}... (max 6 hex digits)")),
                        Some(h) => return Err(format!("Invalid character in Unicode escape: \\U{{{hex}{h}")),
                        None => return Err(format!("Unclosed Unicode escape: \\U{{{hex}")),
                    }
                }
                if hex.is_empty() {
                    return Err("Empty Unicode escape: \\U{}".to_string());
                }
                let code = u32::from_str_radix(&hex, 16)
                    .map_err(|_| format!("Invalid hex in Unicode escape: \\U{{{hex}}}"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| format!("Invalid Unicode code point: \\U{{{hex}}}"))?;
                result.push(ch);
            }
            _ => {
                // Unknown escape — preserve as-is
                result.push('\\');
            }
        }
    }
    Ok(result)
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
        let mut layer = None;
        for part in split_escaped_commas(s) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "text" => {
                    let v = value.strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(value);
                    text = Some(unescape_text(v)?);
                }
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
                    "cap_top" => VAlign::CapTop,
                    "center" => VAlign::Center,
                    "baseline" => VAlign::Baseline,
                    "descent_bottom" => VAlign::DescentBottom,
                    "bottom" => VAlign::Bottom,
                    _ => return Err(format!("Invalid v_align value: '{}'. Use top, cap_top, center, baseline, descent_bottom, or bottom.", value)),
                }),
                "strikeout" => strikeout = Some(value.parse::<bool>().map_err(|_| format!("Invalid strikeout value: '{}'. Use true or false.", value))?),
                "underline" => underline = Some(value.parse::<bool>().map_err(|_| format!("Invalid underline value: '{}'. Use true or false.", value))?),
                "layer" => layer = Some(match value {
                    "over" => true,
                    "under" => false,
                    _ => return Err(format!("Invalid layer value: '{}'. Use over or under.", value)),
                }),
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
            layer_over: layer.unwrap_or(true),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OverlaySpec {
    pub file: PathBuf,
    pub src_page: u32,
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
                "src_page" => from_page = Some(value.parse::<u32>().map_err(|_| format!("Invalid src_page value: '{}'", value))?),
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
    pub pages: u32,
}

impl FromStr for PadToSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pages = s.parse::<u32>().map_err(|e| e.to_string())?;
        if pages == 0 {
            return Err("pad-to value must be greater than 0".to_string());
        }
        Ok(PadToSpec { pages })
    }
}

#[derive(Debug, Clone)]
pub struct PadFileSpec {
    pub file: PathBuf,
    pub page: u32,
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
                "page" => page = Some(value.parse::<u32>().map_err(|e| e.to_string())?),
                _ => return Err(format!("Unknown pad-file key: '{key}'")),
            }
        }
        Ok(PadFileSpec {
            file: file.ok_or("pad-file 'file' is required")?,
            page: page.unwrap_or(1),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BlankPageSpec {
    pub width: f32,
    pub height: f32,
    pub count: u32,
}

impl FromStr for BlankPageSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        // Check for named sizes (no '=' means it's a name, not key-value pairs)
        if !trimmed.contains('=') {
            return match trimmed.to_lowercase().as_str() {
                "letter" => Ok(BlankPageSpec { width: 612.0, height: 792.0, count: 1 }),
                "a4" => Ok(BlankPageSpec { width: 595.28, height: 841.89, count: 1 }),
                "legal" => Ok(BlankPageSpec { width: 612.0, height: 1008.0, count: 1 }),
                _ => Err(format!("Unknown page size: '{}'. Use letter, a4, legal, or w=...,h=...", trimmed)),
            };
        }

        let mut w = None;
        let mut h = None;
        let mut units = None;
        let mut count = None;
        for part in split_escaped_commas(trimmed) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "w" => w = Some(value.parse::<f32>().map_err(|_| format!("Invalid w value: '{}'", value))?),
                "h" => h = Some(value.parse::<f32>().map_err(|_| format!("Invalid h value: '{}'", value))?),
                "units" => units = Some(CliUnit::from_str(value, true).map_err(|e| e.to_string())?),
                "count" => count = Some(value.parse::<u32>().map_err(|_| format!("Invalid count value: '{}'", value))?),
                _ => return Err(format!("Unknown blank-page key: '{}'", key)),
            }
        }

        let unit: Unit = units.map(Unit::from).unwrap_or(Unit::Pt);
        let w = unit.to_points(w.ok_or("blank-page 'w' is required")?);
        let h = unit.to_points(h.ok_or("blank-page 'h' is required")?);
        let count = count.unwrap_or(1);
        if count == 0 {
            return Err("blank-page 'count' must be greater than 0".to_string());
        }

        Ok(BlankPageSpec { width: w, height: h, count })
    }
}

#[derive(Debug, Clone)]
pub struct DrawRectSpec {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: PdfColor,
    pub pages: String,
    pub layer_over: bool,
}

impl FromStr for DrawRectSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut x = None;
        let mut y = None;
        let mut w = None;
        let mut h = None;
        let mut color = None;
        let mut alpha = None;
        let mut pages = None;
        let mut units = None;
        let mut layer = None;
        for part in split_escaped_commas(s) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "x" => x = Some(value.parse::<f32>().map_err(|_| format!("Invalid x value: '{}'", value))?),
                "y" => y = Some(value.parse::<f32>().map_err(|_| format!("Invalid y value: '{}'", value))?),
                "w" => w = Some(value.parse::<f32>().map_err(|_| format!("Invalid w value: '{}'", value))?),
                "h" => h = Some(value.parse::<f32>().map_err(|_| format!("Invalid h value: '{}'", value))?),
                "color" => color = Some(parse_color(value)?),
                "alpha" => alpha = Some(value.parse::<f32>().map_err(|_| format!("Invalid alpha value: '{}'", value))?),
                "pages" => pages = Some(value.to_string()),
                "units" => units = Some(CliUnit::from_str(value, true).map_err(|e| e.to_string())?),
                "layer" => layer = Some(match value {
                    "over" => true,
                    "under" => false,
                    _ => return Err(format!("Invalid layer value: '{}'. Use over or under.", value)),
                }),
                _ => return Err(format!("Unknown draw-rect key: '{}'", key)),
            }
        }

        let mut final_color = color.unwrap_or(PdfColor::BLACK);
        if let Some(a) = alpha {
            final_color.a = a;
        }

        let unit = units.map(Unit::from).unwrap_or(Unit::Pt);
        Ok(DrawRectSpec {
            x: unit.to_points(x.ok_or("draw-rect 'x' is required")?),
            y: unit.to_points(y.ok_or("draw-rect 'y' is required")?),
            w: unit.to_points(w.ok_or("draw-rect 'w' is required")?),
            h: unit.to_points(h.ok_or("draw-rect 'h' is required")?),
            color: final_color,
            pages: pages.unwrap_or_else(|| "all".to_string()),
            layer_over: layer.unwrap_or(true),
        })
    }
}

#[derive(Debug, Clone)]
pub struct DrawLineSpec {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub width: f32,
    pub color: PdfColor,
    pub pages: String,
    pub layer_over: bool,
}

impl FromStr for DrawLineSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut x1 = None;
        let mut y1 = None;
        let mut x2 = None;
        let mut y2 = None;
        let mut width = None;
        let mut color = None;
        let mut alpha = None;
        let mut pages = None;
        let mut units = None;
        let mut layer = None;
        for part in split_escaped_commas(s) {
            let (key, value) = part.split_once('=')
                .ok_or_else(|| format!("Invalid key-value pair: '{}'. Expected 'key=value'.", part))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "x1" => x1 = Some(value.parse::<f32>().map_err(|_| format!("Invalid x1 value: '{}'", value))?),
                "y1" => y1 = Some(value.parse::<f32>().map_err(|_| format!("Invalid y1 value: '{}'", value))?),
                "x2" => x2 = Some(value.parse::<f32>().map_err(|_| format!("Invalid x2 value: '{}'", value))?),
                "y2" => y2 = Some(value.parse::<f32>().map_err(|_| format!("Invalid y2 value: '{}'", value))?),
                "width" => width = Some(value.parse::<f32>().map_err(|_| format!("Invalid width value: '{}'", value))?),
                "color" => color = Some(parse_color(value)?),
                "alpha" => alpha = Some(value.parse::<f32>().map_err(|_| format!("Invalid alpha value: '{}'", value))?),
                "pages" => pages = Some(value.to_string()),
                "units" => units = Some(CliUnit::from_str(value, true).map_err(|e| e.to_string())?),
                "layer" => layer = Some(match value {
                    "over" => true,
                    "under" => false,
                    _ => return Err(format!("Invalid layer value: '{}'. Use over or under.", value)),
                }),
                _ => return Err(format!("Unknown draw-line key: '{}'", key)),
            }
        }

        let mut final_color = color.unwrap_or(PdfColor::BLACK);
        if let Some(a) = alpha {
            final_color.a = a;
        }

        let unit = units.map(Unit::from).unwrap_or(Unit::Pt);
        Ok(DrawLineSpec {
            x1: unit.to_points(x1.ok_or("draw-line 'x1' is required")?),
            y1: unit.to_points(y1.ok_or("draw-line 'y1' is required")?),
            x2: unit.to_points(x2.ok_or("draw-line 'x2' is required")?),
            y2: unit.to_points(y2.ok_or("draw-line 'y2' is required")?),
            width: width.unwrap_or(1.0),
            color: final_color,
            pages: pages.unwrap_or_else(|| "all".to_string()),
            layer_over: layer.unwrap_or(true),
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

    #[test]
    fn test_pad_to_spec_zero() {
        assert!(PadToSpec::from_str("0").is_err());
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

    // --- split_escaped_commas edge cases ---

    #[test]
    fn test_split_escaped_commas_no_escapes() {
        let parts = split_escaped_commas("a=1,b=2,c=3");
        assert_eq!(parts, vec!["a=1", "b=2", "c=3"]);
    }

    #[test]
    fn test_split_escaped_commas_single_part() {
        let parts = split_escaped_commas("text=hello");
        assert_eq!(parts, vec!["text=hello"]);
    }

    #[test]
    fn test_split_escaped_commas_empty_string() {
        let parts = split_escaped_commas("");
        assert_eq!(parts, vec![""]);
    }

    #[test]
    fn test_split_escaped_commas_multiple_escapes() {
        let parts = split_escaped_commas(r"a\,b\,c,d");
        assert_eq!(parts, vec!["a,b,c", "d"]);
    }

    #[test]
    fn test_split_escaped_commas_trailing_backslash() {
        // A trailing backslash not followed by comma is preserved
        let parts = split_escaped_commas(r"text=hello\");
        assert_eq!(parts, vec!["text=hello\\"]);
    }

    #[test]
    fn test_split_escaped_commas_backslash_not_before_comma() {
        // Backslash not followed by comma is preserved as-is
        let parts = split_escaped_commas(r"path=C:\Users\test,key=val");
        assert_eq!(parts, vec![r"path=C:\Users\test", "key=val"]);
    }

    #[test]
    fn test_split_escaped_commas_consecutive_commas() {
        let parts = split_escaped_commas("a,,b");
        assert_eq!(parts, vec!["a", "", "b"]);
    }

    // --- parse_color edge cases ---

    #[test]
    fn test_parse_color_gray_alias() {
        let c = parse_color("gray").unwrap();
        assert!((c.r - 0.5).abs() < f32::EPSILON);
        assert!((c.g - 0.5).abs() < f32::EPSILON);
        assert!((c.b - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_color_grey_alias() {
        let c = parse_color("grey").unwrap();
        assert!((c.r - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_color_case_insensitive() {
        let c1 = parse_color("RED").unwrap();
        let c2 = parse_color("Red").unwrap();
        let c3 = parse_color("red").unwrap();
        assert_eq!(c1, c2);
        assert_eq!(c2, c3);
    }

    #[test]
    fn test_parse_color_hex_without_hash() {
        let c = parse_color("FF0000").unwrap();
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!((c.g - 0.0).abs() < f32::EPSILON);
        assert!((c.b - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_color_rrggbbaa() {
        let c = parse_color("#FF000080").unwrap();
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!((c.a - 128.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_color_invalid_hex() {
        let result = parse_color("#GG0000");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_color_invalid_length() {
        // 5 hex chars - not 3, 6, or 8
        let result = parse_color("#12345");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid color value"));
    }

    #[test]
    fn test_parse_color_unknown_name() {
        // "purple" is not a named color in the parser
        let result = parse_color("purple");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_color_black() {
        let c = parse_color("black").unwrap();
        assert_eq!(c, PdfColor::BLACK);
    }

    #[test]
    fn test_parse_color_white() {
        let c = parse_color("white").unwrap();
        assert_eq!(c, PdfColor::WHITE);
    }

    #[test]
    fn test_parse_color_blue() {
        let c = parse_color("blue").unwrap();
        assert!((c.b - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_color_green() {
        let c = parse_color("green").unwrap();
        assert!((c.g - 0.5).abs() < f32::EPSILON);
        assert!((c.r - 0.0).abs() < f32::EPSILON);
    }

    // --- Alpha override behavior ---

    #[test]
    fn test_watermark_alpha_overrides_hex_alpha() {
        // Color has alpha from RRGGBBAA, but explicit alpha= should override
        let spec = WatermarkSpec::from_str(
            "text=X,font=@H,x=0,y=0,color=#FF0000FF,alpha=0.3"
        ).unwrap();
        // alpha=0.3 should override the FF (1.0) from color
        assert!((spec.color.a - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_watermark_color_without_alpha_defaults_opaque() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,color=#00FF00").unwrap();
        assert!((spec.color.a - 1.0).abs() < f32::EPSILON);
    }

    // --- Missing required fields ---

    #[test]
    fn test_watermark_spec_missing_x() {
        let result = WatermarkSpec::from_str("text=DRAFT,font=@H,y=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x"));
    }

    #[test]
    fn test_watermark_spec_missing_y() {
        let result = WatermarkSpec::from_str("text=DRAFT,font=@H,x=1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("y"));
    }

    // --- Invalid numeric values ---

    #[test]
    fn test_watermark_spec_invalid_size() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,size=abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_watermark_spec_invalid_x() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=notanumber,y=0");
        assert!(result.is_err());
    }

    #[test]
    fn test_watermark_spec_invalid_alpha() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,alpha=nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_watermark_spec_invalid_rotation() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,rotation=xyz");
        assert!(result.is_err());
    }

    // --- Invalid enum values ---

    #[test]
    fn test_watermark_spec_invalid_h_align() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,h_align=middle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("h_align"));
    }

    #[test]
    fn test_watermark_spec_invalid_v_align() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,v_align=middle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("v_align"));
    }

    #[test]
    fn test_watermark_spec_invalid_strikeout() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,strikeout=yes");
        assert!(result.is_err());
    }

    #[test]
    fn test_watermark_spec_invalid_underline() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,underline=1");
        assert!(result.is_err());
    }

    // --- OverlaySpec edge cases ---

    #[test]
    fn test_overlay_spec_invalid_src_page_value() {
        let result = OverlaySpec::from_str("file=f.pdf,src_page=abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_overlay_spec_empty_string() {
        let result = OverlaySpec::from_str("");
        assert!(result.is_err());
    }

    // --- PadToSpec edge cases ---

    #[test]
    fn test_pad_to_spec_large_value() {
        let spec = PadToSpec::from_str("1000").unwrap();
        assert_eq!(spec.pages, 1000);
    }

    #[test]
    fn test_pad_to_spec_one() {
        let spec = PadToSpec::from_str("1").unwrap();
        assert_eq!(spec.pages, 1);
    }

    #[test]
    fn test_pad_to_spec_float_fails() {
        assert!(PadToSpec::from_str("1.5").is_err());
    }

    #[test]
    fn test_pad_to_spec_empty_fails() {
        assert!(PadToSpec::from_str("").is_err());
    }

    // --- PadFileSpec edge cases ---

    #[test]
    fn test_pad_file_spec_unknown_key() {
        let result = PadFileSpec::from_str("file=f.pdf,bogus=val");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bogus"));
    }

    #[test]
    fn test_pad_file_spec_invalid_page() {
        let result = PadFileSpec::from_str("file=f.pdf,page=abc");
        assert!(result.is_err());
    }

    // --- WatermarkSpec with whitespace in values ---

    #[test]
    fn test_watermark_spec_whitespace_in_key_value() {
        // Keys and values are trimmed
        let spec = WatermarkSpec::from_str("text = DRAFT , font = @Helvetica , x = 1 , y = 1").unwrap();
        assert_eq!(spec.text, "DRAFT");
        assert_eq!(spec.font, PathBuf::from("@Helvetica"));
    }

    // --- Default units ---

    #[test]
    fn test_watermark_spec_default_units_is_inches() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=1,y=1").unwrap();
        assert_eq!(spec.units, Unit::In);
    }

    #[test]
    fn test_watermark_spec_units_mm() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=1,y=1,units=mm").unwrap();
        assert_eq!(spec.units, Unit::Mm);
    }

    // --- Default strikeout/underline ---

    #[test]
    fn test_watermark_spec_default_decorations_false() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0").unwrap();
        assert!(!spec.strikeout);
        assert!(!spec.underline);
    }

    // --- WatermarkSpec layer ---

    #[test]
    fn test_watermark_spec_default_layer_over() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0").unwrap();
        assert!(spec.layer_over);
    }

    #[test]
    fn test_watermark_spec_layer_under() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,layer=under").unwrap();
        assert!(!spec.layer_over);
    }

    #[test]
    fn test_watermark_spec_layer_over_explicit() {
        let spec = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,layer=over").unwrap();
        assert!(spec.layer_over);
    }

    #[test]
    fn test_watermark_spec_layer_invalid() {
        let result = WatermarkSpec::from_str("text=X,font=@H,x=0,y=0,layer=middle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("layer"));
    }

    // --- DrawRectSpec ---

    #[test]
    fn test_draw_rect_spec_minimal() {
        let spec = DrawRectSpec::from_str("x=30,y=680,w=550,h=0.5").unwrap();
        assert!((spec.x - 30.0).abs() < f32::EPSILON);
        assert!((spec.y - 680.0).abs() < f32::EPSILON);
        assert!((spec.w - 550.0).abs() < f32::EPSILON);
        assert!((spec.h - 0.5).abs() < f32::EPSILON);
        assert_eq!(spec.color, PdfColor::BLACK); // default
        assert_eq!(spec.pages, "all"); // default
        assert!(spec.layer_over); // default
    }

    #[test]
    fn test_draw_rect_spec_full() {
        let spec = DrawRectSpec::from_str("x=1,y=2,w=3,h=4,color=red,pages=1-3,units=in,layer=under").unwrap();
        assert!((spec.x - 72.0).abs() < f32::EPSILON); // 1 inch = 72pt
        assert!((spec.y - 144.0).abs() < f32::EPSILON);
        assert!((spec.w - 216.0).abs() < f32::EPSILON);
        assert!((spec.h - 288.0).abs() < f32::EPSILON);
        assert_eq!(spec.color, PdfColor::RED);
        assert_eq!(spec.pages, "1-3");
        assert!(!spec.layer_over);
    }

    #[test]
    fn test_draw_rect_spec_with_alpha() {
        let spec = DrawRectSpec::from_str("x=0,y=0,w=10,h=10,color=#FF0000,alpha=0.5").unwrap();
        assert!((spec.color.a - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_draw_rect_spec_missing_x() {
        let result = DrawRectSpec::from_str("y=0,w=10,h=10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x"));
    }

    #[test]
    fn test_draw_rect_spec_missing_w() {
        let result = DrawRectSpec::from_str("x=0,y=0,h=10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("w"));
    }

    #[test]
    fn test_draw_rect_spec_unknown_key() {
        let result = DrawRectSpec::from_str("x=0,y=0,w=10,h=10,bogus=val");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bogus"));
    }

    #[test]
    fn test_draw_rect_spec_invalid_value() {
        let result = DrawRectSpec::from_str("x=abc,y=0,w=10,h=10");
        assert!(result.is_err());
    }

    // --- DrawLineSpec ---

    #[test]
    fn test_draw_line_spec_minimal() {
        let spec = DrawLineSpec::from_str("x1=100,y1=200,x2=400,y2=500").unwrap();
        assert!((spec.x1 - 100.0).abs() < f32::EPSILON);
        assert!((spec.y1 - 200.0).abs() < f32::EPSILON);
        assert!((spec.x2 - 400.0).abs() < f32::EPSILON);
        assert!((spec.y2 - 500.0).abs() < f32::EPSILON);
        assert!((spec.width - 1.0).abs() < f32::EPSILON); // default
        assert_eq!(spec.color, PdfColor::BLACK); // default
        assert_eq!(spec.pages, "all"); // default
        assert!(spec.layer_over); // default
    }

    #[test]
    fn test_draw_line_spec_full() {
        let spec = DrawLineSpec::from_str("x1=1,y1=2,x2=3,y2=4,width=2.5,color=blue,pages=1,units=in,layer=under").unwrap();
        assert!((spec.x1 - 72.0).abs() < f32::EPSILON);
        assert!((spec.y1 - 144.0).abs() < f32::EPSILON);
        assert!((spec.x2 - 216.0).abs() < f32::EPSILON);
        assert!((spec.y2 - 288.0).abs() < f32::EPSILON);
        assert!((spec.width - 2.5).abs() < f32::EPSILON);
        assert_eq!(spec.pages, "1");
        assert!(!spec.layer_over);
    }

    #[test]
    fn test_draw_line_spec_with_alpha() {
        let spec = DrawLineSpec::from_str("x1=0,y1=0,x2=10,y2=10,alpha=0.3").unwrap();
        assert!((spec.color.a - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_draw_line_spec_missing_x1() {
        let result = DrawLineSpec::from_str("y1=0,x2=10,y2=10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x1"));
    }

    #[test]
    fn test_draw_line_spec_missing_y2() {
        let result = DrawLineSpec::from_str("x1=0,y1=0,x2=10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("y2"));
    }

    #[test]
    fn test_draw_line_spec_unknown_key() {
        let result = DrawLineSpec::from_str("x1=0,y1=0,x2=10,y2=10,bogus=val");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bogus"));
    }

    #[test]
    fn test_draw_line_spec_invalid_value() {
        let result = DrawLineSpec::from_str("x1=abc,y1=0,x2=10,y2=10");
        assert!(result.is_err());
    }

    // --- BlankPageSpec ---

    #[test]
    fn test_blank_page_spec_letter() {
        let spec = BlankPageSpec::from_str("letter").unwrap();
        assert!((spec.width - 612.0).abs() < f32::EPSILON);
        assert!((spec.height - 792.0).abs() < f32::EPSILON);
        assert_eq!(spec.count, 1);
    }

    #[test]
    fn test_blank_page_spec_a4() {
        let spec = BlankPageSpec::from_str("a4").unwrap();
        assert!((spec.width - 595.28).abs() < 0.01);
        assert!((spec.height - 841.89).abs() < 0.01);
    }

    #[test]
    fn test_blank_page_spec_legal() {
        let spec = BlankPageSpec::from_str("legal").unwrap();
        assert!((spec.width - 612.0).abs() < f32::EPSILON);
        assert!((spec.height - 1008.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_blank_page_spec_case_insensitive() {
        let spec = BlankPageSpec::from_str("LETTER").unwrap();
        assert!((spec.width - 612.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_blank_page_spec_explicit_dims() {
        let spec = BlankPageSpec::from_str("w=612,h=792").unwrap();
        assert!((spec.width - 612.0).abs() < f32::EPSILON);
        assert!((spec.height - 792.0).abs() < f32::EPSILON);
        assert_eq!(spec.count, 1);
    }

    #[test]
    fn test_blank_page_spec_with_units() {
        let spec = BlankPageSpec::from_str("w=8.5,h=11,units=in").unwrap();
        assert!((spec.width - 612.0).abs() < f32::EPSILON);
        assert!((spec.height - 792.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_blank_page_spec_with_count() {
        let spec = BlankPageSpec::from_str("w=100,h=100,count=3").unwrap();
        assert_eq!(spec.count, 3);
    }

    #[test]
    fn test_blank_page_spec_missing_w() {
        let result = BlankPageSpec::from_str("h=792");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("w"));
    }

    #[test]
    fn test_blank_page_spec_missing_h() {
        let result = BlankPageSpec::from_str("w=612");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("h"));
    }

    #[test]
    fn test_blank_page_spec_unknown_name() {
        let result = BlankPageSpec::from_str("tabloid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("tabloid"));
    }

    #[test]
    fn test_blank_page_spec_unknown_key() {
        let result = BlankPageSpec::from_str("w=100,h=100,bogus=val");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bogus"));
    }

    #[test]
    fn test_blank_page_spec_count_zero() {
        let result = BlankPageSpec::from_str("w=100,h=100,count=0");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("count"));
    }

    #[test]
    fn test_blank_page_spec_invalid_w() {
        let result = BlankPageSpec::from_str("w=abc,h=100");
        assert!(result.is_err());
    }

    // --- unescape_text ---

    #[test]
    fn test_unescape_text_ascii() {
        assert_eq!(unescape_text(r"\u0041").unwrap(), "A");
    }

    #[test]
    fn test_unescape_text_em_dash() {
        assert_eq!(unescape_text(r"\u2014").unwrap(), "\u{2014}");
    }

    #[test]
    fn test_unescape_text_accented() {
        assert_eq!(unescape_text(r"\u00E9").unwrap(), "é");
    }

    #[test]
    fn test_unescape_text_full_range() {
        assert_eq!(unescape_text(r"\U{1F600}").unwrap(), "\u{1F600}");
    }

    #[test]
    fn test_unescape_text_newline_tab() {
        assert_eq!(unescape_text(r"\n").unwrap(), "\n");
        assert_eq!(unescape_text(r"\t").unwrap(), "\t");
    }

    #[test]
    fn test_unescape_text_literal_backslash() {
        assert_eq!(unescape_text(r"\\").unwrap(), "\\");
    }

    #[test]
    fn test_unescape_text_unknown_escape_passthrough() {
        assert_eq!(unescape_text(r"\q").unwrap(), r"\q");
    }

    #[test]
    fn test_unescape_text_invalid_hex() {
        let result = unescape_text(r"\u00GG");
        assert!(result.is_err());
    }

    #[test]
    fn test_unescape_text_incomplete() {
        let result = unescape_text(r"\u00");
        assert!(result.is_err());
    }

    #[test]
    fn test_unescape_text_empty_u_braces() {
        let result = unescape_text(r"\U{}");
        assert!(result.is_err());
    }

    #[test]
    fn test_unescape_text_mixed() {
        assert_eq!(
            unescape_text(r"Hello \u2014 World \U{1F600}").unwrap(),
            "Hello \u{2014} World \u{1F600}"
        );
    }

    // --- split_escaped_commas with quotes ---

    #[test]
    fn test_split_escaped_commas_quoted() {
        let parts = split_escaped_commas(r#"text="Hello, World",x=1"#);
        assert_eq!(parts, vec![r#"text="Hello, World""#, "x=1"]);
    }

    #[test]
    fn test_split_escaped_commas_mixed_quoted() {
        let parts = split_escaped_commas(r#"text="a,b",font=@H,x=1,y=1"#);
        assert_eq!(parts, vec![r#"text="a,b""#, "font=@H", "x=1", "y=1"]);
    }

    #[test]
    fn test_split_escaped_commas_unclosed_quote() {
        // Unclosed quote — rest of string included
        let parts = split_escaped_commas(r#"text="hello,x=1"#);
        assert_eq!(parts, vec![r#"text="hello,x=1"#]);
    }

    // --- WatermarkSpec with quotes and unicode ---

    #[test]
    fn test_watermark_spec_quoted_text() {
        let spec = WatermarkSpec::from_str(r#"text="Hello, World",font=@Helvetica,x=1,y=1"#).unwrap();
        assert_eq!(spec.text, "Hello, World");
    }

    #[test]
    fn test_watermark_spec_unicode_text() {
        let spec = WatermarkSpec::from_str(r"text=em dash: \u2014,font=@Helvetica,x=1,y=1").unwrap();
        assert_eq!(spec.text, "em dash: \u{2014}");
    }

    #[test]
    fn test_watermark_spec_quoted_plus_unicode() {
        let spec = WatermarkSpec::from_str(r#"text="curly: \u201C\u201D",font=@Helvetica,x=1,y=1"#).unwrap();
        assert_eq!(spec.text, "curly: \u{201C}\u{201D}");
    }
}
