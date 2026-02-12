/// Generates a visual test PDF for verifying VAlign behavior.
///
/// The PDF has a single US Letter page with four rows, one per VAlign variant.
/// Each row draws:
///   - A red horizontal reference line at the anchor y-coordinate
///   - Text placed with the corresponding VAlign at that y-coordinate
///   - A label on the left naming the alignment mode
///
/// Correct behavior:
///   - Baseline: the text baseline sits ON the red line
///   - Bottom:   the lowest descender touches the red line
///   - Center:   the x-height midpoint of the text aligns with the red line
///   - Top:      the top of ascenders touches the red line
use std::sync::Arc;

use lopdf::{dictionary, Document, Object, Stream};

fn create_test_doc() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![],
        "Count" => 0,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

#[test]
fn generate_valign_test_pdf() {
    use medpdf::{AddTextParams, HAlign, PdfColor, VAlign};

    let mut doc = create_test_doc();

    // Create a US Letter page (612 x 792 points)
    let page_id = medpdf::create_blank_page(&mut doc, 612.0, 792.0).unwrap();

    let font_path = medpdf::find_font(std::path::Path::new("/Library/Fonts/CrimsonPro-Light.ttf")).unwrap();
    let mut cache = medpdf::FontCache::new();
    let font_data = cache.get_data(&font_path).unwrap();
    let font_name = font_path.get_name();

    let font_size = 36.0;
    let sample_text = "Tepqjy Align";

    // Six rows from top to bottom, each with a reference y-coordinate
    let alignments: Vec<(VAlign, &str, f32)> = vec![
        (VAlign::Top, "Top", 680.0),
        (VAlign::CapTop, "CapTop", 590.0),
        (VAlign::Center, "Center", 500.0),
        (VAlign::Baseline, "Baseline", 410.0),
        (VAlign::DescentBottom, "DescentBottom", 320.0),
        (VAlign::Bottom, "Bottom", 230.0),
    ];

    // Draw a title at the very top
    let title_params = AddTextParams::new(
        "VAlign Test \u{2014} red line = anchor y-coordinate",
        Arc::clone(&font_data),
        font_name.clone(),
    )
    .font_size(14.0)
    .position(50.0, 750.0)
    .color(PdfColor::rgb(0.3, 0.3, 0.3));
    medpdf::add_text_params(&mut doc, page_id, &title_params).unwrap();

    for (valign, label, y) in &alignments {
        // Draw a red horizontal reference line via a thin filled rectangle
        let line_content = format!(
            "q 1 0 0 rg {} {} 550 0.5 re f Q\n",
            30.0, y
        );
        let line_stream = Stream::new(dictionary! {}, line_content.into_bytes());
        let line_id = doc.add_object(line_stream);

        // Insert the line stream into the page's Contents
        let page_dict = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
        if let Ok(Object::Array(ref mut arr)) = page_dict.get_mut(b"Contents") {
            arr.insert(0, Object::Reference(line_id));
        }

        // Draw the label on the left (small, gray)
        let label_params = AddTextParams::new(
            format!("v_align={label}"),
            Arc::clone(&font_data),
            font_name.clone(),
        )
        .font_size(10.0)
        .position(35.0, *y + 20.0)
        .color(PdfColor::rgb(0.5, 0.5, 0.5));
        medpdf::add_text_params(&mut doc, page_id, &label_params).unwrap();

        // Draw the sample text with this VAlign
        let text_params = AddTextParams::new(
            sample_text,
            Arc::clone(&font_data),
            font_name.clone(),
        )
        .font_size(font_size)
        .position(150.0, *y)
        .color(PdfColor::BLACK)
        .h_align(HAlign::Left)
        .v_align(*valign);
        medpdf::add_text_params(&mut doc, page_id, &text_params).unwrap();
    }

    // --- WinAnsi encoding spot-check ---
    // Characters that differ between MacRoman and WinAnsi in the 0x80-0x9F range.
    // If the encoding/width mapping is wrong, these will show as wrong glyphs,
    // have zero-width spacing, or overlap each other.
    let winansi_test = concat!(
        "\u{201C}curly quotes\u{201D} ",  // left/right double quotes (0x93/0x94)
        "\u{2018}single quotes\u{2019} ", // left/right single quotes (0x91/0x92)
        "\u{2013} en dash \u{2013} ",      // en dash (0x96)
        "\u{2014} em dash \u{2014} ",      // em dash (0x97)
        "\u{20AC}100 ",                     // Euro sign (0x80)
        "\u{2022} bullet ",                 // bullet (0x95)
        "\u{2026} ellipsis ",              // horizontal ellipsis (0x85)
        "\u{2122} TM ",                     // trade mark (0x99)
        "\u{0152}\u{0153} ",               // OE/oe ligatures (0x8C/0x9C)
        "\u{0160}\u{0161}",                // S/s with caron (0x8A/0x9A)
    );

    let section_y = 120.0;
    let heading_params = AddTextParams::new(
        "WinAnsi 0x80\u{2013}0x9F spot-check (should be evenly spaced, no overlaps):",
        Arc::clone(&font_data),
        font_name.clone(),
    )
    .font_size(11.0)
    .position(50.0, section_y + 30.0)
    .color(PdfColor::rgb(0.3, 0.3, 0.3));
    medpdf::add_text_params(&mut doc, page_id, &heading_params).unwrap();

    let winansi_params = AddTextParams::new(
        winansi_test,
        Arc::clone(&font_data),
        font_name.clone(),
    )
    .font_size(13.0)
    .position(50.0, section_y)
    .color(PdfColor::BLACK);
    medpdf::add_text_params(&mut doc, page_id, &winansi_params).unwrap();

    // Save to a known location
    let output_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("valign_test.pdf");

    doc.compress();
    doc.save(&output_path).unwrap();
    println!("\nVAlign test PDF written to: {}", output_path.display());
}
