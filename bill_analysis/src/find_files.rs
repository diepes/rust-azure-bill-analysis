use regex::Regex; // Add this line to import the `Regex` struct from the `regex` crate
use std::fs;

pub fn in_folder(folder: &str, file_re_pattern: &str) -> Vec<String> {
    let re = Regex::new(file_re_pattern).unwrap(); // Use `Regex` directly without the `regex::` prefix
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if re.is_match(file_name) {
                files.push(file_name.to_string());
            }
        }
    }
    // sort the files before returning
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_files() {
        let files = in_folder("tests", r"azure_test_.*_01.csv");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "azure_test_data_01.csv");
        assert_eq!(files[1], "azure_test_disks_01.csv");
    }
}
