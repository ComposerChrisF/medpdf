use clap::Parser;
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use std::path::PathBuf;
use uuid::Uuid;

mod spec_types;

use medpdf::{parse_page_spec, AddTextParams, DrawRectParams, DrawLineParams, MedpdfError};
use medpdf::{EncryptionAlgorithm, EncryptionParams};
use medpdf::pdf_font::{find_font_with_style, FontCache};
use medpdf_image::DrawImageParams;
use spec_types::{WatermarkSpec, OverlaySpec, PadToSpec, PadFileSpec, DrawRectSpec, DrawLineSpec, DrawImageSpec, BlankPageSpec};


/// A command-line tool for advanced manipulation of PDF documents.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    output: PathBuf,
    #[arg(num_args = 2.., value_name = "FILE \"PAGES\"")]
    inputs: Vec<String>,
    #[arg(long, action = clap::ArgAction::Append)]
    blank_page: Vec<BlankPageSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    watermark: Vec<WatermarkSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    draw_rect: Vec<DrawRectSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    draw_line: Vec<DrawLineSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    draw_image: Vec<DrawImageSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    overlay: Vec<OverlaySpec>,
    #[arg(long)]
    pad_to: Option<PadToSpec>,
    #[arg(long)]
    pad_last_page_file: Option<PadFileSpec>,
    #[arg(long, help = "Use traditional PDF format for maximum compatibility with older tools")]
    broad_compatibility: bool,
    #[arg(long, help = "Password required to open the document")]
    user_password: Option<String>,
    #[arg(long, help = "Password required to change permissions/restrictions")]
    owner_password: Option<String>,
    #[arg(long, default_value = "aes256", help = "Encryption algorithm: aes256, aes128")]
    encryption_algorithm: String,
    #[arg(long, value_delimiter = ',', help = "Comma-separated permissions: print,modify,copy,annotate,fill,accessibility,assemble,print_hq,all,none")]
    permissions: Vec<String>,
}

fn format_xmp_metadata(doc_uuid: &str) -> String {
    let now = chrono::Local::now();
    let version = env!("CARGO_PKG_VERSION");
    format!("<?xpacket begin=\"?\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" xmlns:pdfxid=\"http://www.npes.org/pdfx/ns/id/\">
    <rdf:RDF>
        <rdf:Description rdf:about=\"\">
            <dc:title>
                <rdf:Alt>
                    <rdf:li xml:lang=\"x-default\"></rdf:li>
                </rdf:Alt>
            </dc:title>
        </rdf:Description>
        <rdf:Description rdf:about=\"\" pdf:Producer=\"lopdf\" pdf:Trapped=\"False\"/>
        <rdf:Description rdf:about=\"\" xmp:CreatorTool=\"pdf-merger v{version}\" xmp:CreateDate=\"{now}\" xmp:ModifyDate=\"{now}\" xmp:MetadataDate=\"{now}\"/>
        <rdf:Description rdf:about=\"\" xmpMM:DocumentID=\"uuid:{doc_uuid}\" xmpMM:VersionID=\"1\" xmpMM:RenditionClass=\"default\"/>
    </rdf:RDF>
</x:xmpmeta>
<?xpacket end=\"w\"?>")
}

fn init_document() -> Document {
    let mut doc = Document::with_version("1.7");
    let doc_uuid = Uuid::new_v4().to_string();
    let pages_id = doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![],
        "Count" => 0,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let metadata_id = doc.new_object_id();
    let metadata = dictionary! {
        "Type" => "Metadata",
        "Subtype" => "XML",
    };
    doc.objects.insert(metadata_id, Object::Stream(Stream {
        dict: metadata,
        content: format_xmp_metadata(&doc_uuid).into_bytes(),
        allows_compression: true,
        start_position: None,
    }));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Metadata" => metadata_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.trailer.set("ID", Object::Array(vec![
        Object::String(doc_uuid.clone().into_bytes(), StringFormat::Literal),
        Object::String(doc_uuid.into_bytes(), StringFormat::Literal),
    ]));
    doc
}

