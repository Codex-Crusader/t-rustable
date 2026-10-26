//! Page ranges such as `1-3,5,8-`. Page numbers start at 1.

use std::path::{Path, PathBuf};

use crate::report::Problem;

/// One part of a range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    One(usize),
    /// From the first number to the second, or to the last page.
    Span(usize, Option<usize>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    parts: Vec<Part>,
    text: String,
}

fn bad(text: &str, why: &str) -> Problem {
    Problem::new(
        "invalid-argument",
        format!("The page range {text:?} is not valid: {why}"),
    )
    .hint("Use page numbers and spans, for example 1-3,5,8- (8- means page 8 to the end).")
}

impl std::str::FromStr for Range {
    type Err = Problem;

    fn from_str(text: &str) -> Result<Self, Problem> {
        let number = |s: &str| -> Result<usize, Problem> {
            match s.trim().parse::<usize>() {
                Ok(n) if n >= 1 => Ok(n),
                _ => Err(bad(text, &format!("{s:?} is not a page number."))),
            }
        };
        let mut parts = Vec::new();
        for item in text.split(',') {
            let part = match item.split_once('-') {
                None => Part::One(number(item)?),
                Some((a, b)) if b.trim().is_empty() => Part::Span(number(a)?, None),
                Some((a, b)) => {
                    let (a, b) = (number(a)?, number(b)?);
                    if b < a {
                        return Err(bad(text, &format!("{item:?} goes backward.")));
                    }
                    Part::Span(a, Some(b))
                }
            };
            parts.push(part);
        }
        Ok(Self {
            parts,
            text: text.into(),
        })
    }
}

impl Range {
    /// Page indices from 0, in the order of the range, for a document of `count` pages.
    pub fn indices(&self, count: usize) -> Result<Vec<usize>, Problem> {
        let mut out = Vec::new();
        for part in &self.parts {
            let (a, b) = match *part {
                Part::One(n) => (n, n),
                Part::Span(a, b) => (a, b.unwrap_or(count)),
            };
            if b > count || a > count {
                return Err(Problem::new(
                    "not-found",
                    format!(
                        "The page range {:?} asks for page {}, but the document has {count} pages.",
                        self.text,
                        a.max(b)
                    ),
                ));
            }
            out.extend(a - 1..b);
        }
        Ok(out)
    }
}

/// A merge input: `file.pdf` or `file.pdf:1-3`. A file that exists with the full name wins,
/// so names with a colon, such as `C:\a.pdf`, work.
pub fn merge_input(text: &str) -> Result<(PathBuf, Option<Range>), Problem> {
    if Path::new(text).exists() {
        return Ok((text.into(), None));
    }
    match text.rsplit_once(':') {
        Some((file, range))
            if !file.is_empty()
                && !range.is_empty()
                && range
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == ',' || c == '-' || c == ' ') =>
        {
            Ok((file.into(), Some(range.parse()?)))
        }
        _ => Ok((text.into(), None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Range {
        s.parse().unwrap()
    }

    #[test]
    fn ranges_give_indices_in_order() {
        assert_eq!(r("1-3,5,8-").indices(9).unwrap(), [0, 1, 2, 4, 7, 8]);
        assert_eq!(r("3,1").indices(3).unwrap(), [2, 0]);
        assert_eq!(r("2-2").indices(2).unwrap(), [1]);
        assert_eq!(r("2-").indices(2).unwrap(), [1]);
        assert_eq!(r(" 1 - 2 ").indices(2).unwrap(), [0, 1]);
    }

    #[test]
    fn bad_ranges_are_refused() {
        for bad in ["", "0", "a", "3-1", "1,,2", "-2", "1-2-3"] {
            assert!(bad.parse::<Range>().is_err(), "{bad:?}");
        }
        assert_eq!(r("4").indices(3).unwrap_err().code, "not-found");
        assert_eq!(r("2-9").indices(3).unwrap_err().code, "not-found");
        assert_eq!(r("5-").indices(3).unwrap_err().code, "not-found");
    }

    #[test]
    fn merge_inputs_split_at_the_last_colon() {
        assert_eq!(merge_input("a.pdf").unwrap(), ("a.pdf".into(), None));
        assert_eq!(
            merge_input("dir/a.pdf:1-2,4").unwrap(),
            ("dir/a.pdf".into(), Some(r("1-2,4")))
        );
        assert_eq!(
            merge_input(r"C:\x\a.pdf").unwrap(),
            (r"C:\x\a.pdf".into(), None)
        );
        assert_eq!(
            merge_input(r"C:\x\a.pdf:3").unwrap(),
            (r"C:\x\a.pdf".into(), Some(r("3")))
        );
        assert!(merge_input("a.pdf:0").is_err());
    }
}
