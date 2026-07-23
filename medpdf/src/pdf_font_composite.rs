//! Type0 / CIDFontType2 composite-font encoding for Unicode text beyond WinAnsi.
//!
//! When watermark text contains characters outside WinAnsiEncoding (CP1252) — the
//! Hawaiian ‘okina, kahakō vowels, and anything else — the single-byte WinAnsi path
//! in [`crate::pdf_watermark`] cannot represent them. This module builds the pieces of
//! a composite font using Identity-H encoding: the content stream carries 2-byte glyph
//! IDs (CID = GID, `CIDToGIDMap` = Identity against the fully-embedded font), a `/W`
//! widths array keyed by GID, and a `ToUnicode` CMap so text extraction round-trips.
//!
//! v1 embeds the full font (no subsetting); glyph-set subsetting of composite fonts is
//! a documented follow-up. See the crate feature plan for the layering.

use std::collections::HashSet;
use std::fmt::Write as _;

use lopdf::Object;
use ttf_parser::Face;

/// Encodes text as an Identity-H glyph-ID string: 2 bytes big-endian per glyph.
///
/// One GID per Unicode scalar via the font cmap (no shaping). On a missing glyph:
/// in `lossy` mode, emits `.notdef` (GID 0) and logs a warning; otherwise returns
/// `Err` with the distinct unrepresentable characters so the caller can fail loudly.
pub(crate) fn encode_text_identity(
    face: &Face,
    text: &str,
    lossy: bool,
) -> std::result::Result<Vec<u8>, Vec<char>> {
    let mut out = Vec::with_capacity(text.len() * 2);
    let mut missing: Vec<char> = Vec::new();
    for ch in text.chars() {
        match face.glyph_index(ch) {
            Some(gid) => out.extend_from_slice(&gid.0.to_be_bytes()),
            None => {
                if lossy {
                    log::warn!(
                        "Font lacks a glyph for '{}' (U+{:04X}); emitting .notdef",
                        ch,
                        ch as u32
                    );
                    out.extend_from_slice(&0u16.to_be_bytes());
                } else if !missing.contains(&ch) {
                    missing.push(ch);
                }
            }
        }
    }
    if !missing.is_empty() {
        return Err(missing);
    }
    Ok(out)
}

/// Collects the used characters that map to a glyph, as `(gid, char)` pairs sorted and
/// deduplicated by GID. Shared basis for the `/W` array and the ToUnicode CMap so both
/// cover exactly the same glyph set.
fn mapped_glyphs(face: &Face, used_chars: &HashSet<char>) -> Vec<(u16, char)> {
    let mut entries: Vec<(u16, char)> = used_chars
        .iter()
        .filter_map(|&ch| face.glyph_index(ch).map(|gid| (gid.0, ch)))
        .collect();
    entries.sort_by_key(|(gid, _)| *gid);
    entries.dedup_by_key(|(gid, _)| *gid);
    entries
}

/// Builds the CIDFontType2 `/W` array (`[ gid [ w ] … ]`) for the used glyphs, with
/// advances scaled to the PDF 1000-unit glyph space.
///
/// A `/W` entry for GID 0 (`.notdef`) is always emitted first. In lossy mode a missing
/// glyph is substituted with `.notdef` (GID 0), which no character maps to and which
/// therefore never appears in [`mapped_glyphs`]; without an explicit width the viewer
/// falls back to `DW` (=1000, a full em) for every substituted glyph while
/// `measure_text_width_with_face` counts 0, skewing advance and alignment (bug-0032).
/// Pinning GID 0 to the font's real `.notdef` advance replaces that full-em skew with
/// the glyph's true width.
pub(crate) fn build_w_array(face: &Face, used_chars: &HashSet<char>) -> Object {
    let upem = face.units_per_em() as f32;
    let scale = if upem > 0.0 { 1000.0 / upem } else { 0.0 };
    let mut arr: Vec<Object> = Vec::new();
    let push_entry = |gid: u16, arr: &mut Vec<Object>| {
        let advance = face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(0) as f32;
        let w = (advance * scale).round() as i64;
        arr.push(Object::Integer(gid as i64));
        arr.push(Object::Array(vec![Object::Integer(w)]));
    };
    push_entry(0, &mut arr);
    for (gid, _) in mapped_glyphs(face, used_chars) {
        if gid != 0 {
            push_entry(gid, &mut arr);
        }
    }
    Object::Array(arr)
}

