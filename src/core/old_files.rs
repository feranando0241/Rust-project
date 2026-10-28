//! Forgotten files: not modified or read for a while, and not just arrived.
//! Which timestamps decide and why: docs/adr/0004-forgotten-files.md.

use crate::core::files::Usage;
use crate::core::models::{FileId, FileInfo};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::time::{Duration, SystemTime};

/// Days without changes or reads after which a file counts as forgotten.
pub const DEFAULT_OLD_DAYS: u64 = 180;

/// Days after arriving on disk (or any other change to the file, its ctime)
/// during which a file is never forgotten, whatever its other dates say: an
/// unpacked archive keeps its old mtime, and on macOS its old birth time too.
pub const ARRIVAL_GRACE_DAYS: u64 = 30;

const SECS_PER_DAY: u64 = 86_400;

/// `n` days as a threshold, or `None` if it isn't a usable one: 0 would make
/// every file forgotten, and some values are too large to represent.
pub fn days(n: u64) -> Option<Duration> {
    if n == 0 {
        return None;
    }
    n.checked_mul(SECS_PER_DAY).map(Duration::from_secs)
}

/// How one file is judged against a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Neither modified nor read for longer than the threshold. Holds how
    /// long it has been untouched.
    Forgotten(Duration),
    /// Modified or read within the threshold.
    InUse,
    /// Would be forgotten, but arrived on disk (or changed) within the grace.
    JustArrived,
    /// The system reports no modification time, so it can't be judged.
    NoDate,
    /// One of its timestamps is later than `now`, so it can't be judged.
    FutureDate,
}

/// Judges one file. `now` is a parameter, not the clock, so the same input
/// always gives the same answer (and tests can pick any moment).
pub fn judge(file: &FileInfo, now: SystemTime, threshold: Duration) -> Verdict {
    let Some(modified) = file.modified else {
        return Verdict::NoDate;
    };
    if [Some(modified), file.accessed, file.changed]
        .iter()
        .flatten()
        .any(|&t| t > now)
    {
        return Verdict::FutureDate;
    }
    // No timestamp is in the future past this point
    let age = |t: SystemTime| now.duration_since(t).unwrap_or_default();

    // Untouched since the most recent change or recorded read
    let untouched = file
        .accessed
        .map_or(age(modified), |read| age(read).min(age(modified)));
    if untouched <= threshold {
        return Verdict::InUse;
    }

    let grace = days(ARRIVAL_GRACE_DAYS).expect("the grace fits in a Duration");
    match file.changed {
        Some(arrived) if age(arrived) < grace => Verdict::JustArrived,
        _ => Verdict::Forgotten(untouched),
    }
}

/// A forgotten file and how long it has been untouched.
#[derive(Debug, Clone)]
pub struct ForgottenFile {
    pub file: FileInfo,
    pub untouched: Duration,
}

/// What `find_old_files` found, including what it couldn't judge.
#[derive(Debug, Clone, Default)]
pub struct OldFiles {
    /// Biggest first (then longest untouched, then path). Each file on disk
    /// appears once, even if it has several hardlinks in the scan.
    pub forgotten: Vec<ForgottenFile>,
    /// Space and count of `forgotten`.
    pub total: Usage,
    /// Files protected by the arrival grace.
    pub just_arrived: usize,
    pub no_date: usize,
    pub future_date: usize,
}

/// Finds the files untouched for longer than `threshold` (see `judge`).
pub fn find_old_files(files: &[FileInfo], now: SystemTime, threshold: Duration) -> OldFiles {
    let mut found = OldFiles::default();
    for file in files {
        match judge(file, now, threshold) {
            Verdict::Forgotten(untouched) => found.forgotten.push(ForgottenFile {
                file: file.clone(),
                untouched,
            }),
            Verdict::InUse => {}
            Verdict::JustArrived => found.just_arrived += 1,
            Verdict::NoDate => found.no_date += 1,
            Verdict::FutureDate => found.future_date += 1,
        }
    }

    // Hardlinks are one file: keep its first path (by path, so it's stable),
    // or the total would promise space that can't come back
    found
        .forgotten
        .sort_by(|a, b| a.file.path.cmp(&b.file.path));
    let mut seen = HashSet::new();
    found
        .forgotten
        .retain(|f| FileId::of(&f.file.path).is_none_or(|id| seen.insert(id)));

    found.forgotten.sort_by_cached_key(|f| {
        (
            Reverse(f.file.size_bytes),
            Reverse(f.untouched),
            f.file.path.clone(),
        )
    });
    found.total = Usage {
        bytes: found.forgotten.iter().map(|f| f.file.size_bytes).sum(),
        files: found.forgotten.len(),
    };
    found
}

#[cfg(test)]
#[path = "../tests/core_old_files.rs"]
mod tests;
