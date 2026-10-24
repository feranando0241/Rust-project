//! Moving the extra copies of each duplicate group to the Trash, with every
//! file re-read from disk right before it is touched.

use crate::core::Progress;
use crate::core::duplicates::DuplicateGroup;
use crate::core::duplicates::hash_cache::HashCache;
use crate::core::duplicates::hashing::{hash_in_parallel, hash_threads};
use crate::core::models::FileInfo;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Outcome of moving duplicates to the Trash. Every file is attempted;
/// one failure doesn't stop the rest.
#[derive(Debug, Default)]
pub struct TrashReport {
    pub moved: usize,
    pub freed_bytes: u64,
    /// Files that were not moved, with the reason.
    pub failed: Vec<(PathBuf, SkipReason)>,
}

/// Why a file was left in place. `core` only says what happened; the UI
/// decides how to word it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The kept copy is missing or changed since the scan, so the whole group
    /// was left alone (it may now be the only good copy).
    KeptCopyChanged(PathBuf),
    /// This copy is missing or no longer matches.
    ChangedSinceScan,
    /// The system refused to move it (e.g. permissions); its message.
    CouldNotMove(String),
}

/// Moves the redundant copies of every group to the operating system Trash.
///
/// Right before acting, every file is read again from disk, because files can
/// change while the user reads the list and hashes may have come from the
/// cache. On purpose this function has no access to the cache:
/// - the kept file is re-hashed; if it is missing or its content changed, the
///   whole group is skipped, so the last good copy is never trashed;
/// - each copy is re-hashed; one that no longer matches is skipped.
///
/// `on_progress` gets `(verified, total)` over every file involved.
pub fn trash_duplicates(groups: &[DuplicateGroup], on_progress: Progress) -> TrashReport {
    remove_duplicates(groups, on_progress, &|path| {
        trash::delete(path).map_err(|e| e.to_string())
    })
}

/// `trash_duplicates` with the removal step passed in, so tests can run the
/// real verification logic without touching the user's Trash.
pub fn remove_duplicates(
    groups: &[DuplicateGroup],
    on_progress: Progress,
    remove: &dyn Fn(&Path) -> Result<(), String>,
) -> TrashReport {
    let total: usize = groups.iter().map(|g| 1 + g.paths_to_remove().len()).sum();
    let verified = AtomicUsize::new(0);
    on_progress(0, total);

    let mut report = TrashReport::default();

    for group in groups {
        // Verify one group at a time (its files in parallel), then act on it
        // right away: verifying everything first would widen the window in
        // which a file could change between its check and its move.
        let files: Vec<FileInfo> = std::iter::once(group.original())
            .chain(group.paths_to_remove())
            .cloned()
            .collect();
        let current: HashMap<PathBuf, String> =
            hash_in_parallel(&files, hash_threads(), &HashCache::disabled(), &|| {
                on_progress(verified.fetch_add(1, Ordering::Relaxed) + 1, total);
            })
            .into_iter()
            .map(|(file, hash)| (file.path, hash))
            .collect();
        let unchanged = |file: &FileInfo| current.get(&file.path) == Some(&group.hash);

        if !unchanged(group.original()) {
            let reason = SkipReason::KeptCopyChanged(group.original().path.clone());
            for path in group.paths_to_remove() {
                report.failed.push((path.path.clone(), reason.clone()));
            }
            continue;
        }

        for copy in group.extras() {
            // A copy's space comes back only once every one of its paths is gone
            let mut every_path_moved = true;
            for path in &copy.paths {
                if !unchanged(path) {
                    report
                        .failed
                        .push((path.path.clone(), SkipReason::ChangedSinceScan));
                    every_path_moved = false;
                    continue;
                }
                match remove(&path.path) {
                    Ok(()) => report.moved += 1,
                    Err(err) => {
                        report
                            .failed
                            .push((path.path.clone(), SkipReason::CouldNotMove(err)));
                        every_path_moved = false;
                    }
                }
            }
            // ponytail: hardlinks outside the scanned folder aren't seen, so the
            // space may not really come back; compare st_nlink if that matters.
            if every_path_moved {
                report.freed_bytes += group.file_size;
            }
        }
    }

    report
}

#[cfg(test)]
#[path = "../../tests/core_duplicates_trash.rs"]
mod tests;