/// Builds a `ToUnicode` CMap (PDF text-extraction map) from GID → Unicode for the used
/// glyphs. `bfchar` entries are chunked at 100 per block per the CMap spec.
pub(crate) fn build_tounicode_cmap(face: &Face, used_chars: &HashSet<char>) -> Vec<u8> {
    let entries = mapped_glyphs(face, used_chars);

    let mut s = String::new();
    s.push_str("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n");
    s.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    s.push_str("/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n");
    s.push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (gid, ch) in chunk {
            let mut dst = String::new();
            let mut buf = [0u16; 2];
            for unit in ch.encode_utf16(&mut buf).iter() {
                let _ = write!(dst, "{unit:04X}");
            }
            let _ = writeln!(s, "<{gid:04X}> <{dst}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A system font resolved to embedded bytes, for glyph-level tests. Returns None if
    /// the platform has no matching system font (keeps these tests environment-tolerant).
    fn embedded_face_bytes() -> Option<std::sync::Arc<Vec<u8>>> {
        let path = crate::pdf_font::find_font(std::path::Path::new("Helvetica")).ok()?;
        let mut cache = crate::pdf_font::FontCache::new();
        match cache.get_data(&path).ok()? {
            crate::font_data::FontData::Embedded(data) => Some(data),
            _ => None,
        }
    }

    #[test]
    fn encode_ascii_produces_two_bytes_per_char() {
        let Some(data) = embedded_face_bytes() else {
            return;
        };
        let face = Face::parse(&data, 0).unwrap();
        let gids = encode_text_identity(&face, "Hi", false).unwrap();
        assert_eq!(gids.len(), 4, "two glyphs, 2 bytes each");
    }

    #[test]
    fn encode_missing_glyph_errors_when_not_lossy() {
        let Some(data) = embedded_face_bytes() else {
            return;
        };
        let face = Face::parse(&data, 0).unwrap();
        // A CJK ideograph is absent from a Latin text font.
        let result = encode_text_identity(&face, "A\u{4E2D}B", false);
        assert!(result.is_err(), "missing glyph must fail loudly");
        assert_eq!(result.unwrap_err(), vec!['\u{4E2D}']);
    }

    #[test]
    fn encode_missing_glyph_lossy_emits_notdef() {
        let Some(data) = embedded_face_bytes() else {
            return;
        };
        let face = Face::parse(&data, 0).unwrap();
        let gids = encode_text_identity(&face, "A\u{4E2D}B", true).unwrap();
        assert_eq!(gids.len(), 6, "three code points, .notdef for the middle");
        assert_eq!(&gids[2..4], &[0, 0], "middle glyph is .notdef");
    }

    #[test]
    fn w_array_always_includes_notdef_gid0() {
        // Lossy-mode substitution emits GID 0 (.notdef), which no character maps to, so
        // it never appears in mapped_glyphs. Without an explicit /W entry the viewer
        // advances DW (=1000, a full em) for every substituted glyph, skewing alignment
        // (bug-0032). build_w_array must always pin GID 0 to the font's real .notdef
        // advance, and exactly once.
        let Some(data) = embedded_face_bytes() else {
            return;
        };
        let face = Face::parse(&data, 0).unwrap();
        let used: HashSet<char> = "Hi".chars().collect();
        let Object::Array(entries) = build_w_array(&face, &used) else {
            panic!("build_w_array must return an array");
        };
        // Entries are [gid, [w], gid, [w], …]; the GID slots are the even indices.
        let gids: Vec<i64> = entries
            .iter()
            .step_by(2)
            .map(|o| o.as_i64().unwrap())
            .collect();
        assert_eq!(
            gids.iter().filter(|&&g| g == 0).count(),
            1,
            "/W must contain exactly one GID-0 (.notdef) entry; got gids {gids:?}"
        );
        let notdef_advance = face.glyph_hor_advance(ttf_parser::GlyphId(0)).unwrap_or(0) as f32;
        let scale = 1000.0 / face.units_per_em() as f32;
        let expected = (notdef_advance * scale).round() as i64;
        // The width array immediately following GID 0 is [expected].
        let idx = gids.iter().position(|&g| g == 0).unwrap() * 2;
        let Object::Array(w) = &entries[idx + 1] else {
            panic!("width slot after GID 0 must be an array");
        };
        assert_eq!(
            w[0].as_i64().unwrap(),
            expected,
            "GID-0 /W must be the real .notdef advance ({expected}), not DW=1000"
        );
    }

    #[test]
    fn tounicode_contains_bfchar_block() {
        let Some(data) = embedded_face_bytes() else {
            return;
        };
        let face = Face::parse(&data, 0).unwrap();
        let used: HashSet<char> = "La\u{2018}i".chars().collect();
        let cmap = String::from_utf8(build_tounicode_cmap(&face, &used)).unwrap();
        assert!(cmap.contains("beginbfchar"));
        assert!(cmap.contains("endcmap"));
        // The ‘okina's Unicode value should appear as a UTF-16 destination.
        assert!(cmap.contains("2018"));
    }
}
