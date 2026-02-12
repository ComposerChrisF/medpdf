// tests/unit_conversion_tests.rs
// Tests for Unit::to_points conversion

use medpdf::pdf_helpers::Unit;

const EPSILON: f32 = 0.0001;

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < EPSILON
}

// --- Inches to Points ---

#[test]
fn test_inches_one() {
    // 1 inch = 72 points
    let points = Unit::In.to_points(1.0);
    assert!(approx_eq(points, 72.0), "Expected 72.0, got {}", points);
}

#[test]
fn test_inches_half() {
    // 0.5 inch = 36 points
    let points = Unit::In.to_points(0.5);
    assert!(approx_eq(points, 36.0), "Expected 36.0, got {}", points);
}

#[test]
fn test_inches_zero() {
    let points = Unit::In.to_points(0.0);
    assert!(approx_eq(points, 0.0), "Expected 0.0, got {}", points);
}

#[test]
fn test_inches_negative() {
    // Negative values should work (for positioning)
    let points = Unit::In.to_points(-1.0);
    assert!(approx_eq(points, -72.0), "Expected -72.0, got {}", points);
}

#[test]
fn test_inches_fractional() {
    // 1.25 inches = 90 points
    let points = Unit::In.to_points(1.25);
    assert!(approx_eq(points, 90.0), "Expected 90.0, got {}", points);
}

#[test]
fn test_inches_large() {
    // 8.5 inches (US Letter width) = 612 points
    let points = Unit::In.to_points(8.5);
    assert!(approx_eq(points, 612.0), "Expected 612.0, got {}", points);
}

#[test]
fn test_inches_us_letter_height() {
    // 11 inches (US Letter height) = 792 points
    let points = Unit::In.to_points(11.0);
    assert!(approx_eq(points, 792.0), "Expected 792.0, got {}", points);
}

// --- Millimeters to Points ---

#[test]
fn test_mm_one_inch_equivalent() {
    // 25.4 mm = 1 inch = 72 points
    let points = Unit::Mm.to_points(25.4);
    assert!(approx_eq(points, 72.0), "Expected 72.0, got {}", points);
}

