//! Unit tests for `src/core/trash.rs`, loaded there as its `tests` module.

use super::*;
use crate::core::duplicates::find_exact_duplicates;
use crate::core::duplicates::hash_cache::HashKind;
use crate::core::duplicates::hashing::{full_hash, prefix_hash};
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
