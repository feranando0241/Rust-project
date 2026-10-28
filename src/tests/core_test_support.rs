//! Helpers shared by the unit and property tests of `core`.

use crate::core::duplicates::hash_cache::HashCache;
use crate::core::duplicates::{DuplicateGroup, find_exact_duplicates};
use crate::core::scanner::scan_directory;
use std::fs;
use std::path::{Path, PathBuf};

/// Creates an empty, unique temp directory for a test.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oxide_clean_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A progress callback that ignores progress.
pub fn no_progress(_done: usize, _total: usize) {}

/// Scans `dir` and finds its duplicates, without a cache.
pub fn detect(dir: &Path) -> Vec<DuplicateGroup> {
    let files = scan_directory(dir, &no_progress).unwrap();
    find_exact_duplicates(&files, &HashCache::disabled(), &no_progress)
}

/// Stand-in for the system Trash: moves files into `<dir>/.fake_trash/`
/// (a hidden folder, so later scans of `dir` skip it). Tests never touch the
/// user's real Trash, even if a bug tried to move the wrong file.
pub fn fake_trash(dir: &Path) -> impl Fn(&Path) -> Result<(), String> + '_ {
    move |path| {
        let bin = dir.join(".fake_trash");
        fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        let name = path
            .strip_prefix(dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('/', "__");
        fs::rename(path, bin.join(name)).map_err(|e| e.to_string())
    }
}

/// xorshift64*: small, fast, and deterministic for a given seed.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}
