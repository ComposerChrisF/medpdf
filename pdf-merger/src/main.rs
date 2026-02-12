use clap::Parser;
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use std::path::PathBuf;
use uuid::Uuid;

mod spec_types;

use medpdf::{parse_page_spec, AddTextParams, PdfMergeError};
use medpdf::pdf_font::{find_font, FontCache};
use medpdf::pdf_helpers::KEY_MEDIA_BOX;
use spec_types::{WatermarkSpec, OverlaySpec, PadToSpec, PadFileSpec};


/// A command-line tool for advanced manipulation of PDF documents.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    output: PathBuf,
    #[arg(required = true, num_args = 2.., value_name = "FILE \"PAGES\"")]
    inputs: Vec<String>,
    #[arg(long, action = clap::ArgAction::Append)]
    watermark: Vec<WatermarkSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    watermark_under: Vec<WatermarkSpec>,
    #[arg(long, action = clap::ArgAction::Append)]
    overlay: Vec<OverlaySpec>,
    #[arg(long)]
    pad_to: Option<PadToSpec>,
    #[arg(long)]
    pad_last_page_file: Option<PadFileSpec>,
    #[arg(long, value_name = "PASSWORD")]
    user_password: Option<String>,
    #[arg(long, value_name = "PASSWORD")]
    owner_password: Option<String>,
    #[arg(long, help = "Use traditional PDF format for maximum compatibility with older tools")]
    broad_compatibility: bool,
}

fn format_xmp_metadata(doc_uuid: &str) -> String {
    let now = chrono::Local::now();
    let version = env!("CARGO_PKG_VERSION");
    let metadata = format!("<?xpacket begin=\"?\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>
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
<?xpacket end=\"w\"?>");
    metadata
}

