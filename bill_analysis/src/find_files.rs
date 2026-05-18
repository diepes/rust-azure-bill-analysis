use regex::Regex; // Add this line to import the `Regex` struct from the `regex` crate
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the previous calendar month as a `YYYY-MM` shorthand string,
/// e.g. if today is 2026-05-15, returns `"2026-04"`.
pub fn last_month_shorthand() -> String {
    use chrono::{Datelike, Local};
    let today = Local::now();
    let (year, month) = if today.month() == 1 {
        (today.year() - 1, 12u32)
    } else {
        (today.year(), today.month() - 1)
    };
    format!("{:04}-{:02}", year, month)
}

/// If `path` starts with a date shorthand (`YYYY-MM`, `YYYYMM`, or bare `MM`), resolve it
/// to a real file or directory under `csv_data/`. Resolution order:
///
/// - `MM` (2 digits): tries `{current_year}-{MM}`, then `{prev_year}-{MM}`, each using the
///   same sub-dir / CSV search as the full-date variants below.
/// - `YYYY-MM` / `YYYYMM`: tries subdirectory then CSV file for each format variant.
///
/// If no match is found, the original path is returned unchanged.
pub fn resolve_date_shorthand(path: &Path) -> PathBuf {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => return path.to_path_buf(),
    };

    // Bare MM (exactly 2 digits, e.g. "04" or "10")
    let re_mm = Regex::new(r"^\d{2}$").unwrap();
    if re_mm.is_match(path_str) {
        use chrono::{Datelike, Local};
        let today = Local::now();
        let cur_year = today.year();
        for year in &[cur_year, cur_year - 1] {
            let candidate = format!("{:04}-{}", year, path_str);
            if let Some(p) = resolve_yyyymm_candidate(&candidate) {
                return p;
            }
        }
        return path.to_path_buf();
    }

    // Match exactly YYYY-MM or YYYYMM at the start of the value
    let re = Regex::new(r"^\d{4}(-\d{2}|\d{2})(?:[^0-9]|$)").unwrap();
    if !re.is_match(path_str) {
        return path.to_path_buf();
    }
    // Extract the leading date token (6 or 7 chars)
    let token = if path_str.len() >= 7 && &path_str[4..5] == "-" {
        &path_str[..7] // YYYY-MM
    } else {
        &path_str[..6] // YYYYMM
    };
    if let Some(p) = resolve_yyyymm_candidate(token) {
        return p;
    }

    // Nothing found — return as-is so the caller surfaces a natural error
    path.to_path_buf()
}

/// Try to find a csv_data entry for a `YYYY-MM` or `YYYYMM` token.
/// Tries both dash and compact forms; subdirectory first, then CSV file.
fn resolve_yyyymm_candidate(token: &str) -> Option<PathBuf> {
    let (dash, compact) = if token.contains('-') {
        (token.to_string(), token.replace('-', ""))
    } else {
        (
            format!("{}-{}", &token[..4], &token[4..]),
            token.to_string(),
        )
    };

    let base = Path::new("csv_data");

    for date in &[&dash, &compact] {
        if let Some(p) = find_entry_with_prefix(base, date, true) {
            return Some(p);
        }
        if let Some(p) = find_entry_with_prefix(base, date, false) {
            return Some(p);
        }
    }
    None
}

/// Scan `base` for an entry whose name starts with `prefix`.
/// If `dir_only` is true, match directories; otherwise match `.csv` files.
/// Returns the last match (alphabetically), or `None`.
fn find_entry_with_prefix(base: &Path, prefix: &str, dir_only: bool) -> Option<PathBuf> {
    let mut matches: Vec<PathBuf> = fs::read_dir(base)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let path = e.path();
            let matches = if dir_only {
                path.is_dir() && name.starts_with(prefix)
            } else {
                path.is_file() && name.starts_with(prefix) && name.ends_with(".csv")
            };
            if matches { Some(path) } else { None }
        })
        .collect();
    matches.sort();
    matches.into_iter().last()
}

/// Scan `data_dir` for subdirectories (or CSV files) whose names start with a `YYYY-MM`
/// prefix and return a sorted, deduplicated list of `"YYYY-MM"` strings.
pub fn list_bill_months(data_dir: &Path) -> Vec<String> {
    let re = Regex::new(r"^(\d{4}-\d{2})").unwrap();
    let mut months: Vec<String> = fs::read_dir(data_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let caps = re.captures(&name)?;
            Some(caps[1].to_string())
        })
        .collect();
    months.sort();
    months.dedup();
    months
}

