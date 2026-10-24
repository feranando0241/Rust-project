//! How files are hashed: a full SHA-256, a cheap hash of the first 4 KB, and
//! a helper that spreads hashing over a few threads.

use crate::core::duplicates::hash_cache::{HashCache, HashKind};
use crate::core::models::FileInfo;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Bytes read by `prefix_hash`: enough to tell most different files apart.
const PREFIX_BYTES: u64 = 4096;

/// Upper bound on hashing threads.
///
/// Measured hashing 3 GB on an Apple A18 Pro with 2 performance and 4
/// efficiency cores and an SSD: 1 thread took 6.0 s, 2 took 3.6 s, 4 took
/// 2.6 s, 6 took 2.2 s, and 8 were no better. A cold cache gave the same
/// numbers, so on an SSD hashing is CPU-bound. 4 threads get most of the gain
/// and leave cores free for the rest of the system.
// ponytail: not measured on spinning disks, where parallel reads can be slower
// than sequential ones; add a `--threads` flag if that ever matters.
const MAX_HASH_THREADS: usize = 4;

/// SHA-256 of a file's whole content, as lowercase hex.
pub fn full_hash(path: &Path) -> io::Result<String> {
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

    Ok(format!("{:x}", hasher.finalize()))
}

/// SHA-256 of only the first `PREFIX_BYTES` of a file. A cheap filter: two
/// files with different prefixes can't be identical, so they never reach the
/// full hash.
pub fn prefix_hash(path: &Path) -> io::Result<String> {
    let mut prefix = Vec::with_capacity(PREFIX_BYTES as usize);
    File::open(path)?
        .take(PREFIX_BYTES)
        .read_to_end(&mut prefix)?;
    Ok(format!("{:x}", Sha256::digest(&prefix)))
}

/// Threads used for full hashing: one per CPU core, up to `MAX_HASH_THREADS`.
pub fn hash_threads() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_HASH_THREADS)
}

/// Computes the full hash of every file using `threads` workers that pull the
/// next file from a shared index, looking each one up in `cache` first.
/// Returns `(file, hash)` pairs in no particular order; unreadable files are
/// left out. `on_file_done` runs once per file, from whichever thread did it.
pub fn hash_in_parallel(
    files: &[FileInfo],
    threads: usize,
    cache: &HashCache,
    on_file_done: &(dyn Fn() + Sync),
) -> Vec<(FileInfo, String)> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(files.len()));

    thread::scope(|scope| {
        for _ in 0..threads.clamp(1, files.len().max(1)) {
            scope.spawn(|| {
                while let Some(file) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if let Ok(hash) = cache.get_or_compute(&file.path, HashKind::Full, full_hash) {
                        results.lock().unwrap().push((file.clone(), hash));
                    }
                    on_file_done();
                }
            });
        }
    });

    results.into_inner().unwrap()
}

#[cfg(test)]
#[path = "../../tests/core_duplicates_hashing.rs"]
mod tests;
