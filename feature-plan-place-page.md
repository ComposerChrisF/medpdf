# Feature Plan: Scaled/Positioned Page Placement

## Problem

Both booklet imposition and N-up layout require placing a source PDF page onto a target page at a specific position and scale. The existing `overlay_page()` always places content at the origin (0,0) with no scaling. There is no way to compose multiple scaled source pages onto a single output sheet.

## Proposed API

```rust
/// Place a page from one document onto a page in another document,
/// applying translation and uniform scaling via a PDF transform matrix.
pub fn place_page(
    target_doc: &mut Document,
    target_page_idx: usize,
    source_doc: &Document,
    source_page_idx: usize,
    params: &PlacePageParams,
) -> Result<(), MedpdfError>;
```

### PlacePageParams

```rust
pub struct PlacePageParams {
    /// X offset on the target page (in points, from left)
    pub x: f64,
    /// Y offset on the target page (in points, from bottom)
    pub y: f64,
    /// Uniform scale factor (1.0 = no scaling, 0.5 = half size)
    pub scale: f64,
}
```

## How It Differs from overlay_page()

| | `overlay_page()` | `place_page()` |
|---|---|---|
| Position | Always (0,0) | Configurable (x, y) |
| Scaling | None (1:1) | Uniform scale factor |
| Use case | Full-page overlays (letterhead, watermarks) | Imposition (booklet, N-up) |

Both share the same underlying mechanism: copying a source page's resources into the target document (with renaming to avoid conflicts) and appending a content stream. `place_page()` wraps the content stream in a `q ... cm ... Q` graphics state block with the appropriate transform matrix.

## Implementation Notes

### Transform Matrix

PDF uses a 6-element matrix `[a b c d e f]` via the `cm` operator. For translate + uniform scale:

```
a = scale,  b = 0
c = 0,      d = scale
e = x,      f = y
```

The content stream wrapper becomes:
```
q
{scale} 0 0 {scale} {x} {y} cm
{source content stream}
Q
```

### Reuse of overlay_page() Internals

The resource copying and renaming logic from `pdf_overlay.rs` can be shared. The main difference is wrapping the source content stream in a transform. Options:
- Refactor `overlay_page()` to accept optional transform params internally, with `place_page()` as the new public API and `overlay_page()` as a convenience wrapper (transform = identity)
- Or keep them separate if the overlay code is too entangled

### Clipping

For N-up, source pages should be clipped to their MediaBox so they don't bleed into adjacent slots. Add a `re W n` clip rectangle before the source content:

```
q
{x} {y} {scaled_width} {scaled_height} re W n
{scale} 0 0 {scale} {x} {y} cm
{source content stream}
Q
```

## Callers

- **pdf-maker** — new `--booklet` and `--n-up` CLI flags
- **pdf-orchestrator** — new `<Booklet>` and `<NUp>` XML elements

All page ordering logic (booklet imposition order, back cover preservation, N-up normal vs. multi-copy mode, padding) lives in the callers. medpdf only provides the primitive: place one page onto another with a transform.

## Why Not Python

This requires manipulating PDF content streams and resource dictionaries at the object level — exactly what medpdf already does for overlays. A Python approach would duplicate all of that logic poorly.

## Testing

- Place a source page at various positions and scales; verify visually
- Round-trip: place, save, re-read, confirm resources and content streams are intact
- Multiple placements on one target page (the N-up case)
- Source pages with different MediaBox sizes
- Source pages with existing transforms/rotations
- Clipping: verify content doesn't extend beyond the placed region