fn merge_pages(
    doc: &mut Document,
    page_ids: &mut Vec<lopdf::ObjectId>,
    inputs: &[String],
    blank_pages: &[BlankPageSpec],
) -> Result<(), MedpdfError> {
    println!("\n--- Merging Pages ---");
    for input_chunk in inputs.chunks(2) {
        let source_path = &input_chunk[0];
        let page_spec = &input_chunk[1];
        println!("Processing '{}' with pages '{}'...", source_path, page_spec);
        let source_doc = Document::load(source_path)?;
        let source_page_count = source_doc.page_iter().count();
        let page_numbers_to_import = parse_page_spec(page_spec, source_page_count as u32)?;
        println!("page_numbers_to_import: {page_numbers_to_import:?}; source_page_count: {source_page_count}");

        let mut copy_cache = std::collections::BTreeMap::new();
        for page_num in page_numbers_to_import {
            println!("Copying page: {page_num} from {source_path}");
            let new_page_id = medpdf::copy_page_with_cache(doc, &source_doc, page_num, &mut copy_cache)?;
            page_ids.push(new_page_id);
        }
    }

    for spec in blank_pages {
        println!("Adding {} blank page(s) ({}x{} pt)", spec.count, spec.width, spec.height);
        for _ in 0..spec.count {
            let page_id = medpdf::create_blank_page(doc, spec.width, spec.height)?;
            page_ids.push(page_id);
        }
    }

    if page_ids.is_empty() {
        return Err("No pages to output. Provide input files or use --blank-page.".into());
    }
    Ok(())
}

fn apply_overlays(
    doc: &mut Document,
    page_ids: &[lopdf::ObjectId],
    overlays: &[OverlaySpec],
) -> Result<(), MedpdfError> {
    println!("\n--- Applying Overlays ---");
    for spec in overlays {
        println!("Applying overlay from {}", spec.file.display());
        let overlay_doc = Document::load(&spec.file)?;
        let target_page_indices = parse_page_spec(&spec.target_pages, page_ids.len() as u32)?;
        for page_index in target_page_indices {
            let dest_page_id = *page_ids.get((page_index - 1) as usize)
                .ok_or_else(|| MedpdfError::new(format!("Overlay target page index {} out of range", page_index)))?;
            medpdf::overlay_page(doc, dest_page_id, &overlay_doc, spec.src_page)?;
        }
    }
    Ok(())
}

