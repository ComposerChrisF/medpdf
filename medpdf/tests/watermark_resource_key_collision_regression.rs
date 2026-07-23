// tests/watermark_resource_key_collision_regression.rs
//
// Regression test for bugs/bug-0037: the watermark path named its resources after
// the new object's id (`F{id}`, `GS{id}`) and wrote them into the page's /Resources
// sub-dictionary with an unconditional `set`. If the page already had a resource
// under that name — `F{objid}` is exactly the scheme medpdf itself emits, so its own
// round-tripped output is a natural candidate — the existing binding was silently
// replaced, and every original text run using that key then rendered in the
// watermark font.
//
// The fix (unique_resource_key) checks the page's effective sub-dictionary: on a
// collision with a *different* object it preserves the existing binding and derives
// a unique key (F{id}_w) via the same find_unique_name machinery overlay uses. The
// content stream only needs *some* key; uniqueness is the invariant.
//
// To confirm this test pins the fix, revert register_font_in_page_resources to
// `format!("F{}", font_id.0)` + unconditional register; existing_key_survives_collision
// then fails (the pre-existing key rebinds to the watermark's Helvetica).

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use medpdf::{AddTextParams, EmbeddedFontCache, FontData, add_text_params};

/// Builds a one-page doc whose page has a /Resources (by reference) with an empty
/// inline /Font sub-dict, plus a separate Times-Roman font object used by existing
/// content. Returns (doc, page_id, resources_id, times_id).
fn doc_with_page_and_times() -> (Document, ObjectId, ObjectId, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    // The page's existing content font.
    let times_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Times-Roman",
        "Encoding" => "WinAnsiEncoding",
    });

    let resources_id = doc.add_object(dictionary! {
        "Font" => Object::Dictionary(Dictionary::new()),
    });
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /Fexisting 12 Tf (hi) Tj ET".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    (doc, page_id, resources_id, times_id)
}

/// Reads the /Font entry key `key` in the page's Resources, returning the object it
/// binds to.
fn font_binding(doc: &Document, resources_id: ObjectId, key: &str) -> Option<ObjectId> {
    let resources = doc.get_dictionary(resources_id).ok()?;
    let font = resources.get(b"Font").ok()?.as_dict().ok()?;
    font.get(key.as_bytes()).ok()?.as_reference().ok()
}

#[test]
fn existing_key_survives_collision() {
    let (mut doc, page_id, resources_id, times_id) = doc_with_page_and_times();

    // The watermark's built-in Helvetica font dict is the FIRST object add_text_params
    // adds, so it will receive id (max_id + 1). Seed the page's /Font with a key
    // spelled exactly that — "F{max_id+1}" — bound to Times-Roman, the collision the
    // watermark path used to clobber.
    let predicted_id = doc.max_id + 1;
    let colliding_key = format!("F{predicted_id}");
    {
        let resources = doc
            .get_object_mut(resources_id)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        let font = resources.get_mut(b"Font").unwrap().as_dict_mut().unwrap();
        font.set(
            colliding_key.as_bytes().to_vec(),
            Object::Reference(times_id),
        );
    }
    // No object was added, so the prediction still holds.
    assert_eq!(doc.max_id + 1, predicted_id);

    let mut cache = EmbeddedFontCache::new();
    let params = AddTextParams::new("DRAFT", FontData::BuiltIn("Helvetica".into()), "Helvetica");
    add_text_params(&mut doc, page_id, &params, &mut cache).expect("watermark must succeed");

    // The pre-existing key must STILL resolve to Times-Roman — it must not have been
    // rebound to the watermark's Helvetica (bug-0037).
    let bound = font_binding(&doc, resources_id, &colliding_key)
        .expect("the pre-existing font key must still be present");
    assert_eq!(
        bound, times_id,
        "existing key {colliding_key} must still bind Times-Roman, not the watermark font (bug-0037)"
    );
    assert_eq!(
        doc.get_dictionary(bound)
            .unwrap()
            .get(b"BaseFont")
            .unwrap()
            .as_name()
            .unwrap(),
        b"Times-Roman",
    );

    // The watermark font (id = predicted_id, Helvetica) must have been registered
    // under a *different*, non-colliding key so its own Tf operator resolves.
    let wm_key = format!("F{predicted_id}_w");
    let wm_bound = font_binding(&doc, resources_id, &wm_key)
        .expect("the watermark font must be registered under a renamed, collision-free key");
    assert_eq!(
        wm_bound,
        (predicted_id, 0),
        "renamed key must bind the new watermark font"
    );
    assert_eq!(
        doc.get_dictionary(wm_bound)
            .unwrap()
            .get(b"BaseFont")
            .unwrap()
            .as_name()
            .unwrap(),
        b"Helvetica",
    );
}

#[test]
fn no_collision_keeps_natural_key() {
    // With no pre-existing key clash, the natural F{id} scheme is preserved (stable
    // output; the fix must not perturb the common case).
    let (mut doc, page_id, resources_id, _times) = doc_with_page_and_times();
    let predicted_id = doc.max_id + 1;

    let mut cache = EmbeddedFontCache::new();
    let params = AddTextParams::new("DRAFT", FontData::BuiltIn("Helvetica".into()), "Helvetica");
    add_text_params(&mut doc, page_id, &params, &mut cache).unwrap();

    let natural = format!("F{predicted_id}");
    let bound = font_binding(&doc, resources_id, &natural)
        .expect("natural key F{id} must be used when free");
    assert_eq!(bound, (predicted_id, 0));
}
