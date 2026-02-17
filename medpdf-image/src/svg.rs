use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use medpdf::{
    deep_copy_object, deep_copy_object_by_id, insert_content_stream,
    register_extgstate_in_page_resources, MedpdfError, Result,
};
use std::collections::BTreeMap;
use std::path::Path;

use crate::{compute_fit, fmt_f32, register_xobject_in_page_resources, unique_xobject_name, ImageFit};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Parsed SVG data ready for embedding.
pub struct SvgData {
    tree: usvg::Tree,
    /// Intrinsic width in SVG user units.
    pub width: f32,
    /// Intrinsic height in SVG user units.
    pub height: f32,
}

impl std::fmt::Debug for SvgData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgData")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// Options controlling SVG-to-PDF conversion.
#[derive(Debug, Clone, Copy)]
pub struct SvgOptions {
    /// Quality multiplier for rasterized filter effects. Default: 1.5.
    pub raster_scale: f32,
    /// DPI for SVG unit interpretation. Default: 96.0.
    pub svg_dpi: f32,
    /// Embed text as selectable PDF text (true) or convert to paths (false). Default: true.
    pub embed_text: bool,
    /// Compress content streams. Default: true.
    pub compress: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            raster_scale: 1.5,
            svg_dpi: 96.0,
            embed_text: true,
            compress: true,
        }
    }
}

/// Parameters for placing an SVG on a PDF page (builder pattern).
pub struct DrawSvgParams {
    pub svg_data: SvgData,
    /// X position in points.
    pub x: f32,
    /// Y position in points.
    pub y: f32,
    /// Output box width in points.
    pub width: f32,
    /// Output box height in points.
    pub height: f32,
    /// Fit mode (default: Contain).
    pub fit: ImageFit,
    /// Opacity (0.0 = transparent, 1.0 = opaque). Default: 1.0.
    pub alpha: f32,
    /// Rotation in degrees. Default: 0.0.
    pub rotation: f32,
    /// If true, draw over existing content; if false, draw under. Default: true.
    pub layer_over: bool,
    /// SVG conversion options.
    pub options: SvgOptions,
}

