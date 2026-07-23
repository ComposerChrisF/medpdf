// tests/font_collection_rejected_regression.rs
//
// Regression test for bugs/bug-0012 (Plan A): font-kit hands back a `font_index`
// selecting a face inside a font collection (.ttc/.otc). The pre-fix code dropped the
// index and hardcoded face 0 everywhere, so requesting a styled macOS system font
// (almost all live in .ttc) silently embedded the WRONG face with `BaseFont = Unknown`
// and the whole collection blob as its font program — unusable in many viewers.
//
// Plan A stops the silent corruption without any public-API change: a nonzero face
// index fails loudly at resolution (handle_to_font_path), and a collection blob is
// refused at the embed step (add_text_params). Extracting a single face is a planned
// follow-up (Plan B).
//
// Two tests:
//   * `embedding_a_collection_blob_is_refused` — deterministic guard-2 test: feeding
//     collection bytes to add_text_params errors (pre-fix: succeeded, embedding face 0).
//   * `styled_system_font_never_silently_embeds_unknown` — invariant test for the
//     reported flow: requesting Helvetica Bold must not silently succeed with
//     BaseFont = Unknown. It must either fail loudly or embed a real face.

mod fixtures;

use std::path::Path;
use std::sync::Arc;

use lopdf::Document;
use medpdf::types::AddTextParams;
use medpdf::{
    EmbeddedFontCache, FontData, FontStyle, FontWeight, add_text_params, find_font_with_style,
};

/// Reads the first available TrueType/OpenType collection (.ttc) on this machine, or
/// None if none is present (the test then skips).
fn load_collection() -> Option<Arc<Vec<u8>>> {
    let candidates = [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Times.ttc",
        "/System/Library/Fonts/Courier.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            // Confirm it really is a collection before using it.
            if ttf_parser::fonts_in_collection(&bytes).is_some() {
                return Some(Arc::new(bytes));
            }
        }
    }
    None
}

fn a_page() -> (Document, lopdf::ObjectId) {
    let source = fixtures::create_pdf_with_pages(1);
    let mut doc = fixtures::create_empty_pdf();
    let page_id = medpdf::copy_page(&mut doc, &source, 1).unwrap();
    (doc, page_id)
}

#[test]
fn embedding_a_collection_blob_is_refused() {
    let Some(collection) = load_collection() else {
        eprintln!("no .ttc collection available; skipping bug-0012 guard-2 test");
        return;
    };

    let (mut doc, page_id) = a_page();
    let params = AddTextParams::new("DRAFT", FontData::Embedded(collection), "SomeCollection")
        .font_size(24.0)
        .position(72.0, 400.0);

    let result = add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new());
    let err = result.expect_err("embedding a .ttc collection blob must fail loudly (bug-0012)");
    let msg = err.to_string();
    assert!(
        msg.contains("collection"),
        "the error must name the collection problem, got: {msg}"
    );
}

/// A stored font dict's BaseFont, if it has one.
fn base_font_names(doc: &Document) -> Vec<String> {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"Font"))
        .filter_map(|d| d.get(b"BaseFont").ok().and_then(|b| b.as_name().ok()))
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

#[test]
fn styled_system_font_never_silently_embeds_unknown() {
    // Requesting a bold system font (Helvetica Bold lives in Helvetica.ttc on macOS)
    // must NOT silently succeed with BaseFont = Unknown. The fix makes it fail loudly
    // at resolution or at embed; only a genuine single-face match may succeed.
    let resolved =
        find_font_with_style(Path::new("Helvetica"), FontWeight::BOLD, FontStyle::Normal);

    let font_path = match resolved {
        Err(_) => return, // Loud failure at resolution — the fix worked (nonzero face index).
        Ok(fp) => fp,
    };

    // It resolved to a FontPath. Run the rest of the real flow.
    let mut cache = medpdf::FontCache::new();
    let font_data = match cache.get_data(&font_path) {
        Err(_) => return, // Loud failure reading the font — acceptable.
        Ok(fd) => fd,
    };

    let (mut doc, page_id) = a_page();
    let params = AddTextParams::new("DRAFT", font_data, font_path.get_name())
        .font_size(24.0)
        .position(72.0, 400.0);

    match add_text_params(&mut doc, page_id, &params, &mut EmbeddedFontCache::new()) {
        Err(_) => {} // Loud failure at embed (collection blob) — the fix worked.
        Ok(()) => {
            // A genuine single-face match may succeed — but it must never be the
            // silent wrong-face embed the bug produced (BaseFont = Unknown).
            let names = base_font_names(&doc);
            assert!(
                !names.iter().any(|n| n == "Unknown"),
                "styled system font silently embedded BaseFont = Unknown (bug-0012); names: {names:?}"
            );
        }
    }
}
