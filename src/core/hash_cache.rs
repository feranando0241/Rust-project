//! Persistent cache of file hashes, so scanning the same folder again doesn't
//! re-read files that haven't changed.
//!
//! Entries are keyed by the file's identity on disk (device + inode) and hold a
//! fingerprint taken *before* hashing: size, modification time and change time
//! (ctime). The OS updates ctime on every content or metadata change, and tools
//! that preserve mtime (`cp -p`, `rsync -t`) can't set it, so a matching
//! fingerprint means the content is the one that was hashed. Any mismatch, a
//! missing entry, or an unreadable cache file just means "hash it again": the
//! cache can make things faster, never wrong.
//!
//! Deleting files never trusts this cache. `duplicates::trash_duplicates` takes
//! no cache and re-reads every file from disk before moving anything.

use crate::core::models::FileId;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// First line of the cache file; a different line means an old or foreign
/// format, and the whole file is ignored.
const HEADER: &str = "oxide_clean hash cache v1";
/// Entries not used for this many days are dropped when saving.
const MAX_IDLE_DAYS: u64 = 90;

/// Which of a file's two hashes is being looked up.
#[derive(Debug, Clone, Copy)]
pub enum HashKind {
    /// SHA-256 of the first 4 KB.
    Prefix,
    /// SHA-256 of the whole file.
    Full,
}

/// State of a file when it was hashed. Any change invalidates its hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    size: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

#[derive(Debug, Clone)]
struct Entry {
    fingerprint: Fingerprint,
    last_used_day: u64,
    prefix: Option<String>,
    full: Option<String>,
}

/// Hash cache shared by the hashing threads.
pub struct HashCache {
    enabled: bool,
    entries: Mutex<HashMap<FileId, Entry>>,
    hits: AtomicUsize,
}

impl HashCache {
    /// A cache that stores nothing: every hash is computed (`--no-cache`).
    pub fn disabled() -> HashCache {
        HashCache {
            enabled: false,
            entries: Mutex::new(HashMap::new()),
            hits: AtomicUsize::new(0),
        }
    }

    /// Loads the cache from `file`. A missing, unreadable, corrupt or
    /// old-format file gives an empty cache instead of an error.
    pub fn load(file: &Path) -> HashCache {
        let entries = fs::read_to_string(file)
            .ok()
            .and_then(|text| parse(&text))
            .unwrap_or_default();
        HashCache {
            enabled: true,
            entries: Mutex::new(entries),
            hits: AtomicUsize::new(0),
        }
    }

