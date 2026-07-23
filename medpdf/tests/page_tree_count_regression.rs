// tests/page_tree_count_regression.rs
//
// Regression test for bugs/bug-0020: page-tree /Count maintenance was broken for
// documents with intermediate /Pages nodes (Acrobat writes balanced trees for
// documents over ~31 pages).
//
// PDF 32000-1 §7.7.3.2: every /Pages node carries a /Count equal to the number of
// *leaf* Page objects beneath it. The pre-fix code violated this two ways:
//
//   1. delete_page updated only the *direct* parent's /Count, leaving every
//      ancestor (including the root) stale — a phantom page for count-trusting
//      readers (most viewers: page count, random access, tree descent).
//   2. delete_page, copy_page, and create_blank_page all assigned
//      /Count = kids.len(). kids.len() counts *children*, not leaves, so under any
//      intermediate /Pages node a page silently appears or vanishes.
//
// medpdf's own get_pages() walks /Kids and ignores /Count, which is why the suite
// never noticed — the corruption is only visible to spec-conforming consumers of
// the saved file. These tests assert every ancestor's /Count equals the leaf count.
//
// The fix (pdf_helpers::adjust_ancestor_counts) walks the /Parent chain applying
// ±1, since adding/removing one leaf changes every ancestor's count by exactly ±1.
//
// To prove these tests pin the fix, set MEDPDF_TEMP_BUG0020=1 in adjust_ancestor_counts
// to reproduce the pre-fix behavior; the four nested-tree tests then fail while the
// flat-tree sanity test still passes (the flat case was never broken).

use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use medpdf::{copy_page, create_blank_page, delete_page};

/// Adds a minimal leaf Page whose /Parent is `parent`, returns its ObjectId.
fn add_leaf(doc: &mut Document, parent: ObjectId) -> ObjectId {
    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];
    let resources = doc.add_object(dictionary! {});
    let content = doc.add_object(Stream::new(dictionary! {}, vec![]));
    doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => parent,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content),
        "Resources" => Object::Reference(resources),
    })
}

/// Reads a /Pages node's stored /Count.
fn count_of(doc: &Document, id: ObjectId) -> i64 {
    doc.get_dictionary(id)
        .unwrap()
        .get(b"Count")
        .unwrap()
        .as_i64()
        .unwrap()
}

/// Root { PagesA { P1, P2 }, P3 } — a nested tree whose intermediate node comes
/// first, so P1/P2 are pages 1/2 (under PagesA) and P3 is page 3 (under root).
/// Returns (doc, root_id, pagesa_id). All /Count values are correct at build time.
fn nested_intermediate_first() -> (Document, ObjectId, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let root_id = doc.new_object_id();
    let pagesa_id = doc.new_object_id();

    let p1 = add_leaf(&mut doc, pagesa_id);
    let p2 = add_leaf(&mut doc, pagesa_id);
    let p3 = add_leaf(&mut doc, root_id);

    doc.objects.insert(
        pagesa_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Parent" => root_id,
            "Kids" => vec![Object::Reference(p1), Object::Reference(p2)],
            "Count" => 2,
        }),
    );
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(pagesa_id), Object::Reference(p3)],
            "Count" => 3,
        }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => root_id,
    });
    doc.trailer.set("Root", catalog);
    (doc, root_id, pagesa_id)
}

/// Root { PageX, PagesA { PA1, PA2 } } — leaf-first, so PageX is page 1 (a direct
/// child of the root) and PA1/PA2 are pages 2/3. Returns (doc, root_id).
fn nested_leaf_first() -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let root_id = doc.new_object_id();
    let pagesa_id = doc.new_object_id();

    let pagex = add_leaf(&mut doc, root_id);
    let pa1 = add_leaf(&mut doc, pagesa_id);
    let pa2 = add_leaf(&mut doc, pagesa_id);

    doc.objects.insert(
        pagesa_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Parent" => root_id,
            "Kids" => vec![Object::Reference(pa1), Object::Reference(pa2)],
            "Count" => 2,
        }),
    );
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(pagex), Object::Reference(pagesa_id)],
            "Count" => 3,
        }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => root_id,
    });
    doc.trailer.set("Root", catalog);
    (doc, root_id)
}

