use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use crate::error::{Result, PdfMergeError};


/// Adds text to a page at a specific position.
pub fn add_text(
    dest_doc: &mut Document,
    page_id: ObjectId,
    text: &str,
    font_data: &[u8], // TODO: Embed font data
    font_name: &str,
    font_size: f32,
    x: i32,
    y: i32,
) -> Result<()> {
    let font_key = add_font_info(dest_doc, page_id, font_data, font_name)?;
    println!("Font key = {font_key}");

    let content = Content {
        operations: vec![
            Operation::new("rg", vec![0.into(), 0.0.into(), 0.51.into()]),
            Operation::new("BT", vec![]),
            Operation::new("Tr", vec![0.into()]),
            Operation::new("Tf", vec![
                Object::Name(font_key.as_bytes().to_vec()),
                font_size.into(),
            ]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    println!("Content={content:?}");
    let content_stream = Stream::new(dictionary! {}, content.encode()?);
    let content_id = dest_doc.add_object(content_stream);

    {
        let page_dict = dest_doc
            .get_object_mut(page_id)?
            .as_dict_mut()
            .or_else(|_| Err(PdfMergeError::new("Page object is not a dictionary")))?;

        if let Ok(contents) = page_dict.get_mut(b"Contents") {
            match contents {
                Object::Array(ref mut arr) => { arr.insert(0, Object::Reference(content_id)); println!("Added Contents to Array!"); },
                Object::Reference(id) => {
                    let old_id = *id;
                    *contents =
                        Object::Array(vec![Object::Reference(old_id), Object::Reference(content_id)]);
                }
                _ => {
                    return Err(PdfMergeError::new("Unexpected page Contents type"))
                }
            }
        } else {
            page_dict.set(b"Contents", Object::Array(vec![Object::Reference(content_id)]));
        }
    }

    Ok(())
}

fn add_font_info(dest_doc: &mut Document, page_id: (u32, u16), font_data: &[u8], font_name: &str) -> Result<String> {
    if font_data.len() == 1 && font_data[0] != '@' as u8 {
        return Ok(format!("F{}", font_data[0]));
    }

    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => if font_data[0] == ('@' as u8) { font_name[1..].to_string() } else { font_name.to_string() },
    };
    let font_id = dest_doc.add_object(font_dict);
    let font_key =format!("F{}", font_id.0);
    let fn_add_font_to_fonts_dict = |dict: &mut Dictionary| { dict.set(font_key.as_bytes(), Object::Reference(font_id)); };

    let fn_add_fonts_to_resources_and_add_font = |resources_dict: &mut Dictionary| -> Result<Option<ObjectId>> {
        let fonts_obj = resources_dict.get_mut(b"Font");
        let fonts_id = match fonts_obj {
            Ok(Object::Reference(id_fonts)) => Some(*id_fonts),
            Ok(Object::Dictionary(dict_fonts)) => { fn_add_font_to_fonts_dict(dict_fonts); None }
            Ok(_) => { return Err(PdfMergeError::new("/Font key of Resource not a Reference nor a Dictionary!")); }
            Err(_) => {
                let mut dict_fonts = dictionary! { };
                fn_add_font_to_fonts_dict(&mut dict_fonts);
                resources_dict.set(b"Font",Object::Dictionary(dict_fonts));
                None
            }
        };
        Ok(fonts_id)
    };

    let page_dict = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

    let resources_obj = page_dict.get_mut(b"Resources");
    let (mut fonts_id, resources_dict_id) = match resources_obj {
        Ok(Object::Reference(id_resources)) => (None, Some(*id_resources)),
        Ok(Object::Dictionary(dict_resources)) => (fn_add_fonts_to_resources_and_add_font(dict_resources)?, None),
        Ok(_) => { return Err(PdfMergeError::new("/Resource key of page not a Reference nor a Dictionary!")); }
        Err(_) => {
            let mut dict_resources = dictionary! { };
            let fonts_id = fn_add_fonts_to_resources_and_add_font(&mut dict_resources)?;
            page_dict.set(b"Resources", Object::Dictionary(dict_resources));
            (fonts_id, None)
        }
    };
    assert!(fonts_id.is_none() || resources_dict_id.is_none());  // Only one of these two is ever set, but both can be None

    if let Some(resources_dict_id) = resources_dict_id {
        let resources_dict = dest_doc.get_object_mut(resources_dict_id)?.as_dict_mut()?;
        assert!(fonts_id.is_none());       // If we entered this branch, then fonts_id should not be set yet!
        fonts_id = fn_add_fonts_to_resources_and_add_font(resources_dict)?;
    }

    if let Some(fonts_id) = fonts_id {
        let fonts_dict = dest_doc.get_object_mut(fonts_id)?.as_dict_mut()?;
        fn_add_font_to_fonts_dict(fonts_dict);
    }

    Ok(font_key)
}