    /// Returns the cached hash of `path` if the file is unchanged since it was
    /// hashed; otherwise runs `compute`, stores the result and returns it.
    pub fn get_or_compute(
        &self,
        path: &Path,
        kind: HashKind,
        compute: impl FnOnce(&Path) -> io::Result<String>,
    ) -> io::Result<String> {
        if !self.enabled {
            return compute(path);
        }
        // Without a stable identity the file can't be cached; just hash it
        let Ok((id, fingerprint)) = stat(path) else {
            return compute(path);
        };
        if let Some(hash) = self.lookup(id, fingerprint, kind) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(hash);
        }
        // The fingerprint was taken before reading: if the file changes while
        // it is being hashed, the next run sees a different fingerprint.
        let hash = compute(path)?;
        self.store(id, fingerprint, kind, hash.clone());
        Ok(hash)
    }

    /// How many hashes were reused instead of computed.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    /// Writes the cache to `file`, dropping entries unused for `MAX_IDLE_DAYS`.
    /// Written to a temporary file and renamed, so an interrupted save never
    /// leaves a half-written cache behind.
    pub fn save(&self, file: &Path) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let today = today();
        let entries = self.entries.lock().unwrap();

        let mut out = String::with_capacity(HEADER.len() + entries.len() * 200);
        out.push_str(HEADER);
        out.push('\n');
        for (id, e) in entries.iter() {
            if today.saturating_sub(e.last_used_day) > MAX_IDLE_DAYS {
                continue;
            }
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                id.dev,
                id.ino,
                e.fingerprint.size,
                e.fingerprint.mtime.0,
                e.fingerprint.mtime.1,
                e.fingerprint.ctime.0,
                e.fingerprint.ctime.1,
                e.last_used_day,
                e.prefix.as_deref().unwrap_or("-"),
                e.full.as_deref().unwrap_or("-"),
            );
        }
        drop(entries);

        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        // Unique temp name so two running instances don't write the same file
        let tmp = file.with_extension(format!("tmp.{}", std::process::id()));
        fs::write(&tmp, out)?;
        fs::rename(&tmp, file)
    }

    fn lookup(&self, id: FileId, fingerprint: Fingerprint, kind: HashKind) -> Option<String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(&id)?;
        if entry.fingerprint != fingerprint {
            return None;
        }
        let hash = match kind {
            HashKind::Prefix => entry.prefix.clone(),
            HashKind::Full => entry.full.clone(),
        }?;
        entry.last_used_day = today();
        Some(hash)
    }

    fn store(&self, id: FileId, fingerprint: Fingerprint, kind: HashKind, hash: String) {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.entry(id).or_insert(Entry {
            fingerprint,
            last_used_day: today(),
            prefix: None,
            full: None,
        });
        // Same inode but the file changed (or the inode was reused): start over
        if entry.fingerprint != fingerprint {
            *entry = Entry {
                fingerprint,
                last_used_day: today(),
                prefix: None,
                full: None,
            };
        }
        entry.last_used_day = today();
        match kind {
            HashKind::Prefix => entry.prefix = Some(hash),
            HashKind::Full => entry.full = Some(hash),
        }
    }

    /// Test-only: records `hash` for the file's *current* state, as if it had
    /// been computed. Used to prove that a wrong cache can't cause a deletion.
    #[cfg(test)]
    pub fn insert_for_test(&self, path: &Path, kind: HashKind, hash: &str) {
        let (id, fingerprint) = stat(path).unwrap();
        self.store(id, fingerprint, kind, hash.to_string());
    }
}

/// Days since the Unix epoch (granularity is all the idle-pruning needs).
fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

#[cfg(unix)]
fn stat(path: &Path) -> io::Result<(FileId, Fingerprint)> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path)?;
    Ok((
        FileId::from_metadata(&m),
        Fingerprint {
            size: m.size(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
        },
    ))
}

// ponytail: no cache off Unix (no stable file id / ctime in std); every hash
// is computed. Add Windows file IDs if Windows support matters.
#[cfg(not(unix))]
fn stat(_path: &Path) -> io::Result<(FileId, Fingerprint)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no stable file identity",
    ))
}

/// Parses the cache file. Any malformed line rejects the whole file: a
/// partially trusted cache isn't worth the risk, and rebuilding it is cheap.
fn parse(text: &str) -> Option<HashMap<FileId, Entry>> {
    let mut lines = text.lines();
    if lines.next()? != HEADER {
        return None;
    }

    let mut entries = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 10 {
            return None;
        }
        let hash = |s: &str| -> Option<Option<String>> {
            match s {
                "-" => Some(None),
                // A SHA-256 written by us: exactly 64 lowercase hex digits
                h if h.len() == 64
                    && h.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
                {
                    Some(Some(h.to_string()))
                }
                _ => None,
            }
        };
        let id = FileId {
            dev: f[0].parse().ok()?,
            ino: f[1].parse().ok()?,
        };
        let fingerprint = Fingerprint {
            size: f[2].parse().ok()?,
            mtime: (f[3].parse().ok()?, f[4].parse().ok()?),
            ctime: (f[5].parse().ok()?, f[6].parse().ok()?),
        };
        entries.insert(
            id,
            Entry {
                fingerprint,
                last_used_day: f[7].parse().ok()?,
                prefix: hash(f[8])?,
                full: hash(f[9])?,
            },
        );
    }
    Some(entries)
}

#[cfg(all(test, unix))]
mod tests {
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
}