/// Find the first `Detail*.csv` file inside `data_dir` for the given `year_month`
/// (`"YYYY-MM"` format). Looks for a subdirectory whose name starts with `year_month`,
/// then returns the last alphabetically-sorted `Detail*.csv` inside it.
/// Falls back to a CSV at the top level of `data_dir` whose name starts with `year_month`.
pub fn find_bill_csv(data_dir: &Path, year_month: &str) -> Option<PathBuf> {
    // First: find a subdirectory whose name starts with the year_month prefix
    let subdir = fs::read_dir(data_dir)
        .ok()?
        .flatten()
        .find(|e| {
            let name = e.file_name().to_str().unwrap_or("").to_string();
            e.path().is_dir() && name.starts_with(year_month)
        })
        .map(|e| e.path());

    if let Some(dir) = subdir {
        // Find the last Detail*.csv inside the subdirectory
        let mut csvs: Vec<PathBuf> = fs::read_dir(&dir)
            .ok()?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_string();
                let path = e.path();
                if path.is_file() && name.contains("Detail") && name.ends_with(".csv") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();
        csvs.sort();
        return csvs.into_iter().last();
    }

    // Fallback: CSV directly in data_dir
    let mut csvs: Vec<PathBuf> = fs::read_dir(data_dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let path = e.path();
            if path.is_file()
                && name.starts_with(year_month)
                && name.contains("Detail")
                && name.ends_with(".csv")
            {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    csvs.sort();
    csvs.into_iter().last()
}

/// split path and search folder for files matching the path.file_name() or if not present with file_re_pattern
pub fn in_folder(path: &Path, file_re_pattern: &str, debug: bool) -> (PathBuf, Vec<String>) {
    let mut files = Vec::new();
    // extract the folder or set to ./(current folder)
    let folder;
    let file_search;
    if path.is_dir() {
        folder = path.to_path_buf();
        file_search = file_re_pattern
    } else {
        folder = path
            .parent()
            .unwrap_or_else(|| panic!("Failed to split directory '{}'", path.display()))
            .to_path_buf();
        file_search = path
            .file_name()
            .unwrap_or_else(|| panic!("Failed to split file '{}'", path.display()))
            .to_str()
            .unwrap();
        // = file_name;
    };
    let re = Regex::new(file_search).unwrap(); // Use `Regex` directly without the `regex::` prefix
    for entry in fs::read_dir(&folder)
        .unwrap_or_else(|_| panic!("Failed to read directory '{}'", path.display()))
    {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if re.is_match(file_name) {
                files.push(file_name.to_string());
            }
            if debug {
                println!("Debug path: {:?} {}", folder, file_name);
            };
        }
    }
    // sort the files before returning
    files.sort();
    (folder, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_files_dir() {
        let (path, files) = in_folder(&PathBuf::from("tests"), r"azure_test_.*_01.csv", false);
        assert_eq!(path.to_str().unwrap(), "tests");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "azure_test_data_01.csv");
        assert_eq!(files[1], "azure_test_disks_01.csv");
    }
    #[test]
    fn test_find_specific_files() {
        let (path, files) = in_folder(
            &PathBuf::from("./tests/azure_test_disks_02.txt"),
            r"azure_test_.*_01.csv",
            false,
        );
        assert_eq!(path.to_str().unwrap(), "./tests");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "azure_test_disks_02.txt");
    }
    #[test]
    fn test_find_file_match_part() {
        let (path, files) = in_folder(
            &PathBuf::from("./tests/disks_02"),
            r"azure_test_.*_01.csv",
            false,
        );
        assert_eq!(path.to_str().unwrap(), "./tests");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "azure_test_disks_02.txt");
    }

    // --- resolve_date_shorthand tests ---

    #[test]
    fn test_resolve_date_shorthand_non_date_passthrough() {
        // Full paths are returned unchanged
        let p = PathBuf::from("./csv_data/some_file.csv");
        assert_eq!(resolve_date_shorthand(&p), p);

        let p2 = PathBuf::from("csv_data");
        assert_eq!(resolve_date_shorthand(&p2), p2);
    }

    #[test]
    fn test_resolve_date_shorthand_non_date_text() {
        // Strings that are not dates are returned unchanged
        let p = PathBuf::from("202");
        assert_eq!(resolve_date_shorthand(&p), p);

        let p2 = PathBuf::from("abcdef");
        assert_eq!(resolve_date_shorthand(&p2), p2);
    }

    #[test]
    fn test_resolve_date_shorthand_detects_yyyy_mm() {
        // A YYYY-MM value that doesn't exist in csv_data/ returns original path
        let p = PathBuf::from("2025-10");
        // csv_data/ is empty in test environment, so nothing is found
        assert_eq!(resolve_date_shorthand(&p), p);
    }

    #[test]
    fn test_resolve_date_shorthand_detects_yyyymm() {
        // A YYYYMM value that doesn't exist in csv_data/ returns original path
        let p = PathBuf::from("202510");
        assert_eq!(resolve_date_shorthand(&p), p);
    }

    #[test]
    fn test_resolve_date_shorthand_mm_too_short_passthrough() {
        // 1 digit — not MM, returned unchanged
        let p = PathBuf::from("4");
        assert_eq!(resolve_date_shorthand(&p), p);
    }

    #[test]
    fn test_resolve_date_shorthand_mm_no_match_returns_original() {
        // "99" is a valid MM shape but no csv_data/ entry will match
        let p = PathBuf::from("99");
        assert_eq!(resolve_date_shorthand(&p), p);
    }
}
