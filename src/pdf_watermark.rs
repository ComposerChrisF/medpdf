use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, Dictionary};
use crate::error::{Result, PdfMergeError};
use crate::font_helpers;
use crate::pdf_helpers::{KEY_CONTENTS, KEY_FONT, KEY_FONT_DESTCRIPTOR, KEY_RESOURCES};


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
    let font_key = add_font_objects(dest_doc, page_id, font_data, font_name)?;

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
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
            Operation::new("Q", vec![]),
        ],
    };
    let content_stream = Stream::new(dictionary! {}, content.encode()?);
    let content_id = dest_doc.add_object(content_stream);

    {
        let page_dict = dest_doc
            .get_object_mut(page_id)?
            .as_dict_mut()
            .or_else(|_| Err(PdfMergeError::new("Page object is not a dictionary")))?;

        if let Ok(contents) = page_dict.get_mut(KEY_CONTENTS) {
            match contents {
                Object::Array(ref mut arr) => { arr.insert(0, Object::Reference(content_id)); },
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
            page_dict.set(KEY_CONTENTS, Object::Array(vec![Object::Reference(content_id)]));
        }
    }

    Ok(())
}

fn add_font_objects(dest_doc: &mut Document, page_id: (u32, u16), font_data: &[u8], font_name: &str) -> Result<String> {
    if font_data.len() == 1 && font_data[0] != '@' as u8 {
        return Ok(format!("F{}", font_data[0]));        // No need to add font objects since we're just reusing existing ones...
    }

    if font_data[0] == '@' as u8 {
        add_known_named_font(dest_doc, page_id, font_name)
    } else {
        add_embedded_font(dest_doc, page_id, font_data)
    }
}

