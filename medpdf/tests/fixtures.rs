// tests/fixtures.rs
// Helper functions for creating test PDF documents

#![allow(dead_code)] // These are test utilities; not all are used in every test file

use lopdf::{dictionary, Document, Object, ObjectId, Stream};

/// Creates a minimal valid PDF document with no pages.
pub fn create_empty_pdf() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![],
        "Count" => 0,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// Creates a minimal valid PDF document with the specified number of pages.
/// Each page has default US Letter dimensions (612 x 792 points).
pub fn create_pdf_with_pages(count: usize) -> Document {
    create_pdf_with_pages_and_size(count, 612.0, 792.0)
}

/// Creates a minimal valid PDF document with the specified number of pages
/// and custom page dimensions.
pub fn create_pdf_with_pages_and_size(count: usize, width: f32, height: f32) -> Document {
    let mut doc = create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    for _ in 0..count {
        let media_box = vec![0.0.into(), 0.0.into(), width.into(), height.into()];
        let resources_id = doc.add_object(dictionary! {});
        let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => media_box,
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
        };
        let page_id = doc.add_object(page);

        // Add page to Kids array
        let pages = doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
        let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
        kids.push(page_id.into());

        // Update page count
        let new_page_count = kids.len();
        pages.set("Count", Object::Integer(new_page_count as i64));
    }

    doc
}

/// Creates a PDF with a page containing specific content stream operations.
/// The `content_ops` should be valid PDF content stream commands.
pub fn create_pdf_with_content(content_ops: &[u8]) -> Document {
    let mut doc = create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];
    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, content_ops.to_vec()));

    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    // Add page to Kids array
    let pages = doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
    kids.push(page_id.into());
    pages.set("Count", Object::Integer(1));

    doc
}

/// Creates a PDF with unbalanced q/Q operators for overlay testing.
/// This creates a page with `extra_q` more 'q' operators than 'Q' operators.
pub fn create_pdf_with_unbalanced_q(extra_q: i32) -> Document {
    let mut content = Vec::new();

    // Add the extra q operators
    for _ in 0..extra_q {
        content.extend_from_slice(b"q\n");
    }

    // Add some actual content
    content.extend_from_slice(b"0 0 0 rg\n");
    content.extend_from_slice(b"100 100 200 200 re\n");
    content.extend_from_slice(b"f\n");

    create_pdf_with_content(&content)
}

/// Gets the first page's ObjectId from a document.
pub fn get_first_page_id(doc: &Document) -> ObjectId {
    *doc.get_pages().get(&1).expect("Document has no pages")
}

/// Gets the content stream bytes from a page.
pub fn get_page_content_bytes(doc: &Document, page_id: ObjectId) -> Vec<u8> {
    let page = doc.get_dictionary(page_id).unwrap();
    let contents = page.get(b"Contents").unwrap();

    match contents {
        Object::Reference(id) => {
            let stream = doc.get_object(*id).unwrap().as_stream().unwrap();
            if stream.is_compressed() {
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone())
            } else {
                stream.content.clone()
            }
        }
        Object::Stream(stream) => {
            if stream.is_compressed() {
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone())
            } else {
                stream.content.clone()
            }
        }
        Object::Array(arr) => {
            // Concatenate all content streams
            let mut result = Vec::new();
            for item in arr {
                if let Object::Reference(id) = item {
                    let stream = doc.get_object(*id).unwrap().as_stream().unwrap();
                    let bytes = if stream.is_compressed() {
                        stream
                            .decompressed_content()
                            .unwrap_or_else(|_| stream.content.clone())
                    } else {
                        stream.content.clone()
                    };
                    result.extend_from_slice(&bytes);
                }
            }
            result
        }
        _ => panic!("Unexpected Contents type"),
    }
}

/// Counts occurrences of 'q' and 'Q' operators in content bytes.
pub fn count_q_operators(content: &[u8]) -> (i32, i32) {
    let content_str = String::from_utf8_lossy(content);
    let mut q_count = 0;
    let mut big_q_count = 0;

    // Simple tokenization - this is approximate but works for test purposes
    for token in content_str.split_whitespace() {
        match token {
            "q" => q_count += 1,
            "Q" => big_q_count += 1,
            _ => {}
        }
    }

    (q_count, big_q_count)
}

/// Creates a PDF where pages inherit MediaBox from the Pages node (no per-page MediaBox).
/// This is valid per the PDF spec — MediaBox is inheritable.
pub fn create_pdf_with_inherited_media_box(page_count: usize, width: f32, height: f32) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    // Build pages with MediaBox on the Pages node, not on individual pages
    let mut kid_ids = Vec::new();
    for _ in 0..page_count {
        let resources_id = doc.add_object(dictionary! {});
        let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
        };
        let page_id = doc.add_object(page);
        kid_ids.push(page_id);
    }

    let kids: Vec<Object> = kid_ids.iter().map(|id| Object::Reference(*id)).collect();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => page_count as i64,
        "MediaBox" => vec![0.0.into(), 0.0.into(), width.into(), height.into()],
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// Creates a PDF with a deeply nested page tree (Pages -> Pages -> Page)
/// where MediaBox is only on the root Pages node.
pub fn create_pdf_with_nested_page_tree(width: f32, height: f32) -> Document {
    let mut doc = Document::with_version("1.7");
    let root_pages_id = doc.new_object_id();
    let intermediate_pages_id = doc.new_object_id();

    // Create a leaf page under the intermediate node
    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => intermediate_pages_id,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

    // Intermediate Pages node (no MediaBox, points to root)
    let intermediate_pages = dictionary! {
        "Type" => "Pages",
        "Parent" => root_pages_id,
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    };
    doc.objects
        .insert(intermediate_pages_id, Object::Dictionary(intermediate_pages));

    // Root Pages node with MediaBox
    let root_pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(intermediate_pages_id)],
        "Count" => 1,
        "MediaBox" => vec![0.0.into(), 0.0.into(), width.into(), height.into()],
    };
    doc.objects
        .insert(root_pages_id, Object::Dictionary(root_pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => root_pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// Creates a PDF where the page has no MediaBox anywhere in the tree.
pub fn create_pdf_without_media_box() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let resources_id = doc.add_object(dictionary! {});
    let content_id = doc.add_object(Stream::new(dictionary! {}, vec![]));
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    };
    let page_id = doc.add_object(page);

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

/// Creates a PDF with multiple pages that share a common font resource.
/// This is useful for testing resource deduplication during page copying.
pub fn create_pdf_with_shared_font(page_count: usize) -> Document {
    let mut doc = create_empty_pdf();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();

    // Create a shared font object
    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    };
    let font_id = doc.add_object(font_dict);

    // Create a shared Resources dictionary referencing the font
    let resources = dictionary! {
        "Font" => dictionary! {
            "F1" => Object::Reference(font_id),
        },
    };
    let resources_id = doc.add_object(resources);

    // Create pages that all share the same Resources
    for _ in 0..page_count {
        let media_box = vec![0.0.into(), 0.0.into(), 612.0.into(), 792.0.into()];
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf ET".to_vec()));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => media_box,
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
        };
        let page_id = doc.add_object(page);

        let pages = doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
        let kids = pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap();
        kids.push(page_id.into());
        let new_page_count = kids.len();
        pages.set("Count", Object::Integer(new_page_count as i64));
    }

    doc
}
