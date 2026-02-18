//! Post-watermark font subsetting using allsorts.
//!
//! Replaces full embedded font streams with subsetted versions containing
//! only the glyphs actually used by watermark text. Content streams and
//! font dictionaries (widths, encoding) are unchanged — only the binary
//! font data, `Length1`, and `BaseFont`/`FontName` prefix are modified.

use lopdf::{dictionary, Document, Object, Stream};
use rand::Rng;

use crate::pdf_watermark::{CachedFontEntry, EmbeddedFontCache};
use crate::Result;

/// Subsets all embedded fonts in the document, replacing full font streams
/// with minimal versions containing only used glyphs.
///
/// Always returns `Ok(())` — individual font subsetting failures are logged
/// as warnings and the original font stream is left untouched.
pub fn subset_fonts(doc: &mut Document, font_cache: &EmbeddedFontCache) -> Result<()> {
    for entry in font_cache.embedded_entries() {
        if entry.used_chars.is_empty() {
            continue;
        }
        match subset_single_font(doc, entry) {
            Ok(saved) => {
                log::info!(
                    "Subsetted font (stream {:?}): saved {} bytes",
                    entry.font_stream_id,
                    saved,
                );
            }
            Err(msg) => {
                log::warn!(
                    "Font subsetting failed for stream {:?}: {}; keeping full font",
                    entry.font_stream_id,
                    msg,
                );
            }
        }
    }
    Ok(())
}

/// Subsets a single font. Returns bytes saved on success, or an error message
/// for graceful fallback (original stream untouched).
fn subset_single_font(
    doc: &mut Document,
    entry: &CachedFontEntry,
) -> std::result::Result<usize, String> {
    use allsorts::binary::read::ReadScope;
    use allsorts::font::read_cmap_subtable;
    use allsorts::font_data::FontData;
    use allsorts::subset::{subset, CmapTarget, SubsetProfile};
    use allsorts::tables::cmap::Cmap;
    use allsorts::tables::FontTableProvider;
    use allsorts::tag;

    let data = &entry.data;
    let original_len = data.len();

    // Parse the font to get a table provider for cmap lookup
    let font_file = ReadScope::new(data)
        .read::<FontData>()
        .map_err(|e| format!("parse error: {e}"))?;
    let provider = font_file
        .table_provider(0)
        .map_err(|e| format!("table provider error: {e}"))?;

    // Read cmap and map characters to glyph IDs
    let cmap_data = provider
        .read_table_data(tag::CMAP)
        .map_err(|e| format!("cmap read error: {e}"))?;
    let cmap = ReadScope::new(&cmap_data)
        .read::<Cmap>()
        .map_err(|e| format!("cmap parse error: {e}"))?;
    let (_encoding, cmap_subtable) =
        read_cmap_subtable(&cmap)
            .map_err(|e| format!("cmap subtable error: {e}"))?
            .ok_or_else(|| "no suitable cmap subtable found".to_string())?;

    let mut glyph_ids: Vec<u16> = vec![0]; // .notdef always first
    for ch in &entry.used_chars {
        match cmap_subtable.map_glyph(*ch as u32) {
            Ok(Some(gid)) if gid != 0 => glyph_ids.push(gid),
            Ok(_) => {} // unmapped or .notdef — skip
            Err(e) => {
                log::debug!("cmap lookup failed for '{}': {e}", ch);
            }
        }
    }
    glyph_ids.sort();
    glyph_ids.dedup();

    if glyph_ids.len() <= 1 {
        return Err("no mapped glyphs found".to_string());
    }

    // Re-parse for subset() since the provider was borrowed from font_file
    let font_file2 = ReadScope::new(data)
        .read::<FontData>()
        .map_err(|e| format!("re-parse error: {e}"))?;
    let provider2 = font_file2
        .table_provider(0)
        .map_err(|e| format!("re-parse table provider error: {e}"))?;

    // Subset the font
    let subsetted = subset(&provider2, &glyph_ids, &SubsetProfile::Pdf, CmapTarget::Unicode)
        .map_err(|e| format!("subset error: {e:?}"))?;

    let subsetted_len = subsetted.len();
    if subsetted_len >= original_len {
        return Err("subsetted font not smaller than original".to_string());
    }

    // Replace the font stream object
    let font_file_dict = dictionary! {
        "Length1" => subsetted_len as i64,
    };
    let mut new_stream = Stream::new(font_file_dict, subsetted);
    new_stream
        .compress()
        .map_err(|e| format!("compression error: {e}"))?;
    doc.objects
        .insert(entry.font_stream_id, Object::Stream(new_stream));

    // Tag BaseFont and FontName with a random 6-letter prefix
    let tag = generate_subset_tag();
    prefix_base_font(doc, entry.font_id, &tag);
    prefix_font_name(doc, entry.descriptor_id, &tag);

    Ok(original_len - subsetted_len)
}

/// Generates a random 6-letter uppercase ASCII tag for subset font naming.
fn generate_subset_tag() -> String {
    let mut rng = rand::rng();
    (0..6).map(|_| (rng.random_range(b'A'..=b'Z')) as char).collect()
}

/// Prefixes `/BaseFont` in a Font dictionary with `"TAG+"`.
fn prefix_base_font(doc: &mut Document, font_id: lopdf::ObjectId, tag: &str) {
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(font_id) {
        if let Ok(Object::Name(name)) = dict.get(b"BaseFont") {
            let old_name = String::from_utf8_lossy(name).to_string();
            let new_name = format!("{tag}+{old_name}");
            dict.set("BaseFont", Object::Name(new_name.into_bytes()));
        }
    }
}

/// Prefixes `/FontName` in a FontDescriptor dictionary with `"TAG+"`.
fn prefix_font_name(doc: &mut Document, descriptor_id: lopdf::ObjectId, tag: &str) {
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(descriptor_id) {
        if let Ok(Object::Name(name)) = dict.get(b"FontName") {
            let old_name = String::from_utf8_lossy(name).to_string();
            let new_name = format!("{tag}+{old_name}");
            dict.set("FontName", Object::Name(new_name.into_bytes()));
        }
    }
}
