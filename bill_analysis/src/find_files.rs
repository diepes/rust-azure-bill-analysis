use regex::Regex; // Add this line to import the `Regex` struct from the `regex` crate
use std::fs;
use std::path::PathBuf;

/// split path and search folder for files matching the path.file_name() or if not present with file_re_pattern
pub fn in_folder(path: &PathBuf, file_re_pattern: &str) -> (PathBuf,Vec<String>) {
    let mut files = Vec::new();
    // extract the folder or set to ./(current folder)
    let folder;
    let file_search;
    if path.is_dir() {
        folder = path.to_path_buf();
        file_search = file_re_pattern
    } else {
        folder = path.parent().expect(&format!("Failed to split directory '{}'", path.display())).to_path_buf();
        file_search = path.file_name().expect(&format!("Failed to split file '{}'", path.display())).to_str().unwrap();
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
        // println!("Debug path: {:?} {}", folder, file_name);
        }
    }
    // sort the files before returning
    files.sort();
    (folder,files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_files_dir() {
        let (path,files) = in_folder(&PathBuf::from("tests"), r"azure_test_.*_01.csv");
        assert_eq!(path.to_str().unwrap(), "tests");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "azure_test_data_01.csv");
        assert_eq!(files[1], "azure_test_disks_01.csv");
    }
    #[test]
    fn test_find_specific_files() {
        let (path,files) = in_folder(&PathBuf::from("./tests/azure_test_disks_02.txt"), r"azure_test_.*_01.csv");
        assert_eq!(path.to_str().unwrap(), "./tests");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "azure_test_disks_02.txt");
    }
    #[test]
    fn test_find_file_match_part() {
        let (path,files) = in_folder(&PathBuf::from("./tests/disks_02"), r"azure_test_.*_01.csv");
        assert_eq!(path.to_str().unwrap(), "./tests");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "azure_test_disks_02.txt");
    }
}
