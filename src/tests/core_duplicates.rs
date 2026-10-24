//! Unit tests for `src/core/duplicates.rs`, loaded there as its `tests` module.

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
    assert_eq!(groups[0].original().path, older);
    assert_eq!(groups[0].paths_to_remove()[0].path, newer);
}

#[test]
fn test_keep_inside_keeps_every_copy_in_that_folder() {
    use std::time::{Duration, SystemTime};

    let dir = temp_dir("keep_inside");
    for folder in ["Downloads", "Photos/sub", "Other"] {
        fs::create_dir_all(dir.join(folder)).unwrap();
    }
    // The oldest copy is in Downloads, so it's kept by default
    let in_downloads = dir.join("Downloads/img.jpg");
    fs::write(&in_downloads, "same image").unwrap();
    let old = SystemTime::now() - Duration::from_secs(3600);
    File::options()
        .write(true)
        .open(&in_downloads)
        .unwrap()
        .set_modified(old)
        .unwrap();
    fs::write(dir.join("Photos/img.jpg"), "same image").unwrap();
    fs::write(dir.join("Photos/sub/img.jpg"), "same image").unwrap();

    let mut groups = detect(&dir);
    let default_kept = groups[0].original().path.clone();
    let default_extras = groups[0].extra_copies();
    let found_in_photos = groups[0].keep_inside(&dir.join("Photos"));
    let to_remove: Vec<_> = groups[0]
        .paths_to_remove()
        .iter()
        .map(|f| f.path.clone())
        .collect();
    let found_in_other = groups[0].keep_inside(&dir.join("Other"));
    let to_remove_after_other = groups[0].paths_to_remove().len();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(default_kept, in_downloads);
    assert_eq!(default_extras, 2);
    assert!(found_in_photos);
    assert_eq!(
        to_remove,
        vec![in_downloads],
        "both copies in Photos stay; only the one outside is extra"
    );
    assert_eq!(groups[0].extra_copies(), 1);
    assert!(!found_in_other, "no copy in Other: nothing changes");
    assert_eq!(to_remove_after_other, 1);
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
            .all(|g| g.paths_to_remove().len() == 2 && g.extra_copies() == 2)
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
