# medpdf workspace

A Rust workspace for medium-level PDF manipulation, built on [lopdf](https://github.com/J-F-Liu/lopdf).

## Crates

| Crate | Description | crates.io |
|-------|-------------|-----------|
| [medpdf](medpdf/) | Medium-level PDF API – page copying, overlays, watermarks, font handling | [![crates.io](https://img.shields.io/crates/v/medpdf.svg)](https://crates.io/crates/medpdf) |
| [medpdf-image](medpdf-image/) | Image embedding companion – JPEG, PNG, SVG, and more | [![crates.io](https://img.shields.io/crates/v/medpdf-image.svg)](https://crates.io/crates/medpdf-image) |
| [pdf-test-visual](pdf-test-visual/) | Visual regression testing utilities (internal, not published) | – |

## Quick Start

```toml
[dependencies]
medpdf = "0.15"
medpdf-image = "0.4"  # if you need image embedding
```

```rust
use lopdf::{Document, Object, dictionary};
use medpdf::{copy_page, create_blank_page, parse_page_spec, Result};

fn main() -> Result<()> {
    let source = Document::load("input.pdf")?;

    // A fresh lopdf document has no catalog; medpdf's page operations require a
    // /Root catalog with a /Pages node, so build that skeleton first.
    let mut dest = Document::with_version("1.5");
    let pages_id = dest.new_object_id();
    dest.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![], "Count" => 0 }),
    );
    let catalog_id = dest.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    dest.trailer.set("Root", catalog_id);

    let pages = parse_page_spec("1-3,5", source.get_pages().len() as u32)?;
    for page in pages {
        copy_page(&mut dest, &source, page)?;
    }

    create_blank_page(&mut dest, 612.0, 792.0)?;
    dest.save("output.pdf")?;
    Ok(())
}
```

See individual crate READMEs for detailed API documentation.

## Building

```bash
cargo build --workspace
cargo test --workspace
```

## License

Licensed under either of [Apache License, Version 2.0](medpdf/LICENSE-APACHE) or [MIT License](medpdf/LICENSE-MIT) at your option.
