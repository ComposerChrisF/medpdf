// Reproducer for bug-0045: copy into medpdf/tests/ and run with --nocapture.
use lopdf::{Document, Object, Stream, dictionary};

#[test]
fn probe() {
    let mut src = Document::with_version("1.7");
    let root_id = src.new_object_id();
    let mid_id = src.new_object_id();
    let f1 = src.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = src.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf (x) Tj ET".to_vec()));
    let page = src.add_object(dictionary! {
        "Type" => "Page", "Parent" => mid_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content,
    });
    src.objects.insert(mid_id, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Parent" => root_id, "Kids" => vec![page.into()], "Count" => 1,
        "Resources" => Object::Null,
    }));
    src.objects.insert(root_id, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => vec![mid_id.into()], "Count" => 1,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => f1 } },
    }));
    let cat = src.add_object(dictionary! { "Type" => "Catalog", "Pages" => root_id });
    src.trailer.set("Root", cat);

    println!("resolve: {:?}", medpdf::pdf_helpers::get_page_resources(&src, page));

    let mut dest = Document::with_version("1.7");
    let dpages = dest.new_object_id();
    dest.objects.insert(dpages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 }));
    let dcat = dest.add_object(dictionary! { "Type" => "Catalog", "Pages" => dpages });
    dest.trailer.set("Root", dcat);
    let new_id = medpdf::copy_page(&mut dest, &src, 1).unwrap();
    println!("copied page /Resources: {:?}", dest.get_dictionary(new_id).unwrap().get(b"Resources"));
}