#[test]
fn test_mm_one() {
    // 1 mm = 72/25.4 = ~2.834645669 points
    let points = Unit::Mm.to_points(1.0);
    let expected = 72.0 / 25.4;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

#[test]
fn test_mm_zero() {
    let points = Unit::Mm.to_points(0.0);
    assert!(approx_eq(points, 0.0), "Expected 0.0, got {}", points);
}

#[test]
fn test_mm_negative() {
    let points = Unit::Mm.to_points(-25.4);
    assert!(approx_eq(points, -72.0), "Expected -72.0, got {}", points);
}

#[test]
fn test_mm_a4_width() {
    // A4 width = 210mm = ~595.276 points
    let points = Unit::Mm.to_points(210.0);
    let expected = 210.0 * 72.0 / 25.4;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

#[test]
fn test_mm_a4_height() {
    // A4 height = 297mm = ~841.89 points
    let points = Unit::Mm.to_points(297.0);
    let expected = 297.0 * 72.0 / 25.4;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

#[test]
fn test_mm_small() {
    // 0.1 mm
    let points = Unit::Mm.to_points(0.1);
    let expected = 0.1 * 72.0 / 25.4;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

// --- Conversion Consistency ---

#[test]
fn test_inches_and_mm_equivalence() {
    // 1 inch should equal 25.4 mm in points
    let inch_points = Unit::In.to_points(1.0);
    let mm_points = Unit::Mm.to_points(25.4);
    assert!(
        approx_eq(inch_points, mm_points),
        "1 inch ({}) should equal 25.4mm ({}) in points",
        inch_points,
        mm_points
    );
}

#[test]
fn test_double_conversion_consistency() {
    // Converting twice should scale properly
    let points_1 = Unit::In.to_points(1.0);
    let points_2 = Unit::In.to_points(2.0);
    assert!(approx_eq(points_2, points_1 * 2.0));
}

#[test]
fn test_mm_conversion_ratio() {
    // The ratio should be constant
    let points_10 = Unit::Mm.to_points(10.0);
    let points_20 = Unit::Mm.to_points(20.0);
    assert!(approx_eq(points_20, points_10 * 2.0));
}

// --- Points (identity) ---

#[test]
fn test_pt_identity() {
    // Points are the native PDF unit — should pass through unchanged
    let points = Unit::Pt.to_points(72.0);
    assert!(approx_eq(points, 72.0), "Expected 72.0, got {}", points);
}

#[test]
fn test_pt_zero() {
    let points = Unit::Pt.to_points(0.0);
    assert!(approx_eq(points, 0.0), "Expected 0.0, got {}", points);
}

#[test]
fn test_pt_negative() {
    let points = Unit::Pt.to_points(-100.0);
    assert!(approx_eq(points, -100.0), "Expected -100.0, got {}", points);
}

#[test]
fn test_pt_fractional() {
    let points = Unit::Pt.to_points(3.5);
    assert!(approx_eq(points, 3.5), "Expected 3.5, got {}", points);
}

#[test]
fn test_pt_large() {
    let points = Unit::Pt.to_points(10000.0);
    assert!(approx_eq(points, 10000.0), "Expected 10000.0, got {}", points);
}

// --- Centimeters to Points ---

#[test]
fn test_cm_one_inch_equivalent() {
    // 2.54 cm = 1 inch = 72 points
    let points = Unit::Cm.to_points(2.54);
    assert!(approx_eq(points, 72.0), "Expected 72.0, got {}", points);
}

#[test]
fn test_cm_one() {
    // 1 cm = 72/2.54 = ~28.34645669 points
    let points = Unit::Cm.to_points(1.0);
    let expected = 72.0 / 2.54;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

#[test]
fn test_cm_zero() {
    let points = Unit::Cm.to_points(0.0);
    assert!(approx_eq(points, 0.0), "Expected 0.0, got {}", points);
}

#[test]
fn test_cm_negative() {
    let points = Unit::Cm.to_points(-2.54);
    assert!(approx_eq(points, -72.0), "Expected -72.0, got {}", points);
}

#[test]
fn test_cm_a4_width() {
    // A4 width = 21cm = ~595.276 points
    let points = Unit::Cm.to_points(21.0);
    let expected = 21.0 * 72.0 / 2.54;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

#[test]
fn test_cm_a4_height() {
    // A4 height = 29.7cm = ~841.89 points
    let points = Unit::Cm.to_points(29.7);
    let expected = 29.7 * 72.0 / 2.54;
    assert!(
        approx_eq(points, expected),
        "Expected {}, got {}",
        expected,
        points
    );
}

// --- Cross-unit Consistency ---

#[test]
fn test_cm_and_mm_equivalence() {
    // 1 cm should equal 10 mm in points
    let cm_points = Unit::Cm.to_points(1.0);
    let mm_points = Unit::Mm.to_points(10.0);
    assert!(
        approx_eq(cm_points, mm_points),
        "1cm ({}) should equal 10mm ({}) in points",
        cm_points,
        mm_points
    );
}

#[test]
fn test_all_units_agree_on_one_inch() {
    // 72pt = 1in = 25.4mm = 2.54cm
    let pt = Unit::Pt.to_points(72.0);
    let inches = Unit::In.to_points(1.0);
    let mm = Unit::Mm.to_points(25.4);
    let cm = Unit::Cm.to_points(2.54);
    assert!(approx_eq(pt, inches), "Pt vs In: {} vs {}", pt, inches);
    assert!(approx_eq(pt, mm), "Pt vs Mm: {} vs {}", pt, mm);
    assert!(approx_eq(pt, cm), "Pt vs Cm: {} vs {}", pt, cm);
}

#[test]
fn test_cm_conversion_ratio() {
    let points_5 = Unit::Cm.to_points(5.0);
    let points_10 = Unit::Cm.to_points(10.0);
    assert!(approx_eq(points_10, points_5 * 2.0));
}
