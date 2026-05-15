use regex::Regex; // Add this line to import the `Regex` struct from the `regex` crate
use std::fs;
use std::path::{Path, PathBuf};

use crate::cmd_parse::GlobalOpts;

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

/// If `path` starts with a date shorthand (`YYYY-MM` or `YYYYMM`), resolve it to a
/// real file or directory under `csv_data/`. Resolution order for each format variant:
///   1. `csv_data/{date}*/`  — a subdirectory whose name starts with the date
///   2. `csv_data/{date}*.csv` — a CSV file whose name starts with the date
/// Both `YYYY-MM` and `YYYYMM` variants are tried (YYYY-MM first).
/// If no match is found, the original path is returned unchanged.
pub fn resolve_date_shorthand(path: &Path) -> PathBuf {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => return path.to_path_buf(),
    };

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

    // Derive both format variants
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
        // 1. Subdirectory: csv_data/{date}*/
        if let Some(p) = find_entry_with_prefix(base, date, true) {
            return p;
        }
        // 2. CSV file: csv_data/{date}*.csv
        if let Some(p) = find_entry_with_prefix(base, date, false) {
            return p;
        }
    }

    // Nothing found — return as-is so the caller surfaces a natural error
    path.to_path_buf()
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

/// split path and search folder for files matching the path.file_name() or if not present with file_re_pattern
pub fn in_folder(
    path: &Path,
    file_re_pattern: &str,
    global_opts: &GlobalOpts,
) -> (PathBuf, Vec<String>) {
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
            .expect(&format!("Failed to split directory '{}'", path.display()))
            .to_path_buf();
        file_search = path
            .file_name()
            .expect(&format!("Failed to split file '{}'", path.display()))
            .to_str()
            .unwrap();
        // = file_name;
    };
    let re = Regex::new(file_search).unwrap(); // Use `Regex` directly without the `regex::` prefix
    for entry in
        fs::read_dir(&folder).expect(&format!("Failed to read directory '{}'", path.display()))
    {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if re.is_match(file_name) {
                files.push(file_name.to_string());
            }
            if global_opts.debug {
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
    static GLOBAL_OPTS: GlobalOpts = GlobalOpts {
        debug: false,
        bill_path: None,
        bill_prev_subtract_path: None,
        cost_min_display: 0.0,
        case_sensitive: true,
        tag_list: false,
    };

    #[test]
    fn test_find_files_dir() {
        let (path, files) = in_folder(
            &PathBuf::from("tests"),
            r"azure_test_.*_01.csv",
            &GLOBAL_OPTS,
        );
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
            &GLOBAL_OPTS,
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
            &GLOBAL_OPTS,
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
}
