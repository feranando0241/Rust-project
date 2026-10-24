//! Unit tests for `src/core/hashing.rs`, loaded there as its `tests` module.

use super::*;
use crate::core::scanner::scan_directory;
use crate::core::test_support::{no_progress, temp_dir};
use std::fs;

#[test]
fn test_parallel_hashing_matches_sequential() {
    let dir = temp_dir("parallel");
    // Several groups of identical files plus same-size distinct ones
    for group in 0..5u8 {
        for copy in 0..3 {
            fs::write(
                dir.join(format!("g{}_{}.bin", group, copy)),
                vec![group; 9000],
            )
            .unwrap();
        }
    }
    for distinct in 0..5u8 {
        let mut bytes = vec![200u8; 9000];
        bytes[8999] = distinct; // same size and prefix, different tail
        fs::write(dir.join(format!("d{}.bin", distinct)), bytes).unwrap();
    }
    let files: Vec<FileInfo> = scan_directory(&dir, &no_progress).unwrap();

    let mut sequential = hash_in_parallel(&files, 1, &HashCache::disabled(), &|| {});
    let mut parallel = hash_in_parallel(&files, 4, &HashCache::disabled(), &|| {});
    fs::remove_dir_all(&dir).unwrap();

    // Same hashes no matter how many threads, just in a different order
    sequential.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    parallel.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    let pairs = |v: &[(FileInfo, String)]| {
        v.iter()
            .map(|(f, h)| (f.path.clone(), h.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(pairs(&sequential), pairs(&parallel));
    assert_eq!(sequential.len(), 20);
}

#[test]
fn test_prefix_hash_only_reads_the_first_4_kb() {
    let dir = temp_dir("prefix");
    let start = "x".repeat(PREFIX_BYTES as usize);
    fs::write(dir.join("a.bin"), format!("{}AAAA", start)).unwrap();
    fs::write(dir.join("b.bin"), format!("{}BBBB", start)).unwrap();

    let same_prefix =
        prefix_hash(&dir.join("a.bin")).unwrap() == prefix_hash(&dir.join("b.bin")).unwrap();
    let same_full =
        full_hash(&dir.join("a.bin")).unwrap() == full_hash(&dir.join("b.bin")).unwrap();
    fs::remove_dir_all(&dir).unwrap();

    assert!(same_prefix && !same_full);
}
