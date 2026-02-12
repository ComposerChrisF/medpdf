/// Rich types for the medpdf crate: color, alignment, font properties, and AddTextParams.
/// An RGBA color with components in [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    /// Alpha component (0.0 = fully transparent, 1.0 = fully opaque).
    /// Supported by `add_text_params()` via an ExtGState dictionary with `ca`/`CA`
    /// parameters. When alpha is 1.0, no ExtGState is emitted (zero overhead).
    pub a: f32,
}

impl PdfColor {
    pub const BLACK: PdfColor = PdfColor {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: PdfColor = PdfColor {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const RED: PdfColor = PdfColor {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }

    pub fn from_argb8(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }
}

impl Default for PdfColor {
    fn default() -> Self {
        Self::BLACK
    }
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    Top,
    Center,
    #[default]
    Baseline,
    Bottom,
}

/// Font weight (numeric, matching CSS/OpenType conventions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100);
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200);
    pub const LIGHT: FontWeight = FontWeight(300);
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const SEMI_BOLD: FontWeight = FontWeight(600);
    pub const BOLD: FontWeight = FontWeight(700);
    pub const EXTRA_BOLD: FontWeight = FontWeight(800);
    pub const BLACK: FontWeight = FontWeight(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Font style (normal, italic, or oblique).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique(f32),
}

/// Parameters for adding text to a PDF page with full control over rendering.
#[derive(Debug, Clone)]
pub struct AddTextParams {
    pub text: String,
    pub font_data: Vec<u8>,
    pub font_name: String,
    pub font_size: f32,
    pub x: f32,
    pub y: f32,
    pub color: PdfColor,
    pub rotation: f32,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub layer_over: bool,
    pub strikeout: bool,
    pub underline: bool,
}

impl AddTextParams {
    pub fn new(text: impl Into<String>, font_data: Vec<u8>, font_name: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_data,
            font_name: font_name.into(),
            font_size: 12.0,
            x: 0.0,
            y: 0.0,
            color: PdfColor::BLACK,
            rotation: 0.0,
            h_align: HAlign::Left,
            v_align: VAlign::Baseline,
            layer_over: true,
            strikeout: false,
            underline: false,
        }
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn color(mut self, color: PdfColor) -> Self {
        self.color = color;
        self
    }

    pub fn rotation(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }

    pub fn h_align(mut self, align: HAlign) -> Self {
        self.h_align = align;
        self
    }

    pub fn v_align(mut self, align: VAlign) -> Self {
        self.v_align = align;
        self
    }

    pub fn layer_over(mut self, over: bool) -> Self {
        self.layer_over = over;
        self
    }

    pub fn strikeout(mut self, strikeout: bool) -> Self {
        self.strikeout = strikeout;
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_color_rgb() {
        let c = PdfColor::rgb(0.5, 0.6, 0.7);
        assert!((c.r - 0.5).abs() < f32::EPSILON);
        assert!((c.a - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pdf_color_from_rgb8() {
        let c = PdfColor::from_rgb8(255, 0, 128);
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!((c.g - 0.0).abs() < f32::EPSILON);
        assert!((c.b - 128.0 / 255.0).abs() < 0.001);
    }

    #[test]
    fn test_pdf_color_constants() {
        assert_eq!(PdfColor::BLACK, PdfColor::rgb(0.0, 0.0, 0.0));
        assert_eq!(PdfColor::WHITE, PdfColor::rgb(1.0, 1.0, 1.0));
        assert_eq!(PdfColor::RED, PdfColor::rgb(1.0, 0.0, 0.0));
    }

    #[test]
    fn test_defaults() {
        assert_eq!(PdfColor::default(), PdfColor::BLACK);
        assert_eq!(HAlign::default(), HAlign::Left);
        assert_eq!(VAlign::default(), VAlign::Baseline);
        assert_eq!(FontWeight::default(), FontWeight::NORMAL);
        assert_eq!(FontStyle::default(), FontStyle::Normal);
    }

    #[test]
    fn test_add_text_params_builder() {
        let params = AddTextParams::new("Hello", vec![1, 2, 3], "TestFont")
            .font_size(24.0)
            .position(100.0, 200.0)
            .color(PdfColor::RED)
            .rotation(45.0)
            .h_align(HAlign::Center)
            .v_align(VAlign::Top)
            .layer_over(false);

        assert_eq!(params.text, "Hello");
        assert!((params.font_size - 24.0).abs() < f32::EPSILON);
        assert!((params.x - 100.0).abs() < f32::EPSILON);
        assert!((params.y - 200.0).abs() < f32::EPSILON);
        assert_eq!(params.color, PdfColor::RED);
        assert!((params.rotation - 45.0).abs() < f32::EPSILON);
        assert_eq!(params.h_align, HAlign::Center);
        assert_eq!(params.v_align, VAlign::Top);
        assert!(!params.layer_over);
    }

    #[test]
    fn test_font_weight_ordering() {
        assert!(FontWeight::THIN < FontWeight::NORMAL);
        assert!(FontWeight::NORMAL < FontWeight::BOLD);
        assert!(FontWeight::BOLD < FontWeight::BLACK);
    }
}
