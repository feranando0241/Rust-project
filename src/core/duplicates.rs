//! Duplicate detection: size → 4 KB prefix → full SHA-256, so most files are
//! never read in full. Moving the copies away lives in `trash`.

use crate::core::Progress;
use crate::core::hash_cache::{HashCache, HashKind};
use crate::core::hashing::{hash_in_parallel, hash_threads, prefix_hash};
use crate::core::models::{FileId, FileInfo};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Represents a verified group of duplicate files.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// SHA-256 hash shared by all files in the group.
    /// Re-checked against every file right before trashing.
    pub hash: String,
    pub file_size: u64,
    /// The file to keep: the oldest by modification time. Any other hardlinks
    /// to it stay too (they are the same file).
    pub original: FileInfo,
    /// Every path to move to the Trash: one per extra copy, plus any other
    /// hardlinks to those copies found in the scan. Space only comes back once
    /// every path to a file is gone, so all of them are listed.
    pub duplicates: Vec<FileInfo>,
    /// How many distinct extra files (not paths) the group has. Each one
    /// frees `file_size` bytes when all of its paths are moved.
    pub extra_copies: usize,
}

/// Finds real duplicates using a three-step approach:
/// 1. Group by file size in bytes (fast, zero-cost pre-filter).
/// 2. Within each size group, compare the first 4 KB of each file.
/// 3. Confirm the remaining candidates with a full SHA-256, on several threads.
///
/// Hashes are looked up in `cache` first and only computed for files that
/// changed (pass `HashCache::disabled()` to always compute them).
///
/// `on_progress` gets `(checked, total)`, where `total` is the number of files
/// that share their size with another file (the only ones that need reading).
/// A file counts as checked once it is ruled out or fully hashed.
pub fn find_exact_duplicates(
    files: &[FileInfo],
    cache: &HashCache,
    on_progress: Progress,
) -> Vec<DuplicateGroup> {
    // Step 1: Group by size
    let mut by_size: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for file in files {
        if file.size_bytes > 0 {
            by_size
                .entry(file.size_bytes)
                .or_default()
                .push(file.clone());
        }
    }

    let candidate_groups: Vec<Vec<FileInfo>> = by_size
        .into_values()
        .filter(|list| list.len() > 1)
        .collect();

    let total: usize = candidate_groups.iter().map(Vec::len).sum();
    let mut checked = 0;
    on_progress(checked, total);

    // Files whose size and prefix both match another file's: only these are read in full
    let mut needs_full_hash: Vec<FileInfo> = Vec::new();
    // Extra paths (hardlinks) to a file that is already being checked
    let mut other_links: HashMap<FileId, Vec<FileInfo>> = HashMap::new();

    for candidates in candidate_groups {
        let group_len = candidates.len();
        let mut by_prefix: HashMap<String, Vec<FileInfo>> = HashMap::new();
        let mut seen_identities = HashSet::new();

        // Step 2: cheap prefix filter, so most files are never read in full
        for file in candidates {
            // Hardlinks are one file with several paths: hash it once, and
            // remember the other paths so they can be removed together.
            if let Some(identity) = FileId::of(&file.path)
                && !seen_identities.insert(identity)
            {
                other_links.entry(identity).or_default().push(file);
                continue;
            }
            if let Ok(prefix) = cache.get_or_compute(&file.path, HashKind::Prefix, prefix_hash) {
                by_prefix.entry(prefix).or_default().push(file);
            }
        }

        let before = needs_full_hash.len();
        needs_full_hash.extend(
            by_prefix
                .into_values()
                .filter(|list| list.len() > 1)
                .flatten(),
        );
        // Everything else in this size group was ruled out by the steps above
        checked += group_len - (needs_full_hash.len() - before);
        on_progress(checked, total);
    }

    // Step 3: full hash of the remaining candidates, spread over several threads
    let checked = AtomicUsize::new(checked);
    let hashed = hash_in_parallel(&needs_full_hash, hash_threads(), cache, &|| {
        on_progress(checked.fetch_add(1, Ordering::Relaxed) + 1, total);
    });

    // Identical content implies identical size, so the hash alone is the group key
    let mut by_hash: HashMap<String, Vec<FileInfo>> = HashMap::new();
    for (file, hash) in hashed {
        by_hash.entry(hash).or_default().push(file);
    }

    let mut duplicate_groups = Vec::new();
    for (hash, mut matching_files) in by_hash {
        if matching_files.len() > 1 {
            // Keep the oldest file (by modification time); ties and files
            // without readable timestamps fall back to path order. This also
            // makes the result independent of which thread finished first.
            matching_files.sort_by_cached_key(|f| {
                let modified = fs::metadata(&f.path).and_then(|m| m.modified()).ok();
                (modified.is_none(), modified, f.path.clone())
            });
            let file_size = matching_files[0].size_bytes;
            let original = matching_files[0].clone();
            let extra_copies = matching_files.len() - 1;
            // Each extra copy goes with all of its hardlinks, or no space is freed
            let duplicates = matching_files
                .into_iter()
                .skip(1)
                .flat_map(|copy| {
                    let links = FileId::of(&copy.path)
                        .and_then(|id| other_links.remove(&id))
                        .unwrap_or_default();
                    std::iter::once(copy).chain(links)
                })
                .collect();

            duplicate_groups.push(DuplicateGroup {
                hash,
                file_size,
                original,
                duplicates,
                extra_copies,
            });
        }
    }

    // Sort from most to least wasted space; ties by path so the order is stable
    duplicate_groups.sort_by_cached_key(|g| {
        (
            Reverse(g.file_size * g.extra_copies as u64),
            g.original.path.clone(),
        )
    });

    duplicate_groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scanner::scan_directory;
    use crate::core::test_support::{detect, temp_dir};
    use std::fs::File;

    #[test]
    fn test_duplicates_keep_oldest() {
        use std::time::{Duration, SystemTime};

        let dir = temp_dir("keep_oldest");
        let newer = dir.join("a_newer.txt");
        let older = dir.join("b_older.txt");
        fs::write(&newer, "same content").unwrap();
        fs::write(&older, "same content").unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(&older)
            .unwrap()
            .set_modified(old_time)
            .unwrap();

        let groups = detect(&dir);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, older);
        assert_eq!(groups[0].duplicates[0].path, newer);
    }

    #[test]
    fn test_groups_of_copies_among_same_size_files() {
        let dir = temp_dir("groups");
        // 5 groups of 3 identical files...
        for group in 0..5u8 {
            for copy in 0..3 {
                fs::write(
                    dir.join(format!("g{}_{}.bin", group, copy)),
                    vec![group; 9000],
                )
                .unwrap();
            }
        }
        // ...plus 5 same-size files that differ only at the very end
        for distinct in 0..5u8 {
            let mut bytes = vec![200u8; 9000];
            bytes[8999] = distinct;
            fs::write(dir.join(format!("d{}.bin", distinct)), bytes).unwrap();
        }

        let groups = detect(&dir);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(groups.len(), 5);
        assert!(
            groups
                .iter()
                .all(|g| g.duplicates.len() == 2 && g.extra_copies == 2)
        );
    }

    #[test]
    fn test_progress_ends_at_total() {
        let dir = temp_dir("progress");
        // Two identical files, one same-size different file, one unique size
        fs::write(dir.join("a.txt"), "same").unwrap();
        fs::write(dir.join("b.txt"), "same").unwrap();
        fs::write(dir.join("c.txt"), "diff").unwrap();
        fs::write(dir.join("d.txt"), "other size").unwrap();

        // Threads may report out of order, so track the highest count seen
        let max_done = AtomicUsize::new(0);
        let seen_total = AtomicUsize::new(usize::MAX);
        let files = scan_directory(&dir, &|_, _| {}).unwrap();
        find_exact_duplicates(&files, &HashCache::disabled(), &|done, total| {
            max_done.fetch_max(done, Ordering::Relaxed);
            seen_total.store(total, Ordering::Relaxed);
        });
        fs::remove_dir_all(&dir).unwrap();

        // Only the three 4-byte files need checking; all of them get checked
        assert_eq!(seen_total.into_inner(), 3);
        assert_eq!(max_done.into_inner(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn test_hardlinks_are_not_duplicates() {
        let dir = temp_dir("hardlinks");
        fs::write(dir.join("a.txt"), "same content").unwrap();
        fs::hard_link(dir.join("a.txt"), dir.join("b.txt")).unwrap();

        let groups = detect(&dir);
        fs::remove_dir_all(&dir).unwrap();

        assert!(
            groups.is_empty(),
            "hardlinks to one file must not be reported as duplicates"
        );
    }

    #[test]
    fn test_same_prefix_different_content_not_duplicates() {
        let dir = temp_dir("same_prefix");
        // Identical first 4 KB, different bytes after that: the prefix filter
        // lets them through, the full hash must reject them.
        let prefix = "x".repeat(5000);
        fs::write(dir.join("a.bin"), format!("{}AAAA", prefix)).unwrap();
        fs::write(dir.join("b.bin"), format!("{}BBBB", prefix)).unwrap();

        let groups = detect(&dir);
        fs::remove_dir_all(&dir).unwrap();

        assert!(groups.is_empty());
    }
}
