# medpdf-image

Image embedding companion crate for [medpdf](https://crates.io/crates/medpdf).

Provides high-level functions for embedding raster images (JPEG, PNG, GIF, BMP, TIFF, WebP) and optionally SVG into PDF documents built with lopdf.

## Features

- **JPEG pass-through** – JPEG files are embedded directly as DCTDecode streams without re-encoding, _unless_ downsampling is required by `max_dpi` (see below), in which case the downsampled JPEG is re-encoded at `jpeg_quality` (default 85)
- **PNG/other formats** – decoded and embedded as FlateDecode streams with alpha support
- **SVG** – optional `svg` feature converts SVG to PDF vector content via svg2pdf (`add_svg`, `load_svg`)
- **Fit modes** – `Stretch`, `Contain`, `Cover` control how the image fills the target box (width and height are both required)
- **DPI limiting** – automatic downscaling to a configurable max DPI; a JPEG whose effective DPI exceeds `max_dpi` is decoded, downsampled, and re-encoded (lossily) at `jpeg_quality`
- **Alpha/opacity** – per-image alpha via ExtGState
- **Rotation** – arbitrary rotation around the image anchor point
- **Recompression** – the `recompress` module (`recompress_images`) re-encodes qualifying FlateDecode image XObjects as DCTDecode (JPEG) to shrink Word-style bloated PDFs

## Installation

```toml
[dependencies]
medpdf-image = "0.4"

# Optional SVG support
medpdf-image = { version = "0.4", features = ["svg"] }
```

## Quick Start

```rust
use lopdf::Document;
use medpdf_image::{DrawImageParams, ImageFit, add_image, load_image};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::load("input.pdf")?;
    // get_pages() is 1-indexed and maps page number → ObjectId.
    let page_id = *doc.get_pages().get(&1).expect("document has a first page");

    let image_data = load_image(std::path::Path::new("logo.png"))?;

    // Required: image_data, x, y, width, height (all in points). Everything else is set
    // through builder methods (shown here with their defaults).
    let params = DrawImageParams::new(image_data, 72.0, 700.0, 200.0, 150.0)
        .fit(ImageFit::Contain)
        .max_dpi(300.0)
        .alpha(1.0)
        .rotation(0.0)
        .layer_over(true);

    add_image(&mut doc, page_id, params)?; // params passed by value
    doc.save("output.pdf")?;
    Ok(())
}
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
