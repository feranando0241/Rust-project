//! Moving the extra copies of each duplicate group to the Trash, with every
//! file re-read from disk right before it is touched.

use crate::core::Progress;
use crate::core::duplicates::DuplicateGroup;
use crate::core::hash_cache::HashCache;
use crate::core::hashing::{hash_in_parallel, hash_threads};
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
mod tests {
    use super::*;
    use crate::core::duplicates::find_exact_duplicates;
    use crate::core::hash_cache::HashKind;
    use crate::core::hashing::{full_hash, prefix_hash};
    use crate::core::scanner::scan_directory;
    use crate::core::test_support::{detect, fake_trash, no_progress, temp_dir};
    use std::fs::{self, File};

    #[test]
    fn test_trash_moves_verified_copies_and_keeps_originals() {
        let dir = temp_dir("trash_success");
        fs::create_dir_all(dir.join("photos")).unwrap();
        fs::write(dir.join("a_keep.txt"), "group one").unwrap();
        fs::write(dir.join("b_copy.txt"), "group one").unwrap();
        fs::write(dir.join("photos/c_copy.txt"), "group one").unwrap();
        fs::write(dir.join("d_keep.bin"), vec![9u8; 5000]).unwrap();
        fs::write(dir.join("e_copy.bin"), vec![9u8; 5000]).unwrap();

        let groups = detect(&dir);
        let max_done = AtomicUsize::new(0);
        let report = remove_duplicates(
            &groups,
            &|done, _| {
                max_done.fetch_max(done, Ordering::Relaxed);
            },
            &fake_trash(&dir),
        );

        let originals_kept = groups.iter().all(|g| g.original().path.exists());
        let copies_gone = groups
            .iter()
            .flat_map(|g| g.paths_to_remove())
            .all(|d| !d.path.exists());
        let in_fake_trash = fs::read_dir(dir.join(".fake_trash")).unwrap().count();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(report.moved, 3);
        assert_eq!(report.freed_bytes, 2 * 9 + 5000);
        assert!(report.failed.is_empty());
        assert!(originals_kept && copies_gone);
        assert_eq!(in_fake_trash, 3);
        assert_eq!(max_done.into_inner(), 5, "every involved file is verified");
    }

    #[test]
    fn test_trash_reports_removal_failures_and_continues() {
        let dir = temp_dir("trash_failure");
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(dir.join(name), "same").unwrap();
        }
        let groups = detect(&dir);

        // The first removal fails (e.g. permission denied); the second must still happen
        let attempts = AtomicUsize::new(0);
        let flaky = |path: &Path| {
            if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                Err("permission denied".to_string())
            } else {
                fake_trash(&dir)(path)
            }
        };
        let report = remove_duplicates(&groups, &no_progress, &flaky);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(report.moved, 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(
            report.failed[0].1,
            SkipReason::CouldNotMove("permission denied".to_string())
        );
    }

    #[test]
    fn test_trash_skips_group_when_kept_copy_changed() {
        let dir = temp_dir("changed_original");
        fs::write(dir.join("a.txt"), "same content").unwrap();
        fs::write(dir.join("b.txt"), "same content").unwrap();

        let groups = detect(&dir);
        assert_eq!(groups.len(), 1);
        // The kept copy is edited after the scan: its duplicate must survive
        fs::write(&groups[0].original().path, "edited after scan").unwrap();

        let report = remove_duplicates(&groups, &no_progress, &fake_trash(&dir));
        let duplicate_still_there = groups[0].paths_to_remove()[0].path.exists();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(report.moved, 0);
        assert_eq!(report.failed.len(), 1);
        assert!(duplicate_still_there);
    }

    /// Regression for a bug found by `prop_removal_never_loses_content` (seed 16):
    /// a copy with two paths (hardlinks) lost only one of them, so its data
    /// stayed on disk while the report claimed the space was freed.
    #[cfg(unix)]
    #[test]
    fn test_hardlinked_copy_is_removed_with_all_its_paths() {
        use std::time::{Duration, SystemTime};

        let dir = temp_dir("hardlinked_copy");
        let keep = dir.join("a_keep.txt");
        fs::write(&keep, "same bytes").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(&keep)
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::write(dir.join("b_copy.txt"), "same bytes").unwrap();
        fs::hard_link(dir.join("b_copy.txt"), dir.join("c_link_to_copy.txt")).unwrap();

        let groups = detect(&dir);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original().path, keep);
        assert_eq!(groups[0].extra_copies(), 1, "one extra file...");
        assert_eq!(
            groups[0].paths_to_remove().len(),
            2,
            "...reachable by two paths"
        );

        let report = remove_duplicates(&groups, &no_progress, &fake_trash(&dir));
        let copy_paths_gone =
            !dir.join("b_copy.txt").exists() && !dir.join("c_link_to_copy.txt").exists();
        fs::remove_dir_all(&dir).unwrap();

        assert!(copy_paths_gone, "every path to the copy must be removed");
        assert_eq!(report.moved, 2);
        assert_eq!(report.freed_bytes, 10, "the copy's size, counted once");
    }

    #[cfg(unix)]
    #[test]
    fn test_poisoned_cache_cannot_cause_a_deletion() {
        let dir = temp_dir("poisoned_cache");
        let original = dir.join("a_original.txt");
        let different = dir.join("b_different.txt");
        fs::write(&original, "real content").unwrap();
        fs::write(&different, "other conten").unwrap(); // same size, different bytes

        // A corrupted or tampered cache claims both files have the same hashes
        let cache = HashCache::load(&dir.join("no-such-cache.tsv"));
        let real_prefix = prefix_hash(&original).unwrap();
        let real_full = full_hash(&original).unwrap();
        for path in [&original, &different] {
            cache.insert_for_test(path, HashKind::Prefix, &real_prefix);
            cache.insert_for_test(path, HashKind::Full, &real_full);
        }

        // Detection trusts the cache, so it reports a false duplicate...
        let files = scan_directory(&dir, &no_progress).unwrap();
        let groups = find_exact_duplicates(&files, &cache, &no_progress);
        assert_eq!(groups.len(), 1, "the poisoned cache fooled detection");

        // ...but deletion re-reads the disk and refuses to move the different file
        let report = remove_duplicates(&groups, &no_progress, &fake_trash(&dir));
        let both_still_there = original.exists() && different.exists();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(report.moved, 0);
        assert_eq!(report.failed.len(), 1);
        assert!(
            both_still_there,
            "a wrong cache must never cause a deletion"
        );
    }
}