fn apply_drawing_commands(
    doc: &mut Document,
    page_ids: &[lopdf::ObjectId],
    rects: &[DrawRectSpec],
    lines: &[DrawLineSpec],
    images: &[DrawImageSpec],
    watermarks: &[WatermarkSpec],
) -> Result<(), MedpdfError> {
    println!("\n--- Applying Drawing Commands ---");
    let mut font_cache = FontCache::new();
    let mut font_object_cache = medpdf::EmbeddedFontCache::new();
    let num_pages = page_ids.len() as u32;

    for layer_over in [false, true] {
        let layer_name = if layer_over { "over" } else { "under" };

        for spec in rects.iter().filter(|s| s.layer_over == layer_over) {
            let target_page_indices = parse_page_spec(&spec.pages, num_pages)?;
            let params = DrawRectParams::new(spec.x, spec.y, spec.w, spec.h)
                .color(spec.color)
                .layer_over(layer_over);
            println!("Drawing rect ({layer_name}) to pages '{target_page_indices:?}'");
            for page_index in target_page_indices {
                let page_id = *page_ids.get((page_index - 1) as usize)
                    .ok_or_else(|| MedpdfError::new(format!("draw-rect target page index {} out of range", page_index)))?;
                medpdf::add_rect(doc, page_id, &params)?;
            }
        }

        for spec in lines.iter().filter(|s| s.layer_over == layer_over) {
            let target_page_indices = parse_page_spec(&spec.pages, num_pages)?;
            let params = DrawLineParams::new(spec.x1, spec.y1, spec.x2, spec.y2)
                .line_width(spec.width)
                .color(spec.color)
                .layer_over(layer_over);
            println!("Drawing line ({layer_name}) to pages '{target_page_indices:?}'");
            for page_index in target_page_indices {
                let page_id = *page_ids.get((page_index - 1) as usize)
                    .ok_or_else(|| MedpdfError::new(format!("draw-line target page index {} out of range", page_index)))?;
                medpdf::add_line(doc, page_id, &params)?;
            }
        }

        for spec in images.iter().filter(|s| s.layer_over == layer_over) {
            let target_page_indices = parse_page_spec(&spec.pages, num_pages)?;
            let image_data = medpdf_image::load_image(&spec.file)?;

            let img_w = image_data.pixel_width() as f32;
            let img_h = image_data.pixel_height() as f32;
            let (out_w, out_h) = match (spec.w, spec.h) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => (w, w * (img_h / img_w)),
                (None, Some(h)) => (h * (img_w / img_h), h),
                (None, None) => unreachable!("validated in FromStr"),
            };

            println!("Drawing image ({layer_name}) '{}' to pages '{target_page_indices:?}'", spec.file.display());
            for page_index in &target_page_indices {
                let page_id = *page_ids.get((*page_index - 1) as usize)
                    .ok_or_else(|| MedpdfError::new(format!("draw-image target page index {} out of range", page_index)))?;
                let params = DrawImageParams::new(image_data.clone(), spec.x, spec.y, out_w, out_h)
                    .fit(spec.fit)
                    .max_dpi(spec.max_dpi)
                    .alpha(spec.alpha)
                    .rotation(spec.rotation)
                    .layer_over(layer_over);
                medpdf_image::add_image(doc, page_id, params)?;
            }
        }

        for spec in watermarks.iter().filter(|s| s.layer_over == layer_over) {
            let font_path = find_font_with_style(&spec.font, spec.weight, spec.style)?;
            let font_data = font_cache.get_data(&font_path)?;
            let font_name = font_path.get_name();
            let target_page_indices = parse_page_spec(&spec.pages, num_pages)?;
            let x_points = spec.units.to_points(spec.x);
            let y_points = spec.units.to_points(spec.y);

            let params = AddTextParams::new(&spec.text, font_data.clone(), font_name)
                .font_size(spec.size)
                .position(x_points, y_points)
                .color(spec.color)
                .rotation(spec.rotation)
                .h_align(spec.h_align)
                .v_align(spec.v_align)
                .layer_over(layer_over)
                .strikeout(spec.strikeout)
                .underline(spec.underline);

            println!("Applying watermark ({layer_name}) '{}' to pages '{target_page_indices:?}'", spec.text);
            for page_index in target_page_indices {
                let page_id = *page_ids.get((page_index - 1) as usize)
                    .ok_or_else(|| MedpdfError::new(format!("Watermark target page index {} out of range", page_index)))?;
                medpdf::add_text_params(doc, page_id, &params, &mut font_object_cache)?;
            }
        }
    }
    Ok(())
}

fn apply_padding(
    doc: &mut Document,
    page_ids: &mut Vec<lopdf::ObjectId>,
    pad_to: &Option<PadToSpec>,
    pad_file: &Option<PadFileSpec>,
) -> Result<(), MedpdfError> {
    println!("\n--- Checking for Padding ---");
    let current_page_count = doc.get_pages().len();

    if let Some(spec) = pad_to {
        let pages = spec.pages as usize;
        if current_page_count > 0 {
            let pages_to_add = (pages - (current_page_count % pages)) % pages;
            if pages_to_add > 0 {
                println!("   -> Padding with {pages_to_add} page(s) to reach a multiple of {pages}.");
                let last_page_id = *page_ids.last()
                    .ok_or_else(|| MedpdfError::new("No pages in document to pad"))?;
                let media_box = medpdf::get_page_media_box(doc, last_page_id)
                    .ok_or_else(|| MedpdfError::new("Could not determine MediaBox for last page"))?;
                let width = media_box[2] - media_box[0];
                let height = media_box[3] - media_box[1];

                for _ in 0..(pages_to_add - 1) {
                    let page_id = medpdf::create_blank_page(doc, width, height)?;
                    page_ids.push(page_id);
                }
                if let Some(spec) = pad_file {
                    let pad_doc = Document::load(&spec.file)?;
                    let page_id = medpdf::copy_page(doc, &pad_doc, spec.page)?;
                    page_ids.push(page_id);
                } else {
                    let page_id = medpdf::create_blank_page(doc, width, height)?;
                    page_ids.push(page_id);
                }
            }
        }
    }
    Ok(())
}

