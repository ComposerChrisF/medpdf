//! Page specification parsing (e.g. `"1-3,5,7-"`, `"all"`).

use nom::{
    IResult,
    branch::alt,
    character::complete::{char, digit1, multispace0},
    combinator::{all_consuming, map, map_res, opt},
    multi::separated_list1,
    sequence::{delimited, separated_pair},
};
use std::collections::HashSet;

use crate::error::{MedpdfError, Result};

#[derive(Debug, Clone, Copy)]
enum PageItem {
    Single(u32),
    Range(Option<u32>, Option<u32>),
}

fn parse_number(input: &str) -> IResult<&str, u32> {
    map_res(digit1, |s: &str| s.parse::<u32>())(input)
}
fn parse_range(input: &str) -> IResult<&str, PageItem> {
    map(
        separated_pair(
            opt(parse_number),
            delimited(multispace0, char('-'), multispace0),
            opt(parse_number),
        ),
        |(start, end)| PageItem::Range(start, end),
    )(input)
}
fn parse_single(input: &str) -> IResult<&str, PageItem> {
    map(parse_number, PageItem::Single)(input)
}
fn parse_item(input: &str) -> IResult<&str, PageItem> {
    alt((parse_range, parse_single))(input)
}
fn parse_spec_list(input: &str) -> IResult<&str, Vec<PageItem>> {
    separated_list1(delimited(multispace0, char(','), multispace0), parse_item)(input)
}