impl DrawSvgParams {
    /// Create new params with required fields. `width` and `height` are in points.
    pub fn new(svg_data: SvgData, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            svg_data,
            x,
            y,
            width,
            height,
            fit: ImageFit::Contain,
            alpha: 1.0,
            rotation: 0.0,
            layer_over: true,
            options: SvgOptions::default(),
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn layer_over(mut self, layer_over: bool) -> Self {
        self.layer_over = layer_over;
        self
    }

    pub fn options(mut self, options: SvgOptions) -> Self {
        self.options = options;
        self
    }

    pub fn raster_scale(mut self, raster_scale: f32) -> Self {
        self.options.raster_scale = raster_scale;
        self
    }

    pub fn svg_dpi(mut self, svg_dpi: f32) -> Self {
        self.options.svg_dpi = svg_dpi;
        self
    }

    pub fn embed_text(mut self, embed_text: bool) -> Self {
        self.options.embed_text = embed_text;
        self
    }

    pub fn compress(mut self, compress: bool) -> Self {
        self.options.compress = compress;
        self
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

fn default_usvg_options() -> usvg::Options<'static> {
    let mut opts = usvg::Options::default();
    opts.fontdb_mut().load_system_fonts();
    opts
}

/// Load an SVG from a file path.
pub fn load_svg(path: &Path) -> Result<SvgData> {
    let data = std::fs::read(path).map_err(|e| {
        MedpdfError::new(format!(
            "Failed to read SVG file '{}': {e}",
            path.display()
        ))
    })?;
    load_svg_bytes(&data)
}

/// Parse an SVG from a string.
pub fn load_svg_str(svg: &str) -> Result<SvgData> {
    let opts = default_usvg_options();
    let tree = usvg::Tree::from_str(svg, &opts)
        .map_err(|e| MedpdfError::new(format!("Failed to parse SVG: {e}")))?;
    let size = tree.size();
    Ok(SvgData {
        tree,
        width: size.width(),
        height: size.height(),
    })
}

/// Parse an SVG from raw bytes.
pub fn load_svg_bytes(data: &[u8]) -> Result<SvgData> {
    let opts = default_usvg_options();
    let tree = usvg::Tree::from_data(data, &opts)
        .map_err(|e| MedpdfError::new(format!("Failed to parse SVG: {e}")))?;
    let size = tree.size();
    Ok(SvgData {
        tree,
        width: size.width(),
        height: size.height(),
    })
}

// ---------------------------------------------------------------------------
// Core: add_svg
// ---------------------------------------------------------------------------

/// Embed an SVG into a PDF page.
pub fn add_svg(doc: &mut Document, page_id: ObjectId, params: DrawSvgParams) -> Result<()> {
    let opts = params.options;

    // Convert SVG → PDF bytes
    let conversion_options = svg2pdf::ConversionOptions {
        compress: opts.compress,
        raster_scale: opts.raster_scale,
        embed_text: opts.embed_text,
        ..Default::default()
    };
    let page_options = svg2pdf::PageOptions { dpi: opts.svg_dpi };
    let pdf_bytes = svg2pdf::to_pdf(&params.svg_data.tree, conversion_options, page_options)
        .map_err(|e| MedpdfError::new(format!("SVG to PDF conversion failed: {e}")))?;

    // Load the intermediate PDF
    let svg_doc = Document::load_mem(&pdf_bytes)
        .map_err(|e| MedpdfError::new(format!("Failed to load intermediate SVG PDF: {e}")))?;

    // Extract Form XObject — use MediaBox dimensions for correct coordinate mapping
    let (form_id, form_w, form_h) = extract_form_xobject(doc, &svg_doc)?;

    // Compute fit using Form XObject dimensions (PDF page coords)
    let (actual_w, actual_h, offset_x, offset_y, needs_clip) =
        compute_fit(form_w, form_h, params.width, params.height, params.fit);

    // Register XObject in page resources
    let svg_name = unique_xobject_name(doc, page_id, "Svg");
    register_xobject_in_page_resources(doc, page_id, form_id, &svg_name)?;

    // Build content stream (same pattern as add_image)
    let mut content = String::new();
    content.push_str("q\n");

    // Alpha via ExtGState
    if (params.alpha - 1.0).abs() > f32::EPSILON {
        let gs_dict = dictionary! {
            "Type" => "ExtGState",
            "ca" => params.alpha,
            "CA" => params.alpha,
        };
        let gs_id = doc.add_object(gs_dict);
        let gs_key = register_extgstate_in_page_resources(doc, page_id, gs_id)?;
        content.push_str(&format!("/{gs_key} gs\n"));
    }

    // Rotation around box center
    if params.rotation.abs() > 0.001 {
        let angle = params.rotation.to_radians();
        let cos = angle.cos();
        let sin = angle.sin();
        let cx = params.x + params.width / 2.0;
        let cy = params.y + params.height / 2.0;
        content.push_str(&format!(
            "1 0 0 1 {cx} {cy} cm\n{cos} {sin} {nsin} {cos} 0 0 cm\n1 0 0 1 {ncx} {ncy} cm\n",
            cx = fmt_f32(cx),
            cy = fmt_f32(cy),
            cos = fmt_f32(cos),
            sin = fmt_f32(sin),
            nsin = fmt_f32(-sin),
            ncx = fmt_f32(-cx),
            ncy = fmt_f32(-cy),
        ));
    }

    // Clipping rect for Cover mode
    if needs_clip {
        content.push_str(&format!(
            "{x} {y} {w} {h} re W n\n",
            x = fmt_f32(params.x),
            y = fmt_f32(params.y),
            w = fmt_f32(params.width),
            h = fmt_f32(params.height),
        ));
    }

    // Placement transform: scale from form BBox to actual size, then translate
    let place_x = params.x + offset_x;
    let place_y = params.y + offset_y;
    let scale_x = actual_w / form_w;
    let scale_y = actual_h / form_h;
    content.push_str(&format!(
        "{sx} 0 0 {sy} {x} {y} cm\n/{name} Do\n",
        sx = fmt_f32(scale_x),
        sy = fmt_f32(scale_y),
        x = fmt_f32(place_x),
        y = fmt_f32(place_y),
        name = svg_name,
    ));

    content.push_str("Q\n");

    let content_stream = Stream::new(dictionary! {}, content.into_bytes());
    let content_id = doc.add_object(content_stream);
    insert_content_stream(doc, page_id, content_id, params.layer_over)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract the first page of a PDF document as a Form XObject in the destination document.
/// Returns (form_object_id, form_width, form_height).
fn extract_form_xobject(
    dest_doc: &mut Document,
    svg_doc: &Document,
) -> Result<(ObjectId, f32, f32)> {
    // Get first page
    let pages = svg_doc.get_pages();
    let first_page_id = pages
        .values()
        .next()
        .copied()
        .ok_or_else(|| MedpdfError::new("SVG PDF has no pages"))?;
    let page_dict = svg_doc
        .get_dictionary(first_page_id)
        .map_err(|e| MedpdfError::new(format!("Failed to get SVG PDF page dict: {e}")))?;

    // Get MediaBox for form BBox dimensions
    let (form_w, form_h) = get_media_box_dimensions(svg_doc, page_dict)?;

    // Extract content bytes
    let content_bytes = extract_content_bytes(svg_doc, page_dict)?;

    // Deep-copy resources from svg_doc into dest_doc
    let mut copied_objects = BTreeMap::new();
    let resources_obj = if let Ok(res) = page_dict.get(b"Resources") {
        match res {
            Object::Reference(id) => {
                let new_id =
                    deep_copy_object_by_id(dest_doc, svg_doc, *id, &mut copied_objects)?;
                Object::Reference(new_id)
            }
            Object::Dictionary(_) => {
                deep_copy_object(dest_doc, svg_doc, res, &mut copied_objects)?
            }
            _ => Object::Dictionary(dictionary! {}),
        }
    } else {
        Object::Dictionary(dictionary! {})
    };

    // Create Form XObject
    let mut form_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(form_w),
            Object::Real(form_h),
        ],
    };
    form_dict.set("Resources", resources_obj);

    let form_stream = Stream::new(form_dict, content_bytes);
    let form_id = dest_doc.add_object(form_stream);

    Ok((form_id, form_w, form_h))
}

/// Read MediaBox dimensions from a page dictionary.
fn get_media_box_dimensions(
    doc: &Document,
    page_dict: &lopdf::Dictionary,
) -> Result<(f32, f32)> {
    let media_box = page_dict
        .get(b"MediaBox")
        .map_err(|_| MedpdfError::new("SVG PDF page has no MediaBox"))?;

    let arr = media_box
        .as_array()
        .map_err(|_| MedpdfError::new("MediaBox is not an array"))?;

    if arr.len() < 4 {
        return Err(MedpdfError::new("MediaBox has fewer than 4 elements"));
    }

    let get_f32 = |obj: &Object| -> Result<f32> {
        match obj {
            Object::Real(v) => Ok(*v as f32),
            Object::Integer(v) => Ok(*v as f32),
            Object::Reference(id) => match doc.get_object(*id)? {
                Object::Real(v) => Ok(*v as f32),
                Object::Integer(v) => Ok(*v as f32),
                _ => Err(MedpdfError::new("MediaBox element is not a number")),
            },
            _ => Err(MedpdfError::new("MediaBox element is not a number")),
        }
    };

    let x1 = get_f32(&arr[0])?;
    let y1 = get_f32(&arr[1])?;
    let x2 = get_f32(&arr[2])?;
    let y2 = get_f32(&arr[3])?;

    Ok(((x2 - x1).abs(), (y2 - y1).abs()))
}

/// Extract and merge all content stream bytes from a page.
fn extract_content_bytes(doc: &Document, page_dict: &lopdf::Dictionary) -> Result<Vec<u8>> {
    let contents = page_dict
        .get(b"Contents")
        .map_err(|_| MedpdfError::new("SVG PDF page has no Contents"))?;

    let mut buf = Vec::new();
    match contents {
        Object::Reference(id) => {
            append_stream_bytes(doc, *id, &mut buf)?;
        }
        Object::Array(arr) => {
            for item in arr {
                if let Object::Reference(id) = item {
                    if !buf.is_empty() {
                        buf.push(b'\n');
                    }
                    append_stream_bytes(doc, *id, &mut buf)?;
                } else {
                    return Err(MedpdfError::new("Unexpected object in Contents array"));
                }
            }
        }
        _ => {
            return Err(MedpdfError::new("Unexpected Contents type"));
        }
    }
    Ok(buf)
}

/// Decompress (if needed) and append a stream's bytes to the buffer.
fn append_stream_bytes(doc: &Document, obj_id: ObjectId, buf: &mut Vec<u8>) -> Result<()> {
    let obj = doc
        .get_object(obj_id)
        .map_err(|e| MedpdfError::new(format!("Failed to get content stream object: {e}")))?;
    let stream = obj
        .as_stream()
        .map_err(|e| MedpdfError::new(format!("Contents entry is not a stream: {e}")))?;

    let bytes = if stream.is_compressed() {
        stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone())
    } else {
        stream.content.clone()
    };
    buf.extend_from_slice(&bytes);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use medpdf::KEY_XOBJECT;

