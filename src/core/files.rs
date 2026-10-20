//! Simple queries over a list of scanned files: biggest, by type, by name,
//! by folder.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use crate::core::models::FileInfo;

/// Extensions treated as documents for the similarity comparison.
const DOCUMENT_EXTENSIONS: &[&str] = &["pdf", "txt", "md", "rs", "json", "csv", "py", "js", "c", "cpp"];

/// True if the file is a document whose text can be compared (PDF, text, code).
pub fn is_document(file: &FileInfo) -> bool {
    let ext = file.extension.as_deref().unwrap_or("").to_lowercase();
    DOCUMENT_EXTENSIONS.contains(&ext.as_str())
}

/// Returns the `n` heaviest files in the list, sorted from largest to smallest.
pub fn get_top_largest(files: &[FileInfo], n: usize) -> Vec<FileInfo> {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    sorted.into_iter().take(n).collect()
}

/// Filters files that match a specific extension (e.g. "pdf", "png").
pub fn filter_by_extension(files: &[FileInfo], ext: &str) -> Vec<FileInfo> {
    let clean_ext = ext.trim().trim_start_matches('.').to_lowercase();
    files
        .iter()
        .filter(|f| {
            f.extension
                .as_ref()
                .map(|e| e == &clean_ext)
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Groups a list of files by their containing folder.
pub fn group_by_folder(files: &[FileInfo]) -> BTreeMap<PathBuf, Vec<FileInfo>> {
    let mut grouped: BTreeMap<PathBuf, Vec<FileInfo>> = BTreeMap::new();

    for file in files {
        let parent = file
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        grouped.entry(parent).or_default().push(file.clone());
    }

    for file_list in grouped.values_mut() {
        file_list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    }

    grouped
}

/// Represents a group of files that share the same filename.
#[derive(Debug, Clone)]
pub struct SameNameGroup {
    /// The shared base name of all files in the group (normalized, case-insensitive).
    pub shared_name: String,
    /// All files that have that name, in different locations.
    pub files: Vec<FileInfo>,
}

/// Finds files that share the same name (regardless of location or content).
///
/// ### How it works:
/// 1. Groups all files by their full name (with extension), normalized to lowercase.
/// 2. Returns only groups with 2 or more files, sorted by number of matches descending.
///
/// ### Difference from exact duplicates:
/// - **Identical copies** (`duplicates` module): same content verified by SHA-256 hash.
/// - **Same name** (this function): same filename, content may differ.
///
/// Useful for detecting different versions of the same file spread across multiple folders.
pub fn find_same_name_files(files: &[FileInfo]) -> Vec<SameNameGroup> {
    let mut by_name: HashMap<String, Vec<FileInfo>> = HashMap::new();

    for file in files {
        // Normalize to lowercase so "Thesis.pdf" and "thesis.pdf" are treated as equal
        let key = file.name().to_lowercase();
        by_name.entry(key).or_default().push(file.clone());
    }

    let mut groups: Vec<SameNameGroup> = by_name
        .into_iter()
        .filter(|(_, group)| group.len() > 1)
        .map(|(shared_name, files)| SameNameGroup { shared_name, files })
        .collect();

    // Sort by most to least matches
    groups.sort_by(|a, b| b.files.len().cmp(&a.files.len()));

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size_bytes: u64) -> FileInfo {
        let path = PathBuf::from(path);
        let extension = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase());
        FileInfo { path, size_bytes, extension }
    }

    #[test]
    fn test_queries() {
        let files = vec![
            file("a/report.PDF", 10),
            file("b/report.pdf", 30),
            file("b/photo.jpg", 20),
        ];

        let top: Vec<u64> = get_top_largest(&files, 2).iter().map(|f| f.size_bytes).collect();
        assert_eq!(top, vec![30, 20]);

        assert_eq!(filter_by_extension(&files, ".pdf").len(), 2);
        assert!(is_document(&files[0]) && !is_document(&files[2]));

        let same_name = find_same_name_files(&files);
        assert_eq!(same_name.len(), 1);
        assert_eq!(same_name[0].shared_name, "report.pdf");

        let folders = group_by_folder(&files);
        assert_eq!(folders[&PathBuf::from("b")].len(), 2);
    }
}
