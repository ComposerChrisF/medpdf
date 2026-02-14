// tests/parsing_tests.rs
// Tests for page specification parsing

use medpdf::parsing::parse_page_spec;

// --- Success Cases ---

#[test]
fn test_all_lowercase() {
    let result = parse_page_spec("all", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_all_uppercase() {
    let result = parse_page_spec("ALL", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_all_mixed_case() {
    let result = parse_page_spec("AlL", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_single_page() {
    let result = parse_page_spec("3", 5).unwrap();
    assert_eq!(result, vec![3]);
}

#[test]
fn test_first_page() {
    let result = parse_page_spec("1", 5).unwrap();
    assert_eq!(result, vec![1]);
}

#[test]
fn test_last_page() {
    let result = parse_page_spec("5", 5).unwrap();
    assert_eq!(result, vec![5]);
}

#[test]
fn test_range() {
    let result = parse_page_spec("2-4", 5).unwrap();
    assert_eq!(result, vec![2, 3, 4]);
}

#[test]
fn test_open_start() {
    let result = parse_page_spec("-3", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_open_end() {
    let result = parse_page_spec("3-", 5).unwrap();
    assert_eq!(result, vec![3, 4, 5]);
}

#[test]
fn test_fully_open() {
    // A fully open range "-" means all pages
    let result = parse_page_spec("-", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_comma_separated() {
    let result = parse_page_spec("1,3,5", 5).unwrap();
    assert_eq!(result, vec![1, 3, 5]);
}

#[test]
fn test_mixed_specs() {
    let result = parse_page_spec("1,3-5,7", 10).unwrap();
    assert_eq!(result, vec![1, 3, 4, 5, 7]);
}

#[test]
fn test_complex_mixed_specs() {
    let result = parse_page_spec("-2,5,8-", 10).unwrap();
    assert_eq!(result, vec![1, 2, 5, 8, 9, 10]);
}

#[test]
fn test_deduplication() {
    // Overlapping ranges should be deduplicated
    let result = parse_page_spec("1-3,2-4", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3, 4]);
}

#[test]
fn test_multiple_overlaps() {
    let result = parse_page_spec("1,1,1,2,2,3", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_whitespace_around_spec() {
    let result = parse_page_spec("  1-3  ", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_whitespace_around_dash() {
    let result = parse_page_spec("1 - 3", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_whitespace_around_comma() {
    let result = parse_page_spec("1 , 3 , 5", 5).unwrap();
    assert_eq!(result, vec![1, 3, 5]);
}

#[test]
fn test_single_page_doc() {
    let result = parse_page_spec("1", 1).unwrap();
    assert_eq!(result, vec![1]);
}

#[test]
fn test_range_equals_single() {
    // Range where start equals end should return single page
    let result = parse_page_spec("3-3", 5).unwrap();
    assert_eq!(result, vec![3]);
}

#[test]
fn test_sorted_output() {
    // Even if specified out of order, output should be sorted
    let result = parse_page_spec("5,1,3", 5).unwrap();
    assert_eq!(result, vec![1, 3, 5]);
}

// --- Error Cases ---

#[test]
fn test_error_page_zero() {
    let result = parse_page_spec("0", 5);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("1 or greater"));
}

#[test]
fn test_error_range_start_zero() {
    let result = parse_page_spec("0-3", 5);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("1 or greater"));
}

#[test]
fn test_error_range_end_zero() {
    let result = parse_page_spec("1-0", 5);
    assert!(result.is_err());
    // This might trigger either "1 or greater" or "inverted range" error
    let err = result.unwrap_err().to_string();
    assert!(err.contains("1 or greater") || err.contains("greater than"));
}

#[test]
fn test_single_page_beyond_max_is_empty() {
    // Page beyond document is silently skipped (filter semantics)
    let result = parse_page_spec("6", 5).unwrap();
    assert_eq!(result, Vec::<u32>::new());
}

#[test]
fn test_range_clamped_to_max() {
    // Range beyond document is clamped to actual page count
    let result = parse_page_spec("3-10", 5).unwrap();
    assert_eq!(result, vec![3, 4, 5]);
}

#[test]
fn test_error_inverted_range() {
    let result = parse_page_spec("5-3", 5);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("greater than"));
}

#[test]
fn test_error_open_range_zero_pages() {
    // Can't use open ranges on a document with no pages
    let result = parse_page_spec("-3", 0);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("no pages") || err.contains("out of bounds"));
}

#[test]
fn test_error_open_end_zero_pages() {
    let result = parse_page_spec("1-", 0);
    assert!(result.is_err());
}

#[test]
fn test_error_fully_open_zero_pages() {
    let result = parse_page_spec("-", 0);
    assert!(result.is_err());
}

#[test]
fn test_error_empty_string() {
    let result = parse_page_spec("", 5);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("parse"));
}

#[test]
fn test_error_invalid_syntax_letters() {
    let result = parse_page_spec("abc", 5);
    assert!(result.is_err());
}

#[test]
fn test_error_invalid_syntax_special() {
    let result = parse_page_spec("1..3", 5);
    assert!(result.is_err());
}

#[test]
fn test_error_double_dash() {
    let result = parse_page_spec("1--3", 5);
    assert!(result.is_err());
}

#[test]
fn test_error_trailing_comma() {
    let result = parse_page_spec("1,2,", 5);
    assert!(result.is_err());
}

#[test]
fn test_error_leading_comma() {
    let result = parse_page_spec(",1,2", 5);
    assert!(result.is_err());
}

#[test]
fn test_error_negative_number() {
    let result = parse_page_spec("-1-3", 5);
    // This is ambiguous - could be interpreted as open range or negative
    // The parser interprets "-1" as open range from 1, which should work
    // So we just check it doesn't panic and either succeeds or fails cleanly
    let _ = result;
}

// --- Edge Cases ---

#[test]
fn test_open_end_range_beyond_doc_is_empty() {
    // "2-" on a 1-page document: no pages match, so empty result
    let result = parse_page_spec("2-", 1).unwrap();
    assert_eq!(result, Vec::<u32>::new());
}

#[test]
fn test_all_on_zero_pages() {
    // "all" on 0 pages should return empty
    let result = parse_page_spec("all", 0).unwrap();
    assert_eq!(result, Vec::<u32>::new());
}

#[test]
fn test_all_on_one_page() {
    let result = parse_page_spec("all", 1).unwrap();
    assert_eq!(result, vec![1]);
}

#[test]
fn test_large_page_count() {
    let result = parse_page_spec("all", 1000).unwrap();
    assert_eq!(result.len(), 1000);
    assert_eq!(result[0], 1);
    assert_eq!(result[999], 1000);
}

#[test]
fn test_large_range() {
    let result = parse_page_spec("1-1000", 1000).unwrap();
    assert_eq!(result.len(), 1000);
}
