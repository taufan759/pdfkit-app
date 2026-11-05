//! Page selections typed by people: "1,3,5-8", "5-" (to the end), "8-5" (backwards).

use std::collections::BTreeSet;

use crate::error::{fail, Result};

/// Parses a page spec into groups, one per comma-separated part, keeping their order.
pub fn parse_ranges(spec: &str, total: u32) -> Result<Vec<Vec<u32>>> {
    let mut groups = Vec::new();
    for part in spec.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (start, end) = match part.split_once('-') {
            Some((a, b)) => (number(a.trim(), Some(1))?, number(b.trim(), Some(total))?),
            None => {
                let n = number(part, None)?;
                (n, n)
            }
        };
        if start == 0 || end == 0 || start > total || end > total {
            return fail(format!(
                "Halaman \"{part}\" di luar jangkauan (PDF ini punya {total} halaman)."
            ));
        }
        let group = if start <= end {
            (start..=end).collect()
        } else {
            (end..=start).rev().collect()
        };
        groups.push(group);
    }
    if groups.is_empty() {
        return fail("Isi nomor halamannya dulu, misalnya 1-3, 5.");
    }
    Ok(groups)
}

/// All pages of a spec, in the order written.
pub fn parse_page_list(spec: &str, total: u32) -> Result<Vec<u32>> {
    Ok(parse_ranges(spec, total)?.concat())
}

/// The distinct pages of a spec; an empty spec means every page.
pub fn parse_page_set(spec: &str, total: u32) -> Result<BTreeSet<u32>> {
    if spec.trim().is_empty() {
        return Ok((1..=total).collect());
    }
    Ok(parse_page_list(spec, total)?.into_iter().collect())
}

fn number(text: &str, default: Option<u32>) -> Result<u32> {
    match (text.is_empty(), default) {
        (true, Some(n)) => Ok(n),
        _ => text
            .parse()
            .or_else(|_| fail(format!("\"{text}\" bukan nomor halaman yang valid."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_and_ranges() {
        assert_eq!(parse_ranges("1, 3-4 ,6-", 7).unwrap(), vec![vec![1], vec![3, 4], vec![6, 7]]);
        assert_eq!(parse_page_list("3-1,5", 5).unwrap(), vec![3, 2, 1, 5]);
        assert_eq!(parse_page_set("", 3).unwrap().len(), 3);
        assert_eq!(parse_page_set("2,2,1", 3).unwrap().into_iter().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn rejects_bad_specs() {
        assert!(parse_ranges("0", 3).is_err());
        assert!(parse_ranges("4", 3).is_err());
        assert!(parse_ranges("a-b", 3).is_err());
        assert!(parse_ranges(" , ", 3).is_err());
    }
}
