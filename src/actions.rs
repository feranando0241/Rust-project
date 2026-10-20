use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use crate::models::FileInfo;

/// Represents a verified group of duplicate files.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// SHA-256 hash shared by all files in the group.
    /// Re-checked against the kept file right before trashing its copies.
    pub hash: String,
    pub file_size: u64,
    /// The file to keep: the oldest by modification time.
    pub original: FileInfo,
    /// The redundant files that are candidates to be sent to the Trash.
    pub duplicates: Vec<FileInfo>,
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

/// Computes the SHA-256 cryptographic hash of a file's content.
pub fn calculate_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192]; // 8 KB buffer for efficient chunked reading

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Finds real duplicates using a two-step approach:
/// 1. Group by file size in bytes (fast, zero-cost pre-filter).
/// 2. For groups with more than one file, compute SHA-256 to confirm 100% match.
pub fn find_exact_duplicates(files: &[FileInfo]) -> Vec<DuplicateGroup> {
    // Step 1: Group by size
    let mut by_size: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for file in files {
        if file.size_bytes > 0 {
            by_size.entry(file.size_bytes).or_default().push(file.clone());
        }
    }

    let candidate_groups: Vec<Vec<FileInfo>> = by_size
        .into_values()
        .filter(|list| list.len() > 1)
        .collect();

    // Step 2: Group by SHA-256 hash within each same-size group
    let mut duplicate_groups = Vec::new();

    for candidates in candidate_groups {
        let mut by_hash: HashMap<String, Vec<FileInfo>> = HashMap::new();
        let mut seen_identities = HashSet::new();

        for file in candidates {
            // Hardlinks share the same data on disk: trashing one frees nothing,
            // so only the first path to each underlying file is considered.
            if let Some(identity) = file_identity(&file.path)
                && !seen_identities.insert(identity)
            {
                continue;
            }
            if let Ok(hash) = calculate_sha256(&file.path) {
                by_hash.entry(hash).or_default().push(file);
            }
        }

        for (hash, mut matching_files) in by_hash {
            if matching_files.len() > 1 {
                // Keep the oldest file (by modification time); ties and files
                // without readable timestamps fall back to path order.
                matching_files.sort_by_cached_key(|f| {
                    let modified = fs::metadata(&f.path).and_then(|m| m.modified()).ok();
                    (modified.is_none(), modified, f.path.clone())
                });
                let file_size = matching_files[0].size_bytes;
                let original = matching_files[0].clone();
                let duplicates = matching_files[1..].to_vec();

                duplicate_groups.push(DuplicateGroup {
                    hash,
                    file_size,
                    original,
                    duplicates,
                });
            }
        }
    }

    // Sort from most to least wasted space
    duplicate_groups.sort_by(|a, b| {
        let wasted_a = a.file_size * a.duplicates.len() as u64;
        let wasted_b = b.file_size * b.duplicates.len() as u64;
        wasted_b.cmp(&wasted_a)
    });

    duplicate_groups
}

/// Identifies the underlying file on disk (device + inode), so different
/// paths to the same data (hardlinks) can be recognized.
#[cfg(unix)]
fn file_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

// ponytail: no hardlink detection off Unix (Windows file IDs need an unstable API);
// add it via GetFileInformationByHandle if Windows support matters.
#[cfg(not(unix))]
fn file_identity(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// Outcome of moving duplicates to the Trash. Every file is attempted;
/// one failure doesn't stop the rest.
#[derive(Debug, Default)]
pub struct TrashReport {
    pub moved: usize,
    pub freed_bytes: u64,
    /// Files that were not moved, with the reason.
    pub failed: Vec<(PathBuf, String)>,
}

/// Moves the redundant copies of every group to the operating system Trash.
///
/// Files can change while the user reads the list, so right before acting:
/// - the kept file is re-hashed; if it is missing or its content changed, the
///   whole group is skipped, so the last good copy is never trashed;
/// - each copy must still have the scanned size, otherwise it is skipped.
pub fn trash_duplicates(groups: &[DuplicateGroup]) -> TrashReport {
    let mut report = TrashReport::default();

    for group in groups {
        let original_intact = calculate_sha256(&group.original.path)
            .map(|hash| hash == group.hash)
            .unwrap_or(false);
        if !original_intact {
            let reason = format!(
                "skipped: the kept copy '{}' is missing or changed since the scan",
                group.original.path.display()
            );
            for dupe in &group.duplicates {
                report.failed.push((dupe.path.clone(), reason.clone()));
            }
            continue;
        }

        for dupe in &group.duplicates {
            // ponytail: size check only (cheap); re-hash each copy too if edits
            // between scan and confirmation turn out to matter.
            let current_size = fs::metadata(&dupe.path).map(|m| m.len()).ok();
            if current_size != Some(group.file_size) {
                report
                    .failed
                    .push((dupe.path.clone(), "skipped: missing or changed since the scan".to_string()));
                continue;
            }

            match trash::delete(&dupe.path) {
                Ok(()) => {
                    report.moved += 1;
                    report.freed_bytes += group.file_size;
                }
                Err(err) => report.failed.push((dupe.path.clone(), err.to_string())),
            }
        }
    }

    report
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
/// - **Identical copies** (option 5): same content verified by SHA-256 hash.
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