fn parse_encryption_algorithm(s: &str) -> Result<EncryptionAlgorithm, MedpdfError> {
    match s.to_ascii_lowercase().as_str() {
        "aes256" | "aes-256" => Ok(EncryptionAlgorithm::Aes256),
        "aes128" | "aes-128" => Ok(EncryptionAlgorithm::Aes128),
        "rc4" | "rc4-128" | "rc4_128" => Ok(EncryptionAlgorithm::Rc4_128),
        _ => Err(MedpdfError::new(format!(
            "Unknown encryption algorithm: '{s}'. Valid values: aes256, aes128, rc4"
        ))),
    }
}

fn save_document(
    doc: &mut Document,
    output: &PathBuf,
    broad_compat: bool,
    encryption: Option<EncryptionParams>,
) -> Result<(), MedpdfError> {
    println!("\nSaving file to {}", output.display());
    doc.change_producer("PDF Merger Command-Line Tool");
    doc.compress();
    if let Some(params) = &encryption {
        println!("Encrypting with {:?}...", params.algorithm);
        medpdf::encrypt_document(doc, params)?;
    }
    if broad_compat {
        doc.save(output)?;
    } else {
        let mut file = std::fs::File::create(output)?;
        doc.save_modern(&mut file)?;
    }
    Ok(())
}

fn main() -> Result<(), MedpdfError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = Args::parse();
    if !args.inputs.is_empty() && args.inputs.len() % 2 != 0 {
        return Err("Input arguments must be in pairs of file paths and page specifications.".into());
    }

    let mut doc = init_document();
    let mut page_ids = Vec::new();

    merge_pages(&mut doc, &mut page_ids, &args.inputs, &args.blank_page)?;
    apply_overlays(&mut doc, &page_ids, &args.overlay)?;
    apply_drawing_commands(&mut doc, &page_ids, &args.draw_rect, &args.draw_line, &args.draw_image, &args.watermark)?;
    apply_padding(&mut doc, &mut page_ids, &args.pad_to, &args.pad_last_page_file)?;

    let encryption = match (&args.user_password, &args.owner_password) {
        (Some(user), Some(owner)) => {
            let algo = parse_encryption_algorithm(&args.encryption_algorithm)?;
            let perms = medpdf::parse_permissions(&args.permissions)
                .map_err(MedpdfError::new)?;
            Some(EncryptionParams::new(user, owner).algorithm(algo).permissions(perms))
        }
        (Some(user), None) => {
            let algo = parse_encryption_algorithm(&args.encryption_algorithm)?;
            let perms = medpdf::parse_permissions(&args.permissions)
                .map_err(MedpdfError::new)?;
            Some(EncryptionParams::new(user, user).algorithm(algo).permissions(perms))
        }
        (None, Some(owner)) => {
            let algo = parse_encryption_algorithm(&args.encryption_algorithm)?;
            let perms = medpdf::parse_permissions(&args.permissions)
                .map_err(MedpdfError::new)?;
            Some(EncryptionParams::new("", owner).algorithm(algo).permissions(perms))
        }
        (None, None) => None,
    };

    save_document(&mut doc, &args.output, args.broad_compatibility, encryption)?;

    println!("Operation successful!");
    Ok(())
}
