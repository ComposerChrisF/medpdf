// src/parsing.rs

use nom::{
    branch::alt,
    character::complete::{char, digit1, multispace0},
    combinator::{all_consuming, map, map_res, opt},
    multi::separated_list1,
    sequence::{delimited, separated_pair},
    IResult,
};
use std::collections::BTreeSet;

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

/// Parses a page specification string into a sorted vector of 1-based page numbers.
pub fn parse_page_spec(spec: &str, max_pages: u32) -> Result<Vec<u32>, String> {
    let mut pages = BTreeSet::new();
    let trimmed_spec = spec.trim();

    if trimmed_spec.eq_ignore_ascii_case("all") {
        (1..=max_pages).for_each(|i| {
            pages.insert(i);
        });
        return Ok(pages.into_iter().collect());
    }

    let parse_result = all_consuming(parse_spec_list)(trimmed_spec);

    match parse_result {
        Ok((_, items)) => {
            for item in items {
                match item {
                    PageItem::Single(num) => {
                        if num == 0 {
                            return Err("Page numbers must be 1 or greater.".to_string());
                        }
                        if max_pages > 0 && num > max_pages {
                            return Err(format!(
                                "Invalid page: page {} is out of bounds (max pages: {}).",
                                num, max_pages
                            ));
                        }
                        pages.insert(num);
                    }
                    PageItem::Range(start_opt, end_opt) => {
                        if max_pages == 0 && (start_opt.is_none() || end_opt.is_none()) {
                            return Err(
                                "Cannot use open ranges on a document with no pages.".to_string()
                            );
                        }
                        let start = start_opt.unwrap_or(1);
                        let end = end_opt.unwrap_or(max_pages);
                        if start == 0 || end == 0 {
                            return Err("Page numbers must be 1 or greater.".to_string());
                        }
                        if max_pages > 0 && start > max_pages {
                            return Err(format!(
                                "Invalid range: start page {} is out of bounds (max pages: {}).",
                                start, max_pages
                            ));
                        }
                        if max_pages > 0 && end > max_pages {
                            return Err(format!(
                                "Invalid range: end page {} is out of bounds (max pages: {}).",
                                end, max_pages
                            ));
                        }
                        if start > end {
                            return Err(format!(
                                "Invalid range: start ({}) is greater than end ({}).",
                                start, end
                            ));
                        }
                        for i in start..=end {
                            pages.insert(i);
                        }
                    }
                }
            }
        }
        Err(e) => {
            return Err(format!(
                "Failed to parse page specification '{}': {}",
                spec, e
            ))
        }
    }
    Ok(pages.into_iter().collect())
}