fn main() -> Result<(), PdfMergeError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = Args::parse();
    if args.inputs.len() % 2 != 0 {
        return Err("Input arguments must be in pairs of file paths and page specifications.".into());
    }

    let mut dest_doc = Document::with_version("1.7");
    let doc_uuid = Uuid::new_v4().to_string();
    let pages_id = dest_doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![],
        "Count" => 0,
    };
    dest_doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
    let metadata_id = dest_doc.new_object_id();
    let metadata = dictionary! {
        "Type" => "Metadata",
        "Subtype" => "XML",
    };
    dest_doc.objects.insert(metadata_id, lopdf::Object::Stream(Stream {
        dict: metadata,
        content: format_xmp_metadata(&doc_uuid).into_bytes(),
        allows_compression: true,
        start_position: None,
    }));
    let catalog_id = dest_doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Metadata" => metadata_id,
    });
    dest_doc.trailer.set("Root", catalog_id);
    dest_doc.trailer.set("ID", Object::Array(vec![
        Object::String(doc_uuid.clone().into_bytes(), StringFormat::Literal),
        Object::String(doc_uuid.into_bytes(), StringFormat::Literal)
    ]));
    let mut dest_page_ids: Vec<lopdf::ObjectId> = vec![];

    // --- Phase 1: Merge Pages ---
    println!("\n--- Merging Pages ---");
    for input_chunk in args.inputs.chunks(2) {
        let source_path = &input_chunk[0];
        let page_spec = &input_chunk[1];
        println!("Processing '{}' with pages '{}'...", source_path, page_spec);
        let source_doc = Document::load(source_path)?;
        let source_page_count = source_doc.page_iter().count();
        let page_numbers_to_import = parse_page_spec(page_spec, source_page_count as u32)?;
        println!("page_numbers_to_import: {page_numbers_to_import:?}; source_page_count: {source_page_count}");

        for page_num in page_numbers_to_import {
            println!("Copying page: {page_num} from {source_path}");
            let new_page_id = medpdf::copy_page(&mut dest_doc, &source_doc, page_num)?;
            dest_page_ids.push(new_page_id);
        }
    }

    // --- Phase 2: Apply Overlays ---
    println!("\n--- Applying Overlays ---");
    for spec in args.overlay.iter() {
        println!("Applying overlay from {}", spec.file.display());
        let overlay_doc = Document::load(&spec.file)?;
        let target_page_indices = parse_page_spec(&spec.target_pages, dest_page_ids.len() as u32)?;
        for page_index in target_page_indices {
            let dest_page_id = *dest_page_ids.get((page_index - 1) as usize)
                .ok_or_else(|| PdfMergeError::new(format!("Overlay target page index {} out of range", page_index)))?;
            medpdf::overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, spec.src_page.into())?;
        }
    }

    // --- Phase 3: Apply Watermarks ---
    println!("\n--- Applying Watermarks ---");
    let mut font_cache = FontCache::new();

    // Helper to apply watermarks with specified layer
    let apply_watermarks = |specs: &[WatermarkSpec], layer_over: bool, font_cache: &mut FontCache, dest_doc: &mut Document, dest_page_ids: &[_]| -> Result<(), PdfMergeError> {
        let layer_name = if layer_over { "over" } else { "under" };
        for spec in specs.iter() {
            let font_path = find_font(&spec.font)?;
            let font_data = font_cache.get_data(&font_path)?;
            let font_name = font_path.get_name();
            let target_page_indices = parse_page_spec(&spec.pages, dest_page_ids.len() as u32)?;
            let x_points = spec.units.to_points(spec.x);
            let y_points = spec.units.to_points(spec.y);

            let params = AddTextParams::new(&spec.text, font_data.to_vec(), font_name)
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
                let page_id = *dest_page_ids.get((page_index - 1) as usize)
                    .ok_or_else(|| PdfMergeError::new(format!("Watermark target page index {} out of range", page_index)))?;
                medpdf::add_text_params(dest_doc, page_id, &params)?;
            }
        }
        Ok(())
    };

    // Apply under-watermarks first (so they render behind everything)
    apply_watermarks(&args.watermark_under, false, &mut font_cache, &mut dest_doc, &dest_page_ids)?;
    // Apply over-watermarks (on top of content)
    apply_watermarks(&args.watermark, true, &mut font_cache, &mut dest_doc, &dest_page_ids)?;

    // --- Phase 4: Padding ---
    println!("\n--- Checking for Padding ---");
    let current_page_count = dest_doc.get_pages().len();

    if let Some(spec) = &args.pad_to {
        let pages = spec.pages as usize;
        if current_page_count > 0 {
            let pages_to_add = (pages - (current_page_count % pages)) % pages;
            if pages_to_add > 0 {
                println!("   -> Padding with {pages_to_add} page(s) to reach a multiple of {pages}.");
                let last_page_id = *dest_page_ids.last()
                    .ok_or_else(|| PdfMergeError::new("No pages in document to pad"))?;
                let last_page = dest_doc.get_object(last_page_id)?.as_dict()?;
                let media_box = last_page.get(KEY_MEDIA_BOX)?.as_array()?;
                let width = media_box.get(2)
                    .ok_or_else(|| PdfMergeError::new("Invalid MediaBox: expected 4 elements"))?
                    .as_f32()?;
                let height = media_box.get(3)
                    .ok_or_else(|| PdfMergeError::new("Invalid MediaBox: expected 4 elements"))?
                    .as_f32()?;

                for _ in 0..(pages_to_add - 1) {
                    medpdf::create_blank_page(&mut dest_doc, width, height)?;
                }
                if let Some(spec) = &args.pad_last_page_file {
                    let pad_doc = Document::load(&spec.file)?;
                    medpdf::copy_page(&mut dest_doc, &pad_doc, spec.page.into())?;
                } else {
                    medpdf::create_blank_page(&mut dest_doc, width, height)?;
                }
            }
        }
    }

    // --- Phase 5: Saving ---
    println!("\nSaving file to {}", args.output.display());
    dest_doc.change_producer("PDF Merger Command-Line Tool");
    dest_doc.compress();
    if args.broad_compatibility {
        dest_doc.save(&args.output)?;
    } else {
        let mut file = std::fs::File::create(&args.output)?;
        dest_doc.save_modern(&mut file)?;
    }

    println!("Operation successful!");
    Ok(())
}
