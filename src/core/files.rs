//! Simple queries over a list of scanned files: biggest, by type, by name,
//! by folder, and where the space goes.

use crate::core::models::FileInfo;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::path::{Component, Path, PathBuf};

/// How many files "the biggest files" shows by default.
pub const DEFAULT_TOP: usize = 10;

/// Extensions treated as documents for the similarity comparison.
const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "txt", "md", "rs", "json", "csv", "py", "js", "c", "cpp",
];

/// True if the file is a document whose text can be compared (PDF, text, code).
pub fn is_document(file: &FileInfo) -> bool {
    // `FileInfo` extensions are already lowercase
    file.extension
        .as_deref()
        .is_some_and(|ext| DOCUMENT_EXTENSIONS.contains(&ext))
}

/// Total size and number of files of some group of files.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub bytes: u64,
    pub files: usize,
}

/// Where the space goes per top-level folder of `root`, each counted with
/// everything inside it. Files directly in `root` are under `None`.
/// Sorted from most to least space.
pub fn usage_by_top_folder(root: &Path, files: &[FileInfo]) -> Vec<(Option<String>, Usage)> {
    usage_by(files, |file| {
        let mut parts = file
            .path
            .strip_prefix(root)
            .unwrap_or(&file.path)
            .components();
        match (parts.next(), parts.next()) {
            // A first component followed by more means the file is inside a folder
            (Some(Component::Normal(dir)), Some(_)) => Some(dir.to_string_lossy().into_owned()),
            _ => None,
        }
    })
}

/// Where the space goes per file extension (`None`: no extension).
/// Sorted from most to least space.
pub fn usage_by_extension(files: &[FileInfo]) -> Vec<(Option<String>, Usage)> {
    usage_by(files, |file| file.extension.clone())
}

/// Adds up size and count per key; most space first, ties by key so the
/// order is stable.
fn usage_by<K: Hash + Eq + Ord>(
    files: &[FileInfo],
    key: impl Fn(&FileInfo) -> K,
) -> Vec<(K, Usage)> {
    let mut totals: HashMap<K, Usage> = HashMap::new();
    for file in files {
        let usage = totals.entry(key(file)).or_default();
        usage.bytes += file.size_bytes;
        usage.files += 1;
    }
    let mut rows: Vec<(K, Usage)> = totals.into_iter().collect();
    rows.sort_by(|(ka, a), (kb, b)| b.bytes.cmp(&a.bytes).then_with(|| ka.cmp(kb)));
    rows
}

/// Returns the `n` heaviest files in the list, sorted from largest to smallest.
pub fn get_top_largest(files: &[FileInfo], n: usize) -> Vec<FileInfo> {
    let mut sorted = files.to_vec();
    sorted.sort_by_key(|f| Reverse(f.size_bytes));
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
        file_list.sort_by_key(|f| Reverse(f.size_bytes));
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
    groups.sort_by_key(|g| Reverse(g.files.len()));

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size_bytes: u64) -> FileInfo {
        FileInfo::from_path(PathBuf::from(path), size_bytes)
    }

    #[test]
    fn test_queries() {
        let files = vec![
            file("a/report.PDF", 10),
            file("b/report.pdf", 30),
            file("b/photo.jpg", 20),
        ];

        let top: Vec<u64> = get_top_largest(&files, 2)
            .iter()
            .map(|f| f.size_bytes)
            .collect();
        assert_eq!(top, vec![30, 20]);

        assert_eq!(filter_by_extension(&files, ".pdf").len(), 2);
        assert!(is_document(&files[0]) && !is_document(&files[2]));

        let same_name = find_same_name_files(&files);
        assert_eq!(same_name.len(), 1);
        assert_eq!(same_name[0].shared_name, "report.pdf");

        let folders = group_by_folder(&files);
        assert_eq!(folders[&PathBuf::from("b")].len(), 2);
    }

    #[test]
    fn test_usage_by_folder_and_type() {
        let root = Path::new("/scan");
        let files = vec![
            file("/scan/notes.txt", 5),
            file("/scan/photos/2024/a.jpg", 100),
            file("/scan/photos/b.jpg", 50),
            file("/scan/docs/c.txt", 20),
            file("/scan/README", 1),
        ];

        let by_folder = usage_by_top_folder(root, &files);
        assert_eq!(
            by_folder,
            vec![
                (
                    Some("photos".to_string()),
                    Usage {
                        bytes: 150,
                        files: 2
                    }
                ),
                (
                    Some("docs".to_string()),
                    Usage {
                        bytes: 20,
                        files: 1
                    }
                ),
                (None, Usage { bytes: 6, files: 2 }),
            ]
        );

        let by_type = usage_by_extension(&files);
        assert_eq!(
            by_type,
            vec![
                (
                    Some("jpg".to_string()),
                    Usage {
                        bytes: 150,
                        files: 2
                    }
                ),
                (
                    Some("txt".to_string()),
                    Usage {
                        bytes: 25,
                        files: 2
                    }
                ),
                (None, Usage { bytes: 1, files: 1 }),
            ]
        );
    }
}