    const SIMPLE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
  <rect width="100" height="50" fill="red"/>
</svg>"#;

    fn create_test_doc_and_page() -> (Document, ObjectId) {
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

        let resources_id = doc.add_object(dictionary! {});
        let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
        let media_box = vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(612.0),
            Object::Real(792.0),
        ];
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => media_box,
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
        };
        let page_id = doc.add_object(page);
        let pages_obj = doc
            .get_object_mut(pages_id)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        let kids = pages_obj.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
        kids.push(Object::Reference(page_id));
        pages_obj.set("Count", Object::Integer(1));
        (doc, page_id)
    }

    #[test]
    fn test_load_svg_str_basic() {
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        assert!((svg.width - 100.0).abs() < 0.01);
        assert!((svg.height - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_load_svg_bytes_basic() {
        let svg = load_svg_bytes(SIMPLE_SVG.as_bytes()).unwrap();
        assert!((svg.width - 100.0).abs() < 0.01);
        assert!((svg.height - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_load_svg_str_invalid() {
        let result = load_svg_str("this is not svg");
        assert!(result.is_err());
    }

    #[test]
    fn test_add_svg_creates_xobject() {
        let (mut doc, page_id) = create_test_doc_and_page();
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(svg, 0.0, 0.0, 200.0, 100.0);
        add_svg(&mut doc, page_id, params).unwrap();

        // Verify XObject was registered in resources
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let res_id = page_dict
            .get(medpdf::KEY_RESOURCES)
            .unwrap()
            .as_reference()
            .unwrap();
        let res_dict = doc.get_dictionary(res_id).unwrap();
        let xobj_dict = res_dict.get(KEY_XOBJECT).unwrap().as_dict().unwrap();
        assert!(
            xobj_dict.get(b"Svg0").is_ok(),
            "Should have Svg0 XObject entry"
        );
    }

    #[test]
    fn test_add_svg_content_has_do() {
        let (mut doc, page_id) = create_test_doc_and_page();
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(svg, 50.0, 60.0, 200.0, 100.0);
        add_svg(&mut doc, page_id, params).unwrap();

        // Read back content streams and check for "Do" and "cm"
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
        let mut found_do = false;
        let mut found_cm = false;
        for item in contents {
            if let Object::Reference(id) = item {
                let stream = doc.get_object(*id).unwrap().as_stream().unwrap();
                let text = String::from_utf8_lossy(&stream.content);
                if text.contains("Do") {
                    found_do = true;
                }
                if text.contains("cm") {
                    found_cm = true;
                }
            }
        }
        assert!(found_do, "Content stream should contain 'Do' operator");
        assert!(found_cm, "Content stream should contain 'cm' operator");
    }

    #[test]
    fn test_add_svg_with_alpha() {
        let (mut doc, page_id) = create_test_doc_and_page();
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(svg, 0.0, 0.0, 200.0, 100.0).alpha(0.5);
        add_svg(&mut doc, page_id, params).unwrap();

        // Verify ExtGState was created for alpha
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let res_id = page_dict
            .get(medpdf::KEY_RESOURCES)
            .unwrap()
            .as_reference()
            .unwrap();
        let res_dict = doc.get_dictionary(res_id).unwrap();
        assert!(
            res_dict.get(medpdf::KEY_EXTGSTATE).is_ok(),
            "Should have ExtGState for alpha"
        );
    }

    #[test]
    fn test_add_svg_layer_under() {
        let (mut doc, page_id) = create_test_doc_and_page();
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(svg, 0.0, 0.0, 200.0, 100.0).layer_over(false);
        add_svg(&mut doc, page_id, params).unwrap();

        // In layer_under mode, new content is prepended
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let contents = page_dict.get(b"Contents").unwrap().as_array().unwrap();
        assert!(contents.len() >= 2, "Should have multiple content streams");

        // First stream should be our SVG content (prepended)
        if let Object::Reference(first_id) = &contents[0] {
            let stream = doc.get_object(*first_id).unwrap().as_stream().unwrap();
            let text = String::from_utf8_lossy(&stream.content);
            assert!(text.contains("Do"), "First stream should contain SVG Do");
        }
    }

    #[test]
    fn test_add_svg_form_xobject_has_bbox() {
        let (mut doc, page_id) = create_test_doc_and_page();
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(svg, 0.0, 0.0, 200.0, 100.0);
        add_svg(&mut doc, page_id, params).unwrap();

        // Find the Form XObject via resources
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let res_id = page_dict
            .get(medpdf::KEY_RESOURCES)
            .unwrap()
            .as_reference()
            .unwrap();
        let res_dict = doc.get_dictionary(res_id).unwrap();
        let xobj_dict = res_dict.get(KEY_XOBJECT).unwrap().as_dict().unwrap();
        let form_ref = xobj_dict.get(b"Svg0").unwrap().as_reference().unwrap();

        let form_obj = doc.get_object(form_ref).unwrap().as_stream().unwrap();
        let bbox = form_obj.dict.get(b"BBox").unwrap().as_array().unwrap();
        assert_eq!(bbox.len(), 4, "BBox should have 4 elements");

        // BBox should have positive width and height
        let w = match &bbox[2] {
            Object::Real(v) => *v as f32,
            Object::Integer(v) => *v as f32,
            _ => panic!("BBox[2] not a number"),
        };
        let h = match &bbox[3] {
            Object::Real(v) => *v as f32,
            Object::Integer(v) => *v as f32,
            _ => panic!("BBox[3] not a number"),
        };
        assert!(w > 0.0, "BBox width should be positive, got {w}");
        assert!(h > 0.0, "BBox height should be positive, got {h}");
    }

    #[test]
    fn test_draw_svg_params_builder() {
        let svg = load_svg_str(SIMPLE_SVG).unwrap();
        let params = DrawSvgParams::new(svg, 10.0, 20.0, 300.0, 150.0)
            .fit(ImageFit::Cover)
            .alpha(0.7)
            .rotation(45.0)
            .layer_over(false)
            .raster_scale(2.0)
            .svg_dpi(72.0)
            .embed_text(false)
            .compress(false);

        assert!((params.x - 10.0).abs() < f32::EPSILON);
        assert!((params.y - 20.0).abs() < f32::EPSILON);
        assert!((params.width - 300.0).abs() < f32::EPSILON);
        assert!((params.height - 150.0).abs() < f32::EPSILON);
        assert_eq!(params.fit, ImageFit::Cover);
        assert!((params.alpha - 0.7).abs() < f32::EPSILON);
        assert!((params.rotation - 45.0).abs() < f32::EPSILON);
        assert!(!params.layer_over);
        assert!((params.options.raster_scale - 2.0).abs() < f32::EPSILON);
        assert!((params.options.svg_dpi - 72.0).abs() < f32::EPSILON);
        assert!(!params.options.embed_text);
        assert!(!params.options.compress);
    }
}
