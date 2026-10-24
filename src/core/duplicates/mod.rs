//! Duplicate detection: size → 4 KB prefix → full SHA-256, so most files are
//! never read in full.
//!
//! - `hashing`     full and 4 KB prefix SHA-256, parallel hashing (private:
//!   only detection and `trash` may depend on how hashes are made)
//! - `hash_cache`  hashes saved between runs
//! - `trash`       moving extra copies to the Trash, re-verified from disk

use crate::core::Progress;
use crate::core::duplicates::hash_cache::{HashCache, HashKind};
use crate::core::duplicates::hashing::{hash_in_parallel, hash_threads, prefix_hash};
use crate::core::models::{FileId, FileInfo};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod hash_cache;
mod hashing;
pub mod trash;

#[cfg(all(test, unix))]
#[path = "../../tests/core_duplicates_props.rs"]
mod duplicates_props;

/// One distinct file with the group's content. It has several paths when it
/// has hardlinks inside the scanned folder; they are always kept or removed
/// together, since the space only comes back once every path is gone.
#[derive(Debug, Clone)]
pub struct FileCopy {
    /// Every path to this file found in the scan. The first is the one shown.
    pub paths: Vec<FileInfo>,
}

impl FileCopy {
    /// The path used to show and verify this copy.
    pub fn main(&self) -> &FileInfo {
        &self.paths[0]
    }

    /// True if any path to this copy is inside `folder`.
    pub fn is_inside(&self, folder: &Path) -> bool {
        self.paths.iter().any(|p| p.path.starts_with(folder))
    }
}

/// Represents a verified group of duplicate files.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// SHA-256 hash shared by all files in the group.
    /// Re-checked against every file right before trashing.
    pub hash: String,
    pub file_size: u64,
    /// Every distinct file with this content, oldest first.
    pub copies: Vec<FileCopy>,
    /// Which of `copies` stay, by index. Private so that a group can never
    /// end up keeping none of them: only `keep_where` changes it.
    kept: Vec<bool>,
}

impl DuplicateGroup {
    /// A group that keeps only its first (oldest) copy.
    fn new(hash: String, file_size: u64, copies: Vec<FileCopy>) -> DuplicateGroup {
        let mut kept = vec![false; copies.len()];
        kept[0] = true;
        DuplicateGroup {
            hash,
            file_size,
            copies,
            kept,
        }
    }

    /// True if `copies[index]` stays.
    pub fn is_kept(&self, index: usize) -> bool {
        self.kept[index]
    }

    /// The first copy that stays; it is shown for the group and verified
    /// before anything in it is moved.
    pub fn kept(&self) -> &FileCopy {
        let index = self
            .kept
            .iter()
            .position(|&k| k)
            .expect("a group always keeps a copy");
        &self.copies[index]
    }

    /// The path shown for the copy that stays.
    pub fn original(&self) -> &FileInfo {
        self.kept().main()
    }

    /// The copies that would be moved to the Trash.
    pub fn extras(&self) -> impl Iterator<Item = &FileCopy> {
        self.copies
            .iter()
            .zip(&self.kept)
            .filter(|(_, kept)| !**kept)
            .map(|(copy, _)| copy)
    }

    /// How many distinct files would be removed (not paths).
    pub fn extra_copies(&self) -> usize {
        self.kept.iter().filter(|&&k| !k).count()
    }

    /// Every path that would be moved: all paths of every extra copy.
    pub fn paths_to_remove(&self) -> Vec<&FileInfo> {
        self.extras().flat_map(|copy| &copy.paths).collect()
    }

    /// Space freed if every extra copy is removed.
    pub fn wasted_bytes(&self) -> u64 {
        self.file_size * self.extra_copies() as u64
    }

    /// Keeps exactly the copies for which `keep` is true. Returns false,
    /// changing nothing, if that would keep none of them.
    pub fn keep_where(&mut self, keep: impl Fn(usize, &FileCopy) -> bool) -> bool {
        let kept: Vec<bool> = self
            .copies
            .iter()
            .enumerate()
            .map(|(i, c)| keep(i, c))
            .collect();
        if !kept.contains(&true) {
            return false;
        }
        self.kept = kept;
        true
    }

    /// Keeps every copy inside `folder` (and only those), so nothing inside
    /// it is ever moved. Returns false, changing nothing, if none is there.
    pub fn keep_inside(&mut self, folder: &Path) -> bool {
        self.keep_where(|_, copy| copy.is_inside(folder))
    }
}

/// `keep_inside` on every group; returns how many have a copy in `folder`.
pub fn keep_copies_inside(groups: &mut [DuplicateGroup], folder: &Path) -> usize {
    groups
        .iter_mut()
        .map(|g| g.keep_inside(folder))
        .filter(|&found| found)
        .count()
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
            // Each copy carries all of its hardlinks, so they're kept or removed together
            let copies = matching_files
                .into_iter()
                .map(|file| {
                    let links = FileId::of(&file.path)
                        .and_then(|id| other_links.remove(&id))
                        .unwrap_or_default();
                    FileCopy {
                        paths: std::iter::once(file).chain(links).collect(),
                    }
                })
                .collect();

            duplicate_groups.push(DuplicateGroup::new(hash, file_size, copies));
        }
    }

    // Sort from most to least wasted space; ties by path so the order is stable
    duplicate_groups.sort_by_cached_key(|g| (Reverse(g.wasted_bytes()), g.original().path.clone()));

    duplicate_groups
}

#[cfg(test)]
#[path = "../../tests/core_duplicates.rs"]
mod tests;
