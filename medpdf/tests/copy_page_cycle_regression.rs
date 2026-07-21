// tests/copy_page_cycle_regression.rs
//
// Regression test for bugs/bug-0007: deep_copy_object_by_id recorded the
// source->dest mapping only *after* the recursive copy returned, so any
// reference cycle not passing through /Parent (an annotation's /P page
// back-reference, a self-linking /Dest) recursed forever and overflowed the
// stack — an uncatchable SIGABRT (exit 134), not an Err.
//
// The failure is a process abort, so `#[should_panic]` cannot catch it: the
// overflow kills the whole test runner. Each case therefore runs the copy in a
// *child process* (this same test binary re-invoked with --exact) and the parent
// asserts on the child's exit status. Reverting the fix in pdf_helpers.rs turns
// each child into a stack-overflow abort, failing the parent assertion.
//
// The success marker is load-bearing: libtest exits 0 when a filter matches zero
// tests, so a mis-named child would "succeed" with the bug still present. We
// require the marker — printed only after copy_page returns Ok *and* the copied
// annotation's back-reference is verified — so the exit code alone can never
// certify a run that did not actually exercise the copy.

mod fixtures;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

/// Set (to the scenario name) in the child process; its presence selects the
/// child role.
const CHILD_ENV: &str = "MEDPDF_BUG0007_CHILD";
/// Printed by the child only after the copy succeeded and was verified.
const OK_MARKER: &str = "BUG0007_CHILD_OK";

/// Builds a one-page source document whose sole annotation refers back to its own
/// page, forming a cycle: page -> /Annots -> annot -> (back-ref) -> page.
///
/// `build_annot(page_id)` returns the complete annotation dictionary, embedding
/// the cyclic reference to the page it is attached to.
fn source_with_self_referencing_annot(
    build_annot: impl FnOnce(ObjectId) -> Dictionary,
) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();

    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
    let annot_id = doc.add_object(build_annot(page_id));

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
        "Annots" => vec![Object::Reference(annot_id)],
    };
    doc.objects.insert(page_id, Object::Dictionary(page));

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// Resolves the copied annotation's back-reference to a page ObjectId. `pick`
/// extracts the reference from the copied annotation dictionary.
fn copied_backref_target(
    dest: &Document,
    new_page_id: ObjectId,
    pick: impl FnOnce(&Dictionary) -> ObjectId,
) -> ObjectId {
    let page_dict = dest
        .get_dictionary(new_page_id)
        .expect("copied page must be a dictionary");
    let annots = page_dict
        .get(b"Annots")
        .expect("copied page must keep /Annots")
        .as_array()
        .expect("/Annots must be an array");
    let annot_ref = annots
        .first()
        .expect("/Annots must have one entry")
        .as_reference()
        .expect("/Annots entry must be a reference");
    let annot_dict = dest
        .get_dictionary(annot_ref)
        .expect("copied annotation must be a dictionary");
    pick(annot_dict)
}

/// The work that used to abort the process. Runs in the child; panics (→ non-zero
/// exit) on any failure, prints `OK_MARKER` on success.
fn run_child(scenario: &str) {
    let mut dest = fixtures::create_empty_pdf();
    match scenario {
        // Cyclic reference as a dictionary value: /P -> own page.
        "p_backref" => {
            let src = source_with_self_referencing_annot(|page_id| {
                dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Link",
                    "Rect" => vec![0.0.into(), 0.0.into(), 100.0.into(), 20.0.into()],
                    "P" => Object::Reference(page_id),
                }
            });
            let new_page_id = medpdf::copy_page(&mut dest, &src, 1)
                .expect("copy_page must return Ok, not crash, on a self-referencing /P");
            let target = copied_backref_target(&dest, new_page_id, |annot| {
                annot
                    .get(b"P")
                    .expect("copied annot must keep /P")
                    .as_reference()
                    .expect("/P must be a reference")
            });
            assert_eq!(
                target, new_page_id,
                "/P must resolve to the *copied* page, not the source page id"
            );
        }
        // Cyclic reference as an array element: /Dest -> [own page, /Fit].
        "dest_selflink" => {
            let src = source_with_self_referencing_annot(|page_id| {
                dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Link",
                    "Rect" => vec![0.0.into(), 0.0.into(), 100.0.into(), 20.0.into()],
                    "Dest" => vec![Object::Reference(page_id), "Fit".into()],
                }
            });
            let new_page_id = medpdf::copy_page(&mut dest, &src, 1)
                .expect("copy_page must return Ok, not crash, on a self-linking /Dest");
            let target = copied_backref_target(&dest, new_page_id, |annot| {
                annot
                    .get(b"Dest")
                    .expect("copied annot must keep /Dest")
                    .as_array()
                    .expect("/Dest must be an array")
                    .first()
                    .expect("/Dest must name a page")
                    .as_reference()
                    .expect("/Dest[0] must be a reference")
            });
            assert_eq!(
                target, new_page_id,
                "/Dest[0] must resolve to the *copied* page, not the source page id"
            );
        }
        other => panic!("unknown bug-0007 child scenario: {other}"),
    }

    // Reached only on: no overflow, Ok result, and cycle rewritten into the copy.
    println!("{OK_MARKER}");
}

/// Re-invokes this test binary to run `test_name` alone, in the child role.
fn spawn_child(test_name: &str, scenario: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("locate the test binary");
    std::process::Command::new(exe)
        .args([test_name, "--exact", "--nocapture"])
        .env(CHILD_ENV, scenario)
        .output()
        .expect("spawn child test process")
}

fn assert_child_ok(out: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "child process did not exit cleanly — bug-0007 regression (deep-copy cycle \
         overflowed the stack).\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains(OK_MARKER),
        "child exited 0 but never reached the success marker, so the copy was not \
         actually exercised (a zero-match filter also exits 0).\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn copy_page_with_self_referencing_p_annotation_does_not_overflow() {
    if let Ok(scenario) = std::env::var(CHILD_ENV) {
        run_child(&scenario);
        return;
    }
    let out = spawn_child(
        "copy_page_with_self_referencing_p_annotation_does_not_overflow",
        "p_backref",
    );
    assert_child_ok(&out);
}

#[test]
fn copy_page_with_self_linking_dest_annotation_does_not_overflow() {
    if let Ok(scenario) = std::env::var(CHILD_ENV) {
        run_child(&scenario);
        return;
    }
    let out = spawn_child(
        "copy_page_with_self_linking_dest_annotation_does_not_overflow",
        "dest_selflink",
    );
    assert_child_ok(&out);
}
