// tests/error_tests.rs
// Tests for medpdf::error module

use medpdf::PdfMergeError;

// --- PdfMergeError::new() ---

#[test]
fn test_new_from_str_literal() {
    let err = PdfMergeError::new("something went wrong");
    let msg = format!("{}", err);
    assert_eq!(msg, "something went wrong");
}

#[test]
fn test_new_from_string() {
    let err = PdfMergeError::new(String::from("dynamic error"));
    let msg = format!("{}", err);
    assert_eq!(msg, "dynamic error");
}

#[test]
fn test_new_empty_message() {
    let err = PdfMergeError::new("");
    let msg = format!("{}", err);
    assert_eq!(msg, "");
}

// --- Display trait ---

#[test]
fn test_display_message_variant() {
    let err = PdfMergeError::new("test message");
    assert_eq!(format!("{err}"), "test message");
}

#[test]
fn test_display_io_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err = PdfMergeError::Io(io_err);
    let display = format!("{err}");
    assert!(display.starts_with("I/O error:"), "got: {display}");
    assert!(display.contains("file missing"), "got: {display}");
}

#[test]
fn test_display_lopdf_variant() {
    let lopdf_err = lopdf::Error::ObjectNotFound((1, 0));
    let err: PdfMergeError = lopdf_err.into();
    let display = format!("{err}");
    assert!(display.starts_with("PDF error:"), "got: {display}");
}

#[test]
fn test_display_fontkit_variant() {
    let fk_err = font_kit::error::SelectionError::NotFound;
    let err = PdfMergeError::FontKit(fk_err);
    let display = format!("{err}");
    assert!(display.starts_with("Font error:"), "got: {display}");
}

#[test]
fn test_display_face_variant() {
    // ttf_parser::FaceParsingError is created when parsing invalid data
    let face_result = ttf_parser::Face::parse(&[], 0);
    assert!(face_result.is_err());
    let face_err = face_result.unwrap_err();
    let err = PdfMergeError::Face(face_err);
    let display = format!("{err}");
    assert!(
        display.starts_with("Font parsing error:"),
        "got: {display}"
    );
}

// --- Debug trait ---

#[test]
fn test_debug_message_variant() {
    let err = PdfMergeError::new("debug test");
    let debug = format!("{err:?}");
    assert!(debug.contains("Message"), "got: {debug}");
    assert!(debug.contains("debug test"), "got: {debug}");
}

#[test]
fn test_debug_io_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let err = PdfMergeError::Io(io_err);
    let debug = format!("{err:?}");
    assert!(debug.contains("Io"), "got: {debug}");
}

// --- From trait conversions ---

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
    let err: PdfMergeError = io_err.into();
    assert!(matches!(err, PdfMergeError::Io(_)));
}

#[test]
fn test_from_lopdf_error() {
    let lopdf_err = lopdf::Error::ObjectNotFound((1, 0));
    let err: PdfMergeError = lopdf_err.into();
    assert!(matches!(err, PdfMergeError::LoPdf(_)));
}

#[test]
fn test_from_str() {
    let err: PdfMergeError = "string slice error".into();
    assert!(matches!(err, PdfMergeError::Message(_)));
    assert_eq!(format!("{err}"), "string slice error");
}

#[test]
fn test_from_string() {
    let err: PdfMergeError = String::from("owned string error").into();
    assert!(matches!(err, PdfMergeError::Message(_)));
    assert_eq!(format!("{err}"), "owned string error");
}

#[test]
fn test_from_face_parsing_error() {
    let face_err = ttf_parser::Face::parse(&[], 0).unwrap_err();
    let err: PdfMergeError = face_err.into();
    assert!(matches!(err, PdfMergeError::Face(_)));
}

#[test]
fn test_from_selection_error() {
    let sel_err = font_kit::error::SelectionError::NotFound;
    let err: PdfMergeError = sel_err.into();
    assert!(matches!(err, PdfMergeError::FontKit(_)));
}

// --- std::error::Error::source() ---

#[test]
fn test_source_message_is_none() {
    use std::error::Error;
    let err = PdfMergeError::new("no source");
    assert!(err.source().is_none());
}

#[test]
fn test_source_io_is_some() {
    use std::error::Error;
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    let err = PdfMergeError::Io(io_err);
    let source = err.source();
    assert!(source.is_some());
    assert!(source.unwrap().to_string().contains("missing"));
}

#[test]
fn test_source_lopdf_is_some() {
    use std::error::Error;
    let lopdf_err = lopdf::Error::ObjectNotFound((1, 0));
    let err: PdfMergeError = lopdf_err.into();
    assert!(err.source().is_some());
}

#[test]
fn test_source_fontkit_is_some() {
    use std::error::Error;
    let fk_err = font_kit::error::SelectionError::NotFound;
    let err: PdfMergeError = fk_err.into();
    assert!(err.source().is_some());
}

#[test]
fn test_source_face_is_some() {
    use std::error::Error;
    let face_err = ttf_parser::Face::parse(&[], 0).unwrap_err();
    let err: PdfMergeError = face_err.into();
    assert!(err.source().is_some());
}

// --- Result type alias ---

#[test]
fn test_result_type_alias_ok() {
    let result: medpdf::Result<i32> = Ok(42);
    assert_eq!(result.unwrap(), 42);
}

#[test]
fn test_result_type_alias_err() {
    let result: medpdf::Result<i32> = Err(PdfMergeError::new("fail"));
    assert!(result.is_err());
}

// --- Error can be used with ? operator ---

fn function_returning_result() -> medpdf::Result<()> {
    let _: () = Err(PdfMergeError::new("propagated"))?;
    Ok(())
}

#[test]
fn test_question_mark_propagation() {
    let result = function_returning_result();
    assert!(result.is_err());
    assert_eq!(format!("{}", result.unwrap_err()), "propagated");
}
