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
fn test_overlapping_ranges_keep_their_repeats() {
    // Overlapping ranges emit each page as written — a spec is a sequence, not a
    // set (plan-0006). Page 2 and 3 fall in both ranges, so each appears twice.
    let result = parse_page_spec("1-3,2-4", 5).unwrap();
    assert_eq!(result, vec![1, 2, 3, 2, 3, 4]);
}

#[test]
fn test_multiple_overlaps() {
    let result = parse_page_spec("1,1,1,2,2,3", 5).unwrap();
    assert_eq!(result, vec![1, 1, 1, 2, 2, 3]);
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
fn test_preserves_user_order() {
    // Output preserves user-specified order; duplicates dropped (first wins)
    let result = parse_page_spec("5,1,3", 5).unwrap();
    assert_eq!(result, vec![5, 1, 3]);
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
fn test_single_page_beyond_max_errors() {
    // bug-0021: a page beyond the document is a loud error, not a silent empty result.
    let result = parse_page_spec("6", 5);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("out of range"));
}

#[test]
fn test_range_beyond_max_errors() {
    // bug-0021: an explicit range end beyond the document is an error, not a silent clamp.
    let result = parse_page_spec("3-10", 5);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("out of range"));
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
    // "-1-3" is ambiguous: the parser consumes "-1" as an open-start range (pages 1..=1)
    // but then fails on the remaining "-3" which can't be parsed as a valid separator+spec.
    let result = parse_page_spec("-1-3", 5);
    assert!(
        result.is_err(),
        "Ambiguous spec '-1-3' should fail to parse"
    );
}

// --- Edge Cases ---

#[test]
fn test_open_end_range_start_beyond_doc_errors() {
    // bug-0021: "2-" on a 1-page document has a start beyond the document → error, not
    // an empty result (the start must exist even for an open-ended range).
    let result = parse_page_spec("2-", 1);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("out of range"));
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

// ---------------------------------------------------------------------------
// plan-0006: a page spec is a sequence to emit, not a set to select.
// Repetition is legal; out-of-range is not. The two are orthogonal, and the
// second half is a contract invariant pdf-maker records on its side and asked
// for explicitly — removing the dedup must not weaken the bug-0021 bounds check.
// ---------------------------------------------------------------------------

#[test]
fn test_repeated_single_page_is_preserved() {
    assert_eq!(parse_page_spec("1,1", 5).unwrap(), vec![1, 1]);
}

#[test]
fn test_repeat_after_range_is_preserved_in_position() {
    assert_eq!(parse_page_spec("1-3,2", 5).unwrap(), vec![1, 2, 3, 2]);
}

#[test]
fn test_page_repeated_many_times() {
    assert_eq!(parse_page_spec("2,2,2,2", 5).unwrap(), vec![2, 2, 2, 2]);
}

/// The invariant pdf-maker depends on: repetition legal, out-of-range still an
/// error naming the offending page, even when a repeat precedes it.
#[test]
fn test_repeats_do_not_weaken_the_out_of_range_check() {
    let err = parse_page_spec("1,1,99", 2).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("99"),
        "the error must name the out-of-range page; got: {msg}"
    );
}

/// The legal half of the same spec on the same document still parses.
#[test]
fn test_repeats_within_range_still_succeed() {
    assert_eq!(parse_page_spec("1,1", 2).unwrap(), vec![1, 1]);
}

/// `"all"` is a whole-document selection, not a user sequence, so it is
/// unaffected: every page once, in order.
#[test]
fn test_all_is_unaffected_by_the_sequence_change() {
    assert_eq!(parse_page_spec("all", 4).unwrap(), vec![1, 2, 3, 4]);
}
