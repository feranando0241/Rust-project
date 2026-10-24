//! Unit tests for `src/core/hash_cache.rs`, loaded there as its `tests` module.

use super::*;
use crate::core::test_support::temp_dir;
use std::fs::File;
use std::time::Duration;

/// A stand-in hash function that records how often it was called.
fn counting_hash(calls: &AtomicUsize) -> impl Fn(&Path) -> io::Result<String> + '_ {
    move |path| {
        calls.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{:064x}", fs::read(path)?.len()))
    }
}

#[test]
fn test_unchanged_file_is_reused_across_runs() {
    let dir = temp_dir("cache_reuse");
    let file = dir.join("a.txt");
    let cache_file = dir.join("cache.tsv");
    fs::write(&file, "hello").unwrap();
    let calls = AtomicUsize::new(0);

    let first = HashCache::load(&cache_file);
    let h1 = first
        .get_or_compute(&file, HashKind::Full, counting_hash(&calls))
        .unwrap();
    first.save(&cache_file).unwrap();

    let second = HashCache::load(&cache_file);
    let h2 = second
        .get_or_compute(&file, HashKind::Full, counting_hash(&calls))
        .unwrap();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(h1, h2);
    assert_eq!(calls.into_inner(), 1, "second run must not hash again");
    assert_eq!(second.hits(), 1);
}

#[test]
fn test_changed_content_with_restored_mtime_is_rehashed() {
    // Simulates `cp -p` / `rsync -t`: new content, same size, old mtime.
    let dir = temp_dir("cache_restored_mtime");
    let file = dir.join("a.txt");
    fs::write(&file, "AAAA").unwrap();
    let mtime = fs::metadata(&file).unwrap().modified().unwrap();

    let cache = HashCache::load(&dir.join("cache.tsv"));
    let real_hash = |p: &Path| fs::read_to_string(p);
    let before = cache
        .get_or_compute(&file, HashKind::Full, real_hash)
        .unwrap();

    std::thread::sleep(Duration::from_millis(10));
    fs::write(&file, "BBBB").unwrap();
    File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    assert_eq!(
        fs::metadata(&file).unwrap().modified().unwrap(),
        mtime,
        "mtime restored"
    );

    let after = cache
        .get_or_compute(&file, HashKind::Full, real_hash)
        .unwrap();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(before, "AAAA");
    assert_eq!(
        after, "BBBB",
        "ctime changed, so the old hash must not be reused"
    );
    assert_eq!(cache.hits(), 0);
}

#[test]
fn test_corrupt_or_foreign_cache_is_ignored() {
    let dir = temp_dir("cache_corrupt");
    let cache_file = dir.join("cache.tsv");
    let file = dir.join("a.txt");
    fs::write(&file, "data").unwrap();

    for content in [
        "garbage",
        "oxide_clean hash cache v0\n1\t2\t3\t4\t5\t6\t7\t8\t-\t-\n",
        "oxide_clean hash cache v1\n1\t2\tnot-a-number\t4\t5\t6\t7\t8\t-\t-\n",
        "oxide_clean hash cache v1\n1\t2\t3\t4\t5\t6\t7\t8\t-\tNOT-A-SHA256\n",
        "oxide_clean hash cache v1\n1\t2\t3\n",
    ] {
        fs::write(&cache_file, content).unwrap();
        let calls = AtomicUsize::new(0);
        let cache = HashCache::load(&cache_file);
        cache
            .get_or_compute(&file, HashKind::Full, counting_hash(&calls))
            .unwrap();
        assert_eq!(
            calls.into_inner(),
            1,
            "bad cache must be treated as empty: {:?}",
            content
        );
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_disabled_cache_never_stores_or_saves() {
    let dir = temp_dir("cache_disabled");
    let file = dir.join("a.txt");
    let cache_file = dir.join("cache.tsv");
    fs::write(&file, "x").unwrap();
    let calls = AtomicUsize::new(0);

    let cache = HashCache::disabled();
    cache
        .get_or_compute(&file, HashKind::Full, counting_hash(&calls))
        .unwrap();
    cache
        .get_or_compute(&file, HashKind::Full, counting_hash(&calls))
        .unwrap();
    cache.save(&cache_file).unwrap();
    let saved = cache_file.exists();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(calls.into_inner(), 2);
    assert!(!saved);
}
