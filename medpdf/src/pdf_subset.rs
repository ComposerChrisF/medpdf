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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;

    /// Loads a system TTF font for tests. Returns None if unavailable.
    fn load_system_ttf() -> Option<Arc<Vec<u8>>> {
        let candidates = [
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Supplemental/Andale Mono.ttf",
            "/System/Library/Fonts/Supplemental/Verdana.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ];
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                return Some(Arc::new(data));
            }
        }
        None
    }

    // A1: generate_subset_tag produces 6 uppercase ASCII chars
    #[test]
    fn test_generate_subset_tag_format() {
        for _ in 0..10 {
            let tag = generate_subset_tag();
            assert_eq!(tag.len(), 6, "Tag should be exactly 6 chars: {tag}");
            assert!(
                tag.chars().all(|c| c.is_ascii_uppercase()),
                "All chars should be A..=Z: {tag}"
            );
        }
    }

    // A2: generate_subset_tag produces varying values
    #[test]
    fn test_generate_subset_tag_varies() {
        let tags: HashSet<String> = (0..50).map(|_| generate_subset_tag()).collect();
        assert!(tags.len() > 1, "50 calls should produce more than 1 distinct tag");
    }

    // A3: prefix_base_font adds TAG+ prefix
    #[test]
    fn test_prefix_base_font_adds_tag() {
        let mut doc = Document::with_version("1.7");
        let font_dict = dictionary! {
            "Type" => "Font",
            "BaseFont" => Object::Name(b"Verdana".to_vec()),
        };
        let font_id = doc.add_object(font_dict);

        prefix_base_font(&mut doc, font_id, "ABCDEF");

        let dict = doc.get_dictionary(font_id).unwrap();
        let base_font = dict.get(b"BaseFont").unwrap();
        if let Object::Name(name) = base_font {
            assert_eq!(
                String::from_utf8_lossy(name).as_ref(),
                "ABCDEF+Verdana"
            );
        } else {
            panic!("BaseFont should be a Name");
        }
    }

    // A4: prefix_base_font with missing key is a no-op
    #[test]
    fn test_prefix_base_font_missing_key_noop() {
        let mut doc = Document::with_version("1.7");
        let font_dict = dictionary! {
            "Type" => "Font",
        };
        let font_id = doc.add_object(font_dict);

        prefix_base_font(&mut doc, font_id, "ABCDEF");

        let dict = doc.get_dictionary(font_id).unwrap();
        assert!(dict.get(b"BaseFont").is_err(), "No BaseFont should exist");
    }

    // A5: prefix_base_font on non-dict object is a no-op
    #[test]
    fn test_prefix_base_font_nondict_noop() {
        let mut doc = Document::with_version("1.7");
        let stream = Stream::new(dictionary! {}, vec![1, 2, 3]);
        let id = doc.add_object(stream);

        prefix_base_font(&mut doc, id, "ABCDEF");
        // Should not panic; stream should be unchanged
        assert!(doc.get_object(id).unwrap().as_stream().is_ok());
    }

    // A6: prefix_font_name adds TAG+ prefix
    #[test]
    fn test_prefix_font_name_adds_tag() {
        let mut doc = Document::with_version("1.7");
        let desc_dict = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(b"Arial".to_vec()),
        };
        let desc_id = doc.add_object(desc_dict);

        prefix_font_name(&mut doc, desc_id, "XYZABC");

        let dict = doc.get_dictionary(desc_id).unwrap();
        let font_name = dict.get(b"FontName").unwrap();
        if let Object::Name(name) = font_name {
            assert_eq!(
                String::from_utf8_lossy(name).as_ref(),
                "XYZABC+Arial"
            );
        } else {
            panic!("FontName should be a Name");
        }
    }

    // A7: prefix_font_name with missing key is a no-op
    #[test]
    fn test_prefix_font_name_missing_key_noop() {
        let mut doc = Document::with_version("1.7");
        let desc_dict = dictionary! {
            "Type" => "FontDescriptor",
        };
        let desc_id = doc.add_object(desc_dict);

        prefix_font_name(&mut doc, desc_id, "ABCDEF");

        let dict = doc.get_dictionary(desc_id).unwrap();
        assert!(dict.get(b"FontName").is_err(), "No FontName should exist");
    }

    // A8: subset_single_font reduces size with real font data
    #[test]
    fn test_subset_single_font_reduces_size() {
        let font_data = match load_system_ttf() {
            Some(f) => f,
            None => { eprintln!("Skipping: no system TTF font found"); return; }
        };

        let mut doc = Document::with_version("1.7");

        // Create font stream object (uncompressed for simplicity)
        let font_file_dict = dictionary! {
            "Length1" => font_data.len() as i64,
        };
        let font_stream = Stream::new(font_file_dict, font_data.to_vec());
        let font_stream_id = doc.add_object(font_stream);

        // Create font descriptor
        let desc_dict = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(b"TestFont".to_vec()),
        };
        let descriptor_id = doc.add_object(desc_dict);

        // Create font dictionary
        let font_dict = dictionary! {
            "Type" => "Font",
            "BaseFont" => Object::Name(b"TestFont".to_vec()),
        };
        let font_id = doc.add_object(font_dict);

        let mut used_chars = HashSet::new();
        for ch in ['D', 'R', 'A', 'F', 'T'] {
            used_chars.insert(ch);
        }

        let entry = CachedFontEntry {
            font_id,
            font_key: "F1".into(),
            font_stream_id,
            descriptor_id,
            data: font_data,
            used_chars,
        };

        let result = subset_single_font(&mut doc, &entry);
        assert!(result.is_ok(), "subset_single_font should succeed: {:?}", result.err());
        let saved = result.unwrap();
        assert!(saved > 0, "Should save some bytes, saved: {saved}");

        // Verify stream was replaced and Length1 updated
        let stream = doc.get_object(font_stream_id).unwrap().as_stream().unwrap();
        let length1 = stream.dict.get(b"Length1").unwrap().as_i64().unwrap();
        assert!(length1 > 0, "Length1 should be positive");
        assert!(stream.dict.has(b"Filter"), "Stream should be compressed");
    }

    // A9: subset_single_font with garbage data returns error
    #[test]
    fn test_subset_single_font_invalid_data() {
        let mut doc = Document::with_version("1.7");

        let garbage_data = Arc::new(vec![0xFF, 0xFE, 0xFD, 0xFC, 0x00, 0x01]);
        let font_file_dict = dictionary! {
            "Length1" => garbage_data.len() as i64,
        };
        let font_stream = Stream::new(font_file_dict, garbage_data.to_vec());
        let font_stream_id = doc.add_object(font_stream);
        let descriptor_id = doc.add_object(dictionary! { "Type" => "FontDescriptor" });
        let font_id = doc.add_object(dictionary! { "Type" => "Font" });

        let mut used_chars = HashSet::new();
        used_chars.insert('A');

        let entry = CachedFontEntry {
            font_id,
            font_key: "F1".into(),
            font_stream_id,
            descriptor_id,
            data: garbage_data,
            used_chars,
        };

        let result = subset_single_font(&mut doc, &entry);
        assert!(result.is_err(), "Should fail with garbage font data");
        let err = result.unwrap_err();
        assert!(err.contains("parse error"), "Error should mention parse error: {err}");
    }

    // A10: subset_single_font with unmappable chars returns error
    #[test]
    fn test_subset_single_font_no_mapped_glyphs() {
        let font_data = match load_system_ttf() {
            Some(f) => f,
            None => { eprintln!("Skipping: no system TTF font found"); return; }
        };

        let mut doc = Document::with_version("1.7");

        let font_file_dict = dictionary! {
            "Length1" => font_data.len() as i64,
        };
        let font_stream = Stream::new(font_file_dict, font_data.to_vec());
        let font_stream_id = doc.add_object(font_stream);
        let descriptor_id = doc.add_object(dictionary! { "Type" => "FontDescriptor" });
        let font_id = doc.add_object(dictionary! { "Type" => "Font" });

        let mut used_chars = HashSet::new();
        used_chars.insert('\u{FFFF}');

        let entry = CachedFontEntry {
            font_id,
            font_key: "F1".into(),
            font_stream_id,
            descriptor_id,
            data: font_data,
            used_chars,
        };

        let result = subset_single_font(&mut doc, &entry);
        assert!(result.is_err(), "Should fail with unmappable chars");
        let err = result.unwrap_err();
        assert!(
            err.contains("no mapped glyphs found"),
            "Error should mention no mapped glyphs: {err}"
        );
    }
}
