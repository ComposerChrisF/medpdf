// tests/place_page_nonfinite_params_regression.rs
//
// Regression test for bugs/bug-0026: place_page validated only `scale` for finiteness. A
// NaN or infinity in x, y, or rotation flowed into the cm/re operands and serialized as
// the literal tokens `NaN`/`inf` — not valid PDF numbers, so viewers saw a corrupt
// content stream, silently. The fix validates x, y, scale, and rotation, naming the
// offending field.
//
// To confirm these pin the fix, set MEDPDF_TEMP_BUG0026 (a temporary guard that restores
// the scale-only check): the x/y/rotation cases then return Ok instead of Err.

mod fixtures;

use medpdf::place_page;
use medpdf::types::PlacePageParams;

fn place_with(params: PlacePageParams) -> medpdf::Result<()> {
    let source = fixtures::create_pdf_with_content(b"q\n0 0 100 100 re f\nQ\n");
    let mut dest = fixtures::create_pdf_with_pages(1);
    let dest_page_id = fixtures::get_first_page_id(&dest);
    place_page(&mut dest, dest_page_id, &source, 1, &params)
}

#[test]
fn nonfinite_x_is_rejected() {
    let r = place_with(PlacePageParams::new(f64::NAN, 0.0, 1.0));
    assert!(
        r.is_err(),
        "NaN x must be rejected, not written as `NaN` — bug-0026"
    );
    assert!(
        r.unwrap_err().to_string().contains('x'),
        "error should name the x field"
    );
}

#[test]
fn nonfinite_y_is_rejected() {
    let r = place_with(PlacePageParams::new(0.0, f64::INFINITY, 1.0));
    assert!(
        r.is_err(),
        "infinite y must be rejected, not written as `inf` — bug-0026"
    );
}

#[test]
fn nonfinite_rotation_is_rejected() {
    let r = place_with(PlacePageParams::new(0.0, 0.0, 1.0).rotation(f64::NAN));
    assert!(r.is_err(), "NaN rotation must be rejected — bug-0026");
}

#[test]
fn finite_params_still_succeed() {
    // The guard must not reject valid placements.
    let r = place_with(PlacePageParams::new(10.0, 20.0, 0.5).rotation(45.0));
    assert!(r.is_ok(), "a fully finite placement must still succeed");
}
