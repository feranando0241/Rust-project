//! Duplicate detection (size → 4 KB prefix → full SHA-256) and moving the
//! extra copies to the Trash with safety checks.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use crate::core::models::FileInfo;

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

/// Bytes read by `hash_prefix`: enough to tell most different files apart.
const PREFIX_BYTES: u64 = 4096;

/// SHA-256 of only the first `PREFIX_BYTES` of a file. A cheap filter: two files
/// with different prefixes can't be identical, so they never reach the full hash.
fn hash_prefix(path: &Path) -> io::Result<String> {
    let mut prefix = Vec::with_capacity(PREFIX_BYTES as usize);
    File::open(path)?.take(PREFIX_BYTES).read_to_end(&mut prefix)?;
    Ok(format!("{:x}", Sha256::digest(&prefix)))
}

/// Finds real duplicates using a three-step approach:
/// 1. Group by file size in bytes (fast, zero-cost pre-filter).
/// 2. Within each size group, compare the first 4 KB of each file.
/// 3. Confirm the remaining candidates with a full SHA-256.
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
        let mut by_prefix: HashMap<String, Vec<FileInfo>> = HashMap::new();
        let mut seen_identities = HashSet::new();

        // Step 2: cheap prefix filter, so most files are never read in full
        for file in candidates {
            // Hardlinks share the same data on disk: trashing one frees nothing,
            // so only the first path to each underlying file is considered.
            if let Some(identity) = file_identity(&file.path)
                && !seen_identities.insert(identity)
            {
                continue;
            }
            if let Ok(prefix) = hash_prefix(&file.path) {
                by_prefix.entry(prefix).or_default().push(file);
            }
        }

        // Step 3: full hash, only for files whose prefixes already match
        for same_prefix in by_prefix.into_values().filter(|list| list.len() > 1) {
            for file in same_prefix {
                if let Ok(hash) = calculate_sha256(&file.path) {
                    by_hash.entry(hash).or_default().push(file);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scanner::scan_directory;

    /// Creates an empty, unique temp directory for a test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxide_clean_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_duplicates_keep_oldest() {
        use std::time::{Duration, SystemTime};

        let dir = temp_dir("keep_oldest");
        let newer = dir.join("a_newer.txt");
        let older = dir.join("b_older.txt");
        fs::write(&newer, "same content").unwrap();
        fs::write(&older, "same content").unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(3600);
        File::options().write(true).open(&older).unwrap().set_modified(old_time).unwrap();

        let groups = find_exact_duplicates(&scan_directory(&dir).unwrap());
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, older);
        assert_eq!(groups[0].duplicates[0].path, newer);
    }

    #[cfg(unix)]
    #[test]
    fn test_hardlinks_are_not_duplicates() {
        let dir = temp_dir("hardlinks");
        fs::write(dir.join("a.txt"), "same content").unwrap();
        fs::hard_link(dir.join("a.txt"), dir.join("b.txt")).unwrap();

        let groups = find_exact_duplicates(&scan_directory(&dir).unwrap());
        fs::remove_dir_all(&dir).unwrap();

        assert!(groups.is_empty(), "hardlinks to one file must not be reported as duplicates");
    }

    #[test]
    fn test_same_prefix_different_content_not_duplicates() {
        let dir = temp_dir("same_prefix");
        // Identical first 4 KB, different bytes after that: the prefix filter
        // lets them through, the full hash must reject them.
        let prefix = "x".repeat(5000);
        fs::write(dir.join("a.bin"), format!("{}AAAA", prefix)).unwrap();
        fs::write(dir.join("b.bin"), format!("{}BBBB", prefix)).unwrap();

        let groups = find_exact_duplicates(&scan_directory(&dir).unwrap());
        fs::remove_dir_all(&dir).unwrap();

        assert!(groups.is_empty());
    }

    #[test]
    fn test_trash_skips_group_when_kept_copy_changed() {
        let dir = temp_dir("changed_original");
        fs::write(dir.join("a.txt"), "same content").unwrap();
        fs::write(dir.join("b.txt"), "same content").unwrap();

        let groups = find_exact_duplicates(&scan_directory(&dir).unwrap());
        assert_eq!(groups.len(), 1);
        // The kept copy is edited after the scan: its duplicate must survive
        fs::write(&groups[0].original.path, "edited after scan").unwrap();

        let report = trash_duplicates(&groups);
        let duplicate_still_there = groups[0].duplicates[0].path.exists();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(report.moved, 0);
        assert_eq!(report.failed.len(), 1);
        assert!(duplicate_still_there);
    }
}
