#[macro_use]
extern crate lopdf;

use clap::{Parser, ValueEnum};
use lopdf::{Document, Object, Stream, StringFormat};
use std::path::PathBuf;
use std::str::FromStr;
use uuid::Uuid;

mod error;
mod parsing;
mod pdf_font;
mod pdf_helpers;
mod pdf_copy_page;
mod pdf_blank_page;
mod pdf_watermark;
mod pdf_overlay;
mod font_helpers;
use parsing::parse_page_spec;

use crate::error::PdfMergeError;
use crate::pdf_font::{find_font, FontCache};
use crate::pdf_helpers::{Unit, KEY_MEDIA_BOX};



#[derive(Debug, Clone)]
struct WatermarkSpec {
    text: String,
    font: PathBuf,
    size: f32,
    x: f32,
    y: f32,
    units: Unit,
    pages: String,
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
                "units" => units = Some(Unit::from_str(value, true).map_err(|e| e.to_string())?),
                // FUTURE: Move units into "x" and "y", e.g. "x=1.25in,y=3pt".  Support pt, in, mm, cm, % of page(?)
                // FUTURE: Add centering options: LCR for x axis (L default), TCBb for y axis (b=baseline default)
                // FUTURE: Add color support (specify kind, too), e.g. "0,0,0" is RGB black, "0,0,0,1" is CMYK black, "0" is gray black.
                // FUTURE: Add transparancy (e.g. "alpha=0.5")
                // FUTURE: Add rotation (in degrees, about x,y origina after LCR/TCBb layout)
                "pages" => pages = Some(value.to_string()),
                _ => return Err(format!("Unknown watermark key: '{}'", key)),
            }
        }
        Ok(WatermarkSpec {
            text: text.ok_or("Watermark 'text' is required")?,
            font: font.ok_or("Watermark 'font' is required")?,
            size: size.unwrap_or(48.0),
            x: x.ok_or("Watermark 'x' coordinate is required")?,
            y: y.ok_or("Watermark 'y' coordinate is required")?,
            units: units.unwrap_or(Unit::In),
            pages: pages.unwrap_or_else(|| "all".to_string()),
        })
    }
}

#[derive(Debug, Clone)]
struct OverlaySpec {
    file: PathBuf,
    src_page: u16,
    target_pages: String,
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
struct PadToSpec {
    pages: u16,
}

impl FromStr for PadToSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pages = s.parse::<u16>().map_err(|e| e.to_string())?;
        Ok(PadToSpec { pages })
    }
}

// FUTURE: Turn page into a page_range_spec, and use that to determine how many blank pages to 
// add.  NOTE, we'd have to decide what to do when we only need to add 1 page, but page_range_spec
// specifies more (just the first "n", or should it be the last "n", or a mode spec?).  If
// added, note that an unspecified page_range_spec probably should default to "all".
#[derive(Debug, Clone)]
struct PadFileSpec {
    file: PathBuf,
    page: u16,
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
    overlay: Vec<OverlaySpec>,
    #[arg(long)]
    pad_to: Option<PadToSpec>,
    #[arg(long)]
    pad_last_page_file: Option<PadFileSpec>,
    #[arg(long, value_name = "PASSWORD")]
    user_password: Option<String>,
    #[arg(long, value_name = "PASSWORD")]
    owner_password: Option<String>,
}

fn format_xmp_metadata(doc_uuid: &str) -> String {
    let now = chrono::Local::now();
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
        <rdf:Description rdf:about=\"\" xmp:CreatorTool=\"pdf_merger v0.1.0\" xmp:CreateDate=\"{now}\" xmp:ModifyDate=\"{now}\" xmp:MetadataDate=\"{now}\"/>
        <rdf:Description rdf:about=\"\" xmpMM:DocumentID=\"uuid:{doc_uuid}\" xmpMM:VersionID=\"1\" xmpMM:RenditionClass=\"default\"/>
    </rdf:RDF>
</x:xmpmeta>
<?xpacket end=\"w\"?>");
    metadata
}

fn main() -> Result<(), PdfMergeError> {
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
            let new_page_id = pdf_copy_page::copy_page(&mut dest_doc, &source_doc, page_num)?;
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
            let dest_page_id = dest_page_ids[(page_index - 1) as usize];
            pdf_overlay::overlay_page(&mut dest_doc, dest_page_id, &overlay_doc, spec.src_page.into())?;
        }
    }

    // --- Phase 3: Apply Watermarks ---
    println!("\n--- Applying Watermarks ---");
    let mut font_cache = FontCache::new();
    for spec in args.watermark.iter() {
        // FUTURE: Encapsulate font_path, font_data, font_name all within Font struct; Cache becomes FontManager
        let font_path = find_font(&spec.font)?;
        let font_data = font_cache.get_data(&font_path)?;
        let font_name = font_path.get_name();
        let target_page_indices = parse_page_spec(&spec.pages, dest_page_ids.len() as u32)?;
        let x_points = spec.units.to_points(spec.x);
        let y_points = spec.units.to_points(spec.y);

        println!("Applying watermark '{}' to pages '{target_page_indices:?}'", spec.text);
        for page_index in target_page_indices {
            let page_id = dest_page_ids[(page_index - 1) as usize];
            pdf_watermark::add_text(&mut dest_doc, page_id, &spec.text, &font_data, &font_name, spec.size, x_points as i32, y_points as i32)?;
        }
    }
    
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
                let width = media_box[2].as_f32()?;
                let height = media_box[3].as_f32()?;

                for _ in 0..(pages_to_add - 1) {
                    pdf_blank_page::create_blank_page(&mut dest_doc, width, height)?;
                }
                if let Some(spec) = &args.pad_last_page_file {
                    let pad_doc = Document::load(&spec.file)?;
                    pdf_copy_page::copy_page(&mut dest_doc, &pad_doc, spec.page.into())?;
                } else {
                    pdf_blank_page::create_blank_page(&mut dest_doc, width, height)?;
                }
            }
        }
    }

    // --- Phase 5: Saving ---
    println!("\nSaving file to {}", args.output.display());
    dest_doc.change_producer("PDF Merger Command-Line Tool");
    //doc.set_creation_date(Local::now());
 
    //let mut save_options = lopdf::SaveOptions::new();
    //if args.owner_password.is_some() || args.user_password.is_some() {
    //    println!("Applying security settings...");
    //    let mut permissions = lopdf::Permissions::new();
    //    if args.owner_password.is_some() {
    //        permissions.set_print(true).set_copy(false).set_modify(false);
    //    }
    //    save_options.set_permissions(permissions);
    //    save_options.set_user_password(args.user_password.as_deref().map(|s| s.as_bytes().to_vec()));
    //    save_options.set_owner_password(args.owner_password.as_deref().map(|s| s.as_bytes().to_vec()));
    //}
    dest_doc.compress();
    dest_doc.save(args.output)?;//.save_with_options(&args.output, &save_options)?;
    
    println!("✅ Operation successful!");
    Ok(())
}