/// A flat single-level source doc with one simple page.
fn simple_source() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page = add_leaf(&mut doc, pages_id);
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page)],
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog);
    doc
}

// --- delete_page ---

#[test]
fn delete_page_decrements_every_ancestor_count() {
    let (mut doc, root, pagesa) = nested_intermediate_first();
    assert_eq!(doc.get_pages().len(), 3);
    assert_eq!(count_of(&doc, root), 3);
    assert_eq!(count_of(&doc, pagesa), 2);

    // Page 1 is P1, a leaf under the intermediate node PagesA.
    delete_page(&mut doc, 1).expect("delete P1 (under PagesA)");

    assert_eq!(doc.get_pages().len(), 2, "two leaves remain");
    assert_eq!(
        count_of(&doc, pagesa),
        1,
        "direct parent PagesA must drop to 1"
    );
    // This is the ancestor the pre-fix code never touched.
    assert_eq!(
        count_of(&doc, root),
        2,
        "root /Count must be decremented too (bug-0020: stale ancestor count)"
    );
}

#[test]
fn delete_page_uses_leaf_count_not_kids_len() {
    let (mut doc, root) = nested_leaf_first();
    assert_eq!(count_of(&doc, root), 3);

    // Page 1 is PageX, a *direct* child of the root.
    delete_page(&mut doc, 1).expect("delete PageX (direct root child)");

    // The root's /Kids is now just [PagesA] (len 1), but two leaf pages remain.
    // The pre-fix code set /Count = kids.len() = 1, hiding a page.
    assert_eq!(doc.get_pages().len(), 2);
    assert_eq!(
        count_of(&doc, root),
        2,
        "root /Count must equal leaf count (2), not kids.len() (1) — bug-0020"
    );
}

// --- copy_page ---

#[test]
fn copy_page_into_nested_dest_uses_leaf_count() {
    // Dest: Root { PagesA { PA1, PA2 }, P3 } — root /Count 3, three leaves.
    let (mut dest, root, _pagesa) = nested_intermediate_first();
    let src = simple_source();
    assert_eq!(count_of(&dest, root), 3);

    copy_page(&mut dest, &src, 1).expect("copy_page into nested dest");

    // The new leaf attaches to the root /Pages node, whose /Kids becomes
    // [PagesA, P3, new] (len 3) — the pre-fix code set /Count = 3, hiding a page.
    assert_eq!(dest.get_pages().len(), 4);
    assert_eq!(
        count_of(&dest, root),
        4,
        "root /Count must be 4 (leaf count), not kids.len() (3) — bug-0020"
    );
}

// --- create_blank_page ---

#[test]
fn blank_page_into_nested_dest_uses_leaf_count() {
    let (mut dest, root, _pagesa) = nested_intermediate_first();
    assert_eq!(count_of(&dest, root), 3);

    create_blank_page(&mut dest, 200.0, 300.0).expect("create_blank_page into nested dest");

    assert_eq!(dest.get_pages().len(), 4);
    assert_eq!(
        count_of(&dest, root),
        4,
        "root /Count must be 4 (leaf count), not kids.len() (3) — bug-0020"
    );
}

// --- flat-tree sanity (the common single-level path, never broken) ---

#[test]
fn flat_tree_counts_stay_correct() {
    let mut doc = Document::with_version("1.7");
    let root_id = doc.new_object_id();
    let kids: Vec<Object> = (0..3)
        .map(|_| Object::Reference(add_leaf(&mut doc, root_id)))
        .collect();
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => 3,
        }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => root_id,
    });
    doc.trailer.set("Root", catalog);

    assert_eq!(count_of(&doc, root_id), 3);

    create_blank_page(&mut doc, 100.0, 100.0).unwrap();
    assert_eq!(count_of(&doc, root_id), 4);
    assert_eq!(doc.get_pages().len(), 4);

    delete_page(&mut doc, 1).unwrap();
    assert_eq!(count_of(&doc, root_id), 3);
    assert_eq!(doc.get_pages().len(), 3);
}
