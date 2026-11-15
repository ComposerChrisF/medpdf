// Throwaway repro: register a new font on a page whose /Resources are inherited.
use lopdf::{Document, Object, dictionary};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut doc = Document::load(&a[1]).expect("load");
    let page_id = *doc.get_pages().get(&1).expect("page 1");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
    });
    medpdf::register_in_page_resources(&mut doc, page_id, b"Font", b"F9", font).expect("register");
    let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
    match page.get(b"Resources") {
        Ok(Object::Dictionary(d)) => println!("page's own /Resources now: {d:?}"),
        other => println!("page /Resources: {other:?}"),
    }
    doc.save(&a[2]).expect("save");
}