/// Parses a page specification string into a vector of 1-based page numbers,
/// preserving user-specified order. Duplicates are dropped (first occurrence wins).
///
/// **Out-of-range pages are an error, not silently dropped (bug-0021).** A single page,
/// or a range's start, greater than `max_pages` returns `Err`; an *explicit* range end
/// beyond `max_pages` (e.g. `"1-100"` on a 3-page document) also returns `Err`. An *open*
/// end (`"3-"`) means "through the last page", so it is defined by `max_pages` and is
/// never out of range. `"all"` yields every page. This is a fail-loud contract: callers
/// do not need to re-validate the returned set against the request.
pub fn parse_page_spec(spec: &str, max_pages: u32) -> Result<Vec<u32>> {
    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    let trimmed_spec = spec.trim();

    if trimmed_spec.eq_ignore_ascii_case("all") {
        return Ok((1..=max_pages).collect());
    }

    let parse_result = all_consuming(parse_spec_list)(trimmed_spec);

    match parse_result {
        Ok((_, items)) => {
            for item in items {
                match item {
                    PageItem::Single(num) => {
                        if num == 0 {
                            return Err(MedpdfError::new("Page numbers must be 1 or greater."));
                        }
                        // Out-of-range is an error, not a silent drop (bug-0021).
                        if num > max_pages {
                            return Err(MedpdfError::new(format!(
                                "Page {num} is out of range: the document has {max_pages} page(s)."
                            )));
                        }
                        if seen.insert(num) {
                            pages.push(num);
                        }
                    }
                    PageItem::Range(start_opt, end_opt) => {
                        if max_pages == 0 && (start_opt.is_none() || end_opt.is_none()) {
                            return Err(MedpdfError::new(
                                "Cannot use open ranges on a document with no pages.",
                            ));
                        }
                        let start = start_opt.unwrap_or(1);
                        let end = end_opt.unwrap_or(max_pages);
                        if start == 0 || end == 0 {
                            return Err(MedpdfError::new("Page numbers must be 1 or greater."));
                        }
                        // Only error on inverted range when both bounds are explicit
                        if start_opt.is_some() && end_opt.is_some() && start > end {
                            return Err(MedpdfError::new(format!(
                                "Invalid range: start ({}) is greater than end ({}).",
                                start, end
                            )));
                        }
                        // Out-of-range is an error, not a silent clamp/skip (bug-0021). The
                        // start must exist; an EXPLICIT end beyond the document is an error.
                        // An OPEN end (`N-`) means "through the last page", so it is defined
                        // by max_pages and is never out of range.
                        if start > max_pages {
                            return Err(MedpdfError::new(format!(
                                "Page {start} is out of range: the document has {max_pages} page(s)."
                            )));
                        }
                        if let Some(explicit_end) = end_opt
                            && explicit_end > max_pages
                        {
                            return Err(MedpdfError::new(format!(
                                "Page {explicit_end} is out of range: the document has {max_pages} page(s)."
                            )));
                        }
                        for i in start..=end {
                            if seen.insert(i) {
                                pages.push(i);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            return Err(MedpdfError::new(format!(
                "Failed to parse page specification '{}': {}",
                spec, e
            )));
        }
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_page() {
        assert_eq!(parse_page_spec("3", 5).unwrap(), vec![3]);
    }

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_page_spec("2-4", 5).unwrap(), vec![2, 3, 4]);
    }

    #[test]
    fn test_parse_open_start_range() {
        assert_eq!(parse_page_spec("-3", 5).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_open_end_range() {
        assert_eq!(parse_page_spec("3-", 5).unwrap(), vec![3, 4, 5]);
    }

    #[test]
    fn test_parse_all() {
        assert_eq!(parse_page_spec("all", 3).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_all_case_insensitive() {
        assert_eq!(parse_page_spec("ALL", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_page_spec("All", 3).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_comma_separated() {
        assert_eq!(parse_page_spec("1,3,5", 5).unwrap(), vec![1, 3, 5]);
    }

    #[test]
    fn test_parse_mixed() {
        assert_eq!(parse_page_spec("1-3,5", 5).unwrap(), vec![1, 2, 3, 5]);
    }

    #[test]
    fn test_parse_out_of_range_single_error() {
        // bug-0021: a single page beyond the document is a loud error, not a silent drop.
        assert!(parse_page_spec("99", 3).is_err());
    }

    #[test]
    fn test_parse_out_of_range_explicit_range_error() {
        // bug-0021: an explicit end beyond the document is an error, not a silent clamp.
        assert!(parse_page_spec("1-100", 3).is_err());
    }

    #[test]
    fn test_parse_out_of_range_start_error() {
        // bug-0021: an open range whose start is beyond the document errors (not empty).
        assert!(parse_page_spec("5-", 3).is_err());
    }

    #[test]
    fn test_parse_open_end_is_not_out_of_range() {
        // An open end means "through the last page" — defined by max_pages, never an error.
        assert_eq!(parse_page_spec("2-", 4).unwrap(), vec![2, 3, 4]);
    }

    #[test]
    fn test_parse_zero_page_error() {
        assert!(parse_page_spec("0", 5).is_err());
    }

    #[test]
    fn test_parse_inverted_range_error() {
        assert!(parse_page_spec("5-3", 5).is_err());
    }

    #[test]
    fn test_parse_whitespace() {
        assert_eq!(parse_page_spec(" 1 - 3 , 5 ", 5).unwrap(), vec![1, 2, 3, 5]);
    }

    #[test]
    fn test_parse_dedup() {
        assert_eq!(parse_page_spec("1,1,2", 5).unwrap(), vec![1, 2]);
    }

    #[test]
    fn test_parse_empty_string_error() {
        assert!(parse_page_spec("", 5).is_err());
    }

    #[test]
    fn test_parse_open_range_zero_pages_error() {
        assert!(parse_page_spec("1-", 0).is_err());
    }

    #[test]
    fn test_parse_preserves_user_order() {
        assert_eq!(parse_page_spec("5,3,1", 5).unwrap(), vec![5, 3, 1]);
    }

    #[test]
    fn test_parse_order_with_range_dedup() {
        // 3 first, then range 1-4 adds 1,2,4 (3 already seen)
        assert_eq!(parse_page_spec("3,1-4", 5).unwrap(), vec![3, 1, 2, 4]);
    }
}
