//! Unit tests for `src/core/scanner.rs`, loaded there as its `tests` module.

use super::*;
use crate::core::test_support::{no_progress, temp_dir};
use std::fs::{File, FileTimes};
use std::time::{Duration, SystemTime};

#[test]
fn test_scan_records_modified_and_accessed_times() {
    let dir = temp_dir("scan_times");
    let file = dir.join("old.bin");
    fs::write(&file, "data").unwrap();
    let day = Duration::from_secs(86_400);
    let modified = SystemTime::now() - 400 * day;
    let accessed = SystemTime::now() - 200 * day;
    File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(
            FileTimes::new()
                .set_modified(modified)
                .set_accessed(accessed),
        )
        .unwrap();

    let files = scan_directory(&dir, &no_progress).unwrap();
    fs::remove_dir_all(&dir).unwrap();

    // Within a second: some file systems store coarser timestamps
    let close = |a: Option<SystemTime>, b: SystemTime| {
        let a = a.expect("timestamp recorded");
        a.duration_since(b).unwrap_or_else(|e| e.duration()) < Duration::from_secs(1)
    };
    assert_eq!(files.len(), 1);
    assert!(close(files[0].modified, modified), "modified time");
    assert!(close(files[0].accessed, accessed), "accessed time");
}

#[test]
fn test_should_ignore_directory() {
    assert!(should_ignore_directory("node_modules"));
    assert!(should_ignore_directory("target"));
    assert!(should_ignore_directory(".git"));
    assert!(should_ignore_directory(".cache"));
    assert!(!should_ignore_directory("photos"));
    assert!(!should_ignore_directory("documents"));
}
