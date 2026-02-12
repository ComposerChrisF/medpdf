use crate::error::{PdfMergeError, Result};
use crate::font_helpers;
use crate::pdf_helpers::{KEY_CONTENTS, KEY_EXTGSTATE, KEY_FONT, KEY_FONT_DESTCRIPTOR, KEY_RESOURCES};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};

/// Counts the net q/Q balance across content streams.
/// Returns the number of unclosed 'q' operations (positive means more q's than Q's).
fn count_q_balance(dest_doc: &Document, content_refs: &[ObjectId]) -> Result<isize> {
    let mut total_balance: isize = 0;

    for &content_id in content_refs {
        let obj = dest_doc.get_object(content_id)?;
        if let Ok(stream) = obj.as_stream() {
            // Need to decompress if necessary
            let content_bytes = if stream.is_compressed() {
                stream.decompressed_content()?
            } else {
                stream.content.clone()
            };

            // Parse and count q/Q operations
            if let Ok(content) = Content::decode(&content_bytes) {
                for operation in content.operations.iter() {
                    match operation.operator.as_str() {
                        "q" => total_balance += 1,
                        "Q" => total_balance -= 1,
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(total_balance)
}

/// Gets all content stream ObjectIds from a page's Contents entry.
fn get_content_stream_ids(dest_doc: &Document, page_id: ObjectId) -> Result<Vec<ObjectId>> {
    let page_dict = dest_doc.get_object(page_id)?.as_dict()?;

    match page_dict.get(KEY_CONTENTS) {
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|obj| obj.as_reference().ok())
            .collect::<Vec<_>>()
            .pipe(Ok),
        Ok(Object::Reference(id)) => Ok(vec![*id]),
        Ok(_) => Err(PdfMergeError::new("Unexpected Contents type")),
        Err(_) => Ok(vec![]), // No contents yet
    }
}

/// Helper trait for pipe syntax
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for Vec<T> {}

/// Converts a UTF-8 string to WinAnsiEncoding (Windows Code Page 1252) bytes.
/// Characters that cannot be represented in WinAnsiEncoding are replaced with '?'.
pub fn utf8_to_winansi(text: &str) -> Vec<u8> {
    text.chars().map(unicode_to_winansi).collect()
}

/// Maps a Unicode codepoint to its WinAnsiEncoding byte value.
/// Returns b'?' for characters not representable in WinAnsiEncoding.
pub fn unicode_to_winansi(c: char) -> u8 {
    let cp = c as u32;
    match cp {
        // ASCII range (0x00-0x7F) - direct mapping
        0x0000..=0x007F => cp as u8,

        // Latin-1 Supplement (0xA0-0xFF) - direct mapping
        0x00A0..=0x00FF => cp as u8,

        // Special WinAnsi characters in 0x80-0x9F range
        // These differ from Latin-1 and need explicit mapping
        0x20AC => 0x80, // Euro
        0x201A => 0x82, // single low-9 quotation mark
        0x0192 => 0x83, // latin small letter f with hook
        0x201E => 0x84, // double low-9 quotation mark
        0x2026 => 0x85, // horizontal ellipsis
        0x2020 => 0x86, // dagger
        0x2021 => 0x87, // double dagger
        0x02C6 => 0x88, // modifier letter circumflex accent
        0x2030 => 0x89, // per mille sign
        0x0160 => 0x8A, // latin capital letter s with caron
        0x2039 => 0x8B, // single left-pointing angle quotation mark
        0x0152 => 0x8C, // latin capital ligature oe
        0x017D => 0x8E, // latin capital letter z with caron
        0x2018 => 0x91, // left single quotation mark
        0x2019 => 0x92, // right single quotation mark
        0x201C => 0x93, // left double quotation mark
        0x201D => 0x94, // right double quotation mark
        0x2022 => 0x95, // bullet
        0x2013 => 0x96, // en dash
        0x2014 => 0x97, // em dash
        0x02DC => 0x98, // small tilde
        0x2122 => 0x99, // trade mark sign
        0x0161 => 0x9A, // latin small letter s with caron
        0x203A => 0x9B, // single right-pointing angle quotation mark
        0x0153 => 0x9C, // latin small ligature oe
        0x017E => 0x9E, // latin small letter z with caron
        0x0178 => 0x9F, // latin capital letter y with diaeresis

        // Character not in WinAnsiEncoding
        _ => b'?',
    }
}

fn add_font_objects(
    dest_doc: &mut Document,
    page_id: (u32, u16),
    font_data: &[u8],
    font_name: &str,
) -> Result<String> {
    if font_data.len() == 1 && font_data[0] != b'@' {
        return Ok(format!("F{}", font_data[0])); // No need to add font objects since we're just reusing existing ones...
    }

    if font_data[0] == b'@' {
        add_known_named_font(dest_doc, page_id, font_name)
    } else {
        add_embedded_font(dest_doc, page_id, font_data)
    }
}

/// Registers a font object in the page's resources and returns the font key.
/// This handles the complex logic of navigating/creating the Resources -> Font hierarchy.
fn register_font_in_page_resources(
    dest_doc: &mut Document,
    page_id: ObjectId,
    font_id: ObjectId,
) -> Result<String> {
    let font_key = format!("F{}", font_id.0);
    let font_key_bytes = font_key.as_bytes().to_vec();

    // Helper to add font reference to a fonts dictionary
    let add_font_to_dict = |dict: &mut Dictionary, key: &[u8], id: ObjectId| {
        dict.set(key.to_vec(), Object::Reference(id));
    };

    // First pass: handle page's Resources (may be inline dict or reference)
    let page_dict = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

    let resources_obj = page_dict.get_mut(KEY_RESOURCES);
    let (mut fonts_id, resources_dict_id) = match resources_obj {
        Ok(Object::Reference(id_resources)) => (None, Some(*id_resources)),
        Ok(Object::Dictionary(dict_resources)) => {
            let fonts_id = handle_fonts_in_resources(dict_resources, &font_key_bytes, font_id)?;
            (fonts_id, None)
        }
        Ok(_) => {
            return Err(PdfMergeError::new(
                "/Resource key of page not a Reference nor a Dictionary!",
            ))
        }
        Err(_) => {
            // No resources yet - create inline
            let mut dict_resources = dictionary! {};
            let fonts_id =
                handle_fonts_in_resources(&mut dict_resources, &font_key_bytes, font_id)?;
            page_dict.set(KEY_RESOURCES, Object::Dictionary(dict_resources));
            (fonts_id, None)
        }
    };

    // Only one of these two is ever set, but both can be None
    // Was: assert!(fonts_id.is_none() || resources_dict_id.is_none());
    if fonts_id.is_some() && resources_dict_id.is_some() {
        return Err(PdfMergeError::new(
            "Internal error: both Fonts and Resources are set!",
        ));
    }

    // Second pass: if Resources was a reference, handle it now
    if let Some(resources_dict_id) = resources_dict_id {
        let resources_dict = dest_doc.get_object_mut(resources_dict_id)?.as_dict_mut()?;
        fonts_id = handle_fonts_in_resources(resources_dict, &font_key_bytes, font_id)?;
    }

    // Third pass: if Fonts was a reference, handle it now
    if let Some(fonts_id) = fonts_id {
        let fonts_dict = dest_doc.get_object_mut(fonts_id)?.as_dict_mut()?;
        add_font_to_dict(fonts_dict, &font_key_bytes, font_id);
    }

    Ok(font_key)
}

/// Handles adding a font to a Resources dictionary's Font entry.
/// Returns Some(ObjectId) if the Font entry is a reference that needs separate handling.
fn handle_fonts_in_resources(
    resources_dict: &mut Dictionary,
    font_key: &[u8],
    font_id: ObjectId,
) -> Result<Option<ObjectId>> {
    match resources_dict.get_mut(KEY_FONT) {
        Ok(Object::Reference(id_fonts)) => Ok(Some(*id_fonts)),
        Ok(Object::Dictionary(dict_fonts)) => {
            dict_fonts.set(font_key.to_vec(), Object::Reference(font_id));
            Ok(None)
        }
        Ok(_) => Err(PdfMergeError::new(
            "/Font key of Resource not a Reference nor a Dictionary!",
        )),
        Err(_) => {
            // No Font dict yet - create inline
            let mut dict_fonts = dictionary! {};
            dict_fonts.set(font_key.to_vec(), Object::Reference(font_id));
            resources_dict.set(KEY_FONT, Object::Dictionary(dict_fonts));
            Ok(None)
        }
    }
}

/// Registers an ExtGState object in the page's resources and returns the gs key.
/// Mirrors the three-tier pattern of `register_font_in_page_resources()`.
fn register_extgstate_in_page_resources(
    dest_doc: &mut Document,
    page_id: ObjectId,
    gs_id: ObjectId,
) -> Result<String> {
    let gs_key = format!("GS{}", gs_id.0);
    let gs_key_bytes = gs_key.as_bytes().to_vec();

    // First pass: handle page's Resources (may be inline dict or reference)
    let page_dict = dest_doc
        .get_object_mut(page_id)?
        .as_dict_mut()
        .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

    let resources_obj = page_dict.get_mut(KEY_RESOURCES);
    let (mut extgstate_ref_id, resources_dict_id) = match resources_obj {
        Ok(Object::Reference(id_resources)) => (None, Some(*id_resources)),
        Ok(Object::Dictionary(dict_resources)) => {
            let extgstate_ref =
                handle_extgstate_in_resources(dict_resources, &gs_key_bytes, gs_id)?;
            (extgstate_ref, None)
        }
        Ok(_) => {
            return Err(PdfMergeError::new(
                "/Resource key of page not a Reference nor a Dictionary!",
            ))
        }
        Err(_) => {
            // No resources yet - create inline
            let mut dict_resources = dictionary! {};
            let extgstate_ref =
                handle_extgstate_in_resources(&mut dict_resources, &gs_key_bytes, gs_id)?;
            page_dict.set(KEY_RESOURCES, Object::Dictionary(dict_resources));
            (extgstate_ref, None)
        }
    };

    if extgstate_ref_id.is_some() && resources_dict_id.is_some() {
        return Err(PdfMergeError::new(
            "Internal error: both ExtGState and Resources are set!",
        ));
    }

    // Second pass: if Resources was a reference, handle it now
    if let Some(resources_dict_id) = resources_dict_id {
        let resources_dict = dest_doc.get_object_mut(resources_dict_id)?.as_dict_mut()?;
        extgstate_ref_id = handle_extgstate_in_resources(resources_dict, &gs_key_bytes, gs_id)?;
    }

    // Third pass: if ExtGState was a reference, handle it now
    if let Some(extgstate_id) = extgstate_ref_id {
        let extgstate_dict = dest_doc.get_object_mut(extgstate_id)?.as_dict_mut()?;
        extgstate_dict.set(gs_key_bytes, Object::Reference(gs_id));
    }

    Ok(gs_key)
}

/// Handles adding an ExtGState entry to a Resources dictionary's ExtGState sub-dictionary.
/// Returns Some(ObjectId) if the ExtGState entry is a reference that needs separate handling.
fn handle_extgstate_in_resources(
    resources_dict: &mut Dictionary,
    gs_key: &[u8],
    gs_id: ObjectId,
) -> Result<Option<ObjectId>> {
    match resources_dict.get_mut(KEY_EXTGSTATE) {
        Ok(Object::Reference(id)) => Ok(Some(*id)),
        Ok(Object::Dictionary(dict)) => {
            dict.set(gs_key.to_vec(), Object::Reference(gs_id));
            Ok(None)
        }
        Ok(_) => Err(PdfMergeError::new(
            "/ExtGState key of Resource not a Reference nor a Dictionary!",
        )),
        Err(_) => {
            // No ExtGState dict yet - create inline
            let mut dict = dictionary! {};
            dict.set(gs_key.to_vec(), Object::Reference(gs_id));
            resources_dict.set(KEY_EXTGSTATE, Object::Dictionary(dict));
            Ok(None)
        }
    }
}

fn add_known_named_font(
    dest_doc: &mut Document,
    page_id: ObjectId,
    font_name: &str,
) -> Result<String> {
    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => font_name.to_string(),
        "Encoding" => "WinAnsiEncoding",
    };
    let font_id = dest_doc.add_object(font_dict);
    register_font_in_page_resources(dest_doc, page_id, font_id)
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

/// Adds text to a page using rich parameters (color, f32 coords, rotation, alignment).
///
/// **Note:** `VAlign::Bottom` currently behaves identically to `VAlign::Baseline`
/// (both use `dy = 0.0`). True bottom alignment would require font descent metrics,
/// which is not yet implemented.
pub fn add_text_params(
    dest_doc: &mut Document,
    page_id: ObjectId,
    params: &crate::types::AddTextParams,
) -> Result<()> {
    let font_key = add_font_objects(dest_doc, page_id, &params.font_data, &params.font_name)?;

    // Measure text width for alignment and/or strikeout/underline
    let needs_width = params.h_align != crate::types::HAlign::Left
        || params.v_align != crate::types::VAlign::Baseline
        || params.strikeout
        || params.underline;
    let text_width = if needs_width {
        crate::font_helpers::measure_text_width(&params.font_data, params.font_size, &params.text)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    // Compute alignment offsets
    let dx = match params.h_align {
        crate::types::HAlign::Left => 0.0,
        crate::types::HAlign::Center => -text_width / 2.0,
        crate::types::HAlign::Right => -text_width,
    };
    let dy = match params.v_align {
        crate::types::VAlign::Baseline => 0.0,
        crate::types::VAlign::Bottom => 0.0, // baseline ~= bottom for simple cases
        crate::types::VAlign::Center => params.font_size * 0.35, // approximate half x-height
        crate::types::VAlign::Top => params.font_size * 0.7, // approximate ascent fraction
    };

    let encoded_text = utf8_to_winansi(&params.text);

    let mut ops = vec![Operation::new("q", vec![])];

    // Apply alpha via ExtGState when not fully opaque
    let alpha = params.color.a;
    if (alpha - 1.0).abs() > f32::EPSILON {
        let gs_dict = dictionary! {
            "Type" => "ExtGState",
            "ca" => alpha,
            "CA" => alpha,
        };
        let gs_id = dest_doc.add_object(gs_dict);
        let gs_key = register_extgstate_in_page_resources(dest_doc, page_id, gs_id)?;
        ops.push(Operation::new(
            "gs",
            vec![Object::Name(gs_key.as_bytes().to_vec())],
        ));
    }

    // Set color
    ops.push(Operation::new(
        "rg",
        vec![
            params.color.r.into(),
            params.color.g.into(),
            params.color.b.into(),
        ],
    ));

    // Apply rotation + translation via cm (concat matrix)
    if params.rotation.abs() > 0.001 {
        let angle = params.rotation.to_radians();
        let cos = angle.cos();
        let sin = angle.sin();
        ops.push(Operation::new(
            "cm",
            vec![
                cos.into(),
                sin.into(),
                (-sin).into(),
                cos.into(),
                params.x.into(),
                params.y.into(),
            ],
        ));
        // After cm, text position is relative to the transformed origin
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new("Tr", vec![0.into()]));
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(font_key.as_bytes().to_vec()),
                params.font_size.into(),
            ],
        ));
        ops.push(Operation::new("Td", vec![dx.into(), dy.into()]));
    } else {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new("Tr", vec![0.into()]));
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(font_key.as_bytes().to_vec()),
                params.font_size.into(),
            ],
        ));
        let final_x = params.x + dx;
        let final_y = params.y + dy;
        ops.push(Operation::new("Td", vec![final_x.into(), final_y.into()]));
    }

    ops.push(Operation::new(
        "Tj",
        vec![Object::String(encoded_text, StringFormat::Literal)],
    ));
    ops.push(Operation::new("ET", vec![]));

    // Draw underline/strikeout rectangles
    // In rotated mode, cm is active so we use (dx, dy) offsets.
    // In non-rotated mode, we use absolute (params.x + dx, params.y + dy) coords.
    if params.underline || params.strikeout {
        let has_rotation = params.rotation.abs() > 0.001;
        let rect_x = if has_rotation { dx } else { params.x + dx };
        let rect_base_y = if has_rotation { dy } else { params.y + dy };
        let line_height = params.font_size * 0.05;
        if params.underline {
            let line_y = rect_base_y - params.font_size * 0.15;
            ops.push(Operation::new(
                "re",
                vec![
                    rect_x.into(),
                    line_y.into(),
                    text_width.into(),
                    line_height.into(),
                ],
            ));
            ops.push(Operation::new("f", vec![]));
        }
        if params.strikeout {
            let line_y = rect_base_y + params.font_size * 0.3;
            ops.push(Operation::new(
                "re",
                vec![
                    rect_x.into(),
                    line_y.into(),
                    text_width.into(),
                    line_height.into(),
                ],
            ));
            ops.push(Operation::new("f", vec![]));
        }
    }

    ops.push(Operation::new("Q", vec![]));

    let content = Content { operations: ops };
    let content_stream = Stream::new(dictionary! {}, content.encode()?);
    let content_id = dest_doc.add_object(content_stream);

    let (q_id, closing_q_id) = if params.layer_over {
        let existing_content_ids = get_content_stream_ids(dest_doc, page_id)?;
        let q_balance = count_q_balance(dest_doc, &existing_content_ids)?;

        let q_stream = Stream::new(dictionary! {}, b"q\n".to_vec());
        let q_id = dest_doc.add_object(q_stream);

        let num_closing_qs = 1 + q_balance.max(0) as usize;
        let closing_content = "Q\n".repeat(num_closing_qs);
        let closing_stream = Stream::new(dictionary! {}, closing_content.into_bytes());
        let closing_q_id = dest_doc.add_object(closing_stream);

        (Some(q_id), Some(closing_q_id))
    } else {
        (None, None)
    };

    {
        let page_dict = dest_doc
            .get_object_mut(page_id)?
            .as_dict_mut()
            .map_err(|_| PdfMergeError::new("Page object is not a dictionary"))?;

        if let Ok(contents) = page_dict.get_mut(KEY_CONTENTS) {
            match contents {
                Object::Array(ref mut arr) => {
                    if params.layer_over {
                        arr.insert(0, Object::Reference(q_id.unwrap()));
                        arr.push(Object::Reference(closing_q_id.unwrap()));
                        arr.push(Object::Reference(content_id));
                    } else {
                        arr.insert(0, Object::Reference(content_id));
                    }
                }
                Object::Reference(id) => {
                    let old_id = *id;
                    *contents = if params.layer_over {
                        Object::Array(vec![
                            Object::Reference(q_id.unwrap()),
                            Object::Reference(old_id),
                            Object::Reference(closing_q_id.unwrap()),
                            Object::Reference(content_id),
                        ])
                    } else {
                        Object::Array(vec![
                            Object::Reference(content_id),
                            Object::Reference(old_id),
                        ])
                    };
                }
                _ => return Err(PdfMergeError::new("Unexpected page Contents type")),
            }
        } else {
            page_dict.set(
                KEY_CONTENTS,
                Object::Array(vec![Object::Reference(content_id)]),
            );
        }
    }

    Ok(())
}

fn add_embedded_font(
    dest_doc: &mut Document,
    page_id: ObjectId,
    font_data: &[u8],
) -> Result<String> {
    let (font_info, font_descriptor) = font_helpers::get_pdf_font_info_of_data(font_data)?;
    let mut font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" =>  font_info.subtype,
        "BaseFont" => font_info.base_font,
        "FirstChar" => font_info.first_char,
        "LastChar" => font_info.last_char,
        "Widths" => widths_as_object_array(&font_info.widths[..]),
    };

    // Only add Encoding if present (symbol fonts omit it)
    if let Some(ref encoding) = font_info.encoding {
        font_dict.set("Encoding", Object::Name(encoding.as_bytes().to_vec()));
    }
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
        "Length1" => font_data.len() as i64,
    };
    let font_file = Stream::new(font_file_dict, font_data.into());
    let font_file_id = dest_doc.add_object(font_file);
    descriptor_dict.set(font_descriptor.font_file_key, font_file_id);
    let descriptor_id = dest_doc.add_object(descriptor_dict);
    font_dict.set(KEY_FONT_DESTCRIPTOR, descriptor_id);

    let font_id = dest_doc.add_object(font_dict);
    register_font_in_page_resources(dest_doc, page_id, font_id)
}
