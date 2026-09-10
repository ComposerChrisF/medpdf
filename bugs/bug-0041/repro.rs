//! Repro for medpdf bug-0041: a page copied twice through one cache shares its
//! `/Annots` objects, and each shared annotation's `/P` points at the FIRST copy.
//!
//! This is a pdf-maker-side probe — it drives the `pdf-maker` binary — because
//! that is where it was verified. Reduced to a medpdf-level test, it is the same
//! shape as `bugs/bug-0040/repro.rs`: copy page 1 twice with one cache, then
//! compare the two pages' `/Annots` entries and each annotation's `/P`.
//!
//! Observed 2026-09-10 against medpdf 0.15.0 (the bug-0040 fix in place):
//!
//!     page objects: [(4, 0), (7, 0)]          <- distinct, bug-0040 works
//!     annots per page: [[(6, 0)], [(6, 0)]]   <- the SAME annotation object
//!     page (4, 0) annot (6, 0) has /P = Some((4, 0))  matches its own page: true
//!     page (7, 0) annot (6, 0) has /P = Some((4, 0))  matches its own page: false
use lopdf::{Document, Object, Stream, StringFormat, dictionary};
use std::process::Command;

fn make_annotated_pdf(path: &std::path::Path) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![], "Count" => Object::Integer(0),
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let idb = b"test0123456789ab".to_vec();
    doc.trailer.set("ID", Object::Array(vec![
        Object::String(idb.clone(), StringFormat::Literal),
        Object::String(idb, StringFormat::Literal),
    ]));

    let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf 100 700 Td (Hi) Tj ET".to_vec());
    let content_id = doc.add_object(content);
    let page_id = doc.new_object_id();
    // A link annotation whose /P points at its page.
    let annot_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![Object::Real(10.0), Object::Real(10.0), Object::Real(100.0), Object::Real(50.0)],
        "P" => Object::Reference(page_id),
        "A" => dictionary! { "S" => "URI", "URI" => Object::String(b"https://example.com".to_vec(), StringFormat::Literal) },
    });
    doc.objects.insert(page_id, Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![Object::Real(0.0), Object::Real(0.0), Object::Real(612.0), Object::Real(792.0)],
        "Resources" => dictionary! {},
        "Contents" => Object::Reference(content_id),
        "Annots" => vec![Object::Reference(annot_id)],
    }));
    let pd = doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    pd.get_mut(b"Kids").unwrap().as_array_mut().unwrap().push(Object::Reference(page_id));
    pd.set("Count", Object::Integer(1));
    doc.save(path).unwrap();
}

#[test]
fn probe_duplicated_annotated_page() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("annot.pdf");
    let out = dir.path().join("out.pdf");
    make_annotated_pdf(&src);

    let st = Command::new(env!("CARGO_BIN_EXE_pdf-maker"))
        .args(["-o", out.to_str().unwrap(), src.to_str().unwrap(), "1,1"])
        .status().unwrap();
    assert!(st.success());

    let doc = Document::load(&out).unwrap();
    let pages: Vec<_> = doc.get_pages().values().copied().collect();
    println!("page objects: {pages:?}");
    let annots: Vec<Vec<lopdf::ObjectId>> = pages.iter().map(|p| {
        doc.get_dictionary(*p).unwrap().get(b"Annots").ok()
            .and_then(|a| a.as_array().ok())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default()
    }).collect();
    println!("annots per page: {annots:?}");
    for (i, p) in pages.iter().enumerate() {
        for a in &annots[i] {
            let parent = doc.get_dictionary(*a).unwrap().get(b"P").ok().and_then(|o| o.as_reference().ok());
            println!("page {:?} annot {:?} has /P = {:?}  (matches its own page: {})", p, a, parent, parent == Some(*p));
        }
    }
}