fn add_known_named_font(dest_doc: &mut Document, page_id: (u32, u16), font_name: &str) -> Result<String> {
    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => font_name.to_string(),
    };
    let font_id = dest_doc.add_object(font_dict);
    let font_key =format!("F{}", font_id.0);
    let fn_add_font_to_fonts_dict = |dict: &mut Dictionary| { dict.set(font_key.as_bytes(), Object::Reference(font_id)); };

    let fn_add_fonts_to_resources_and_add_font = |resources_dict: &mut Dictionary| -> Result<Option<ObjectId>> {
        let fonts_obj = resources_dict.get_mut(KEY_FONT);
        let fonts_id = match fonts_obj {
            Ok(Object::Reference(id_fonts)) => Some(*id_fonts),
            Ok(Object::Dictionary(dict_fonts)) => { fn_add_font_to_fonts_dict(dict_fonts); None }
            Ok(_) => { return Err(PdfMergeError::new("/Font key of Resource not a Reference nor a Dictionary!")); }
            Err(_) => {
                let mut dict_fonts = dictionary! { };
                fn_add_font_to_fonts_dict(&mut dict_fonts);
                resources_dict.set(KEY_FONT,Object::Dictionary(dict_fonts));
                None
            }
        };
        Ok(fonts_id)
    };

    let page_dict = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

    let resources_obj = page_dict.get_mut(KEY_RESOURCES);
    let (mut fonts_id, resources_dict_id) = match resources_obj {
        Ok(Object::Reference(id_resources)) => (None, Some(*id_resources)),
        Ok(Object::Dictionary(dict_resources)) => (fn_add_fonts_to_resources_and_add_font(dict_resources)?, None),
        Ok(_) => { return Err(PdfMergeError::new("/Resource key of page not a Reference nor a Dictionary!")); }
        Err(_) => {
            let mut dict_resources = dictionary! { };
            let fonts_id = fn_add_fonts_to_resources_and_add_font(&mut dict_resources)?;
            page_dict.set(KEY_RESOURCES, Object::Dictionary(dict_resources));
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

fn widths_as_object_array(widths: &[u16]) -> Object {
    Object::Array(widths.iter().map(|v| Object::Integer(*v as i64)).collect())
}

fn bbox_as_object_array(bbox: &[i16]) -> Object {
    assert!(bbox.len() == 4);
    Object::Array(vec![
        Object::Integer(bbox[0] as i64),
        Object::Integer(bbox[1] as i64),
        Object::Integer(bbox[2] as i64),
        Object::Integer(bbox[3] as i64),
    ])
}

fn add_embedded_font(dest_doc: &mut Document, page_id: (u32, u16), font_data: &[u8]) -> Result<String> {
    let (font_info, font_descriptor) = font_helpers::get_pdf_font_info_of_data(font_data)?;
    let mut font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" =>  font_info.subtype,
        "BaseFont" => font_info.base_font,
        "Encoding" => font_info.encoding,
        "FirstChar" => font_info.first_char,
        "LastChar" => font_info.last_char,
        "Widths" => widths_as_object_array(&font_info.widths[..]),
    };
    let mut descriptor_dict = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => font_descriptor.font_name,
        "Flags" => font_descriptor.flags,
        "FontBBox" => bbox_as_object_array(&font_descriptor.font_bbox[..]),
        "ItalicAngle" => font_descriptor.italic_angle,
        "Ascent" => font_descriptor.ascent,
        "Descent" => font_descriptor.descent,
        "CapHeight" => font_descriptor.cap_height,
        "StemV" => font_descriptor.stem_v,
        "Leading" => font_descriptor.leading,
        "XHeight" => font_descriptor.x_height,
    };
    let font_file_dict = dictionary! {
        //"Subtype" => font_descriptor.embedded_font_subtype,
    };
    let font_file = Stream::new(font_file_dict, font_data.into());
    let font_file_id = dest_doc.add_object(font_file);
    descriptor_dict.set(font_descriptor.font_file_key, font_file_id);
    let descriptor_id = dest_doc.add_object(descriptor_dict);
    font_dict.set(KEY_FONT_DESTCRIPTOR, descriptor_id);

    let font_id = dest_doc.add_object(font_dict);
    let font_key =format!("F{}", font_id.0);
    let fn_add_font_to_fonts_dict = |dict: &mut Dictionary| { dict.set(font_key.as_bytes(), Object::Reference(font_id)); };

    let fn_add_fonts_to_resources_and_add_font = |resources_dict: &mut Dictionary| -> Result<Option<ObjectId>> {
        let fonts_obj = resources_dict.get_mut(KEY_FONT);
        let fonts_id = match fonts_obj {
            Ok(Object::Reference(id_fonts)) => Some(*id_fonts),
            Ok(Object::Dictionary(dict_fonts)) => { fn_add_font_to_fonts_dict(dict_fonts); None }
            Ok(_) => { return Err(PdfMergeError::new("/Font key of Resource not a Reference nor a Dictionary!")); }
            Err(_) => {
                let mut dict_fonts = dictionary! { };
                fn_add_font_to_fonts_dict(&mut dict_fonts);
                resources_dict.set(KEY_FONT,Object::Dictionary(dict_fonts));
                None
            }
        };
        Ok(fonts_id)
    };

    let page_dict = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

    let resources_obj = page_dict.get_mut(KEY_RESOURCES);
    let (mut fonts_id, resources_dict_id) = match resources_obj {
        Ok(Object::Reference(id_resources)) => (None, Some(*id_resources)),
        Ok(Object::Dictionary(dict_resources)) => (fn_add_fonts_to_resources_and_add_font(dict_resources)?, None),
        Ok(_) => { return Err(PdfMergeError::new("/Resource key of page not a Reference nor a Dictionary!")); }
        Err(_) => {
            let mut dict_resources = dictionary! { };
            let fonts_id = fn_add_fonts_to_resources_and_add_font(&mut dict_resources)?;
            page_dict.set(KEY_RESOURCES, Object::Dictionary(dict_resources));
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
