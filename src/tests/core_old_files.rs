//! Unit tests for `src/core/old_files.rs`, loaded there as its `tests` module.

use super::*;
use crate::core::test_support::Rng;
use std::path::PathBuf;

const DAY: Duration = Duration::from_secs(86_400);
const THRESHOLD: Duration = Duration::from_secs(180 * 86_400);

/// A fixed "now", so every test sees the same moment.
fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + DAY * 20_000
}

/// `n` days before `now()`; negative means in the future.
fn days_ago(n: i64) -> SystemTime {
    if n >= 0 {
        now() - DAY * n as u32
    } else {
        now() + DAY * (-n) as u32
    }
}

/// A file with its timestamps given in days before `now()`.
fn file(
    path: &str,
    size: u64,
    modified: Option<i64>,
    accessed: Option<i64>,
    changed: Option<i64>,
) -> FileInfo {
    FileInfo {
        modified: modified.map(days_ago),
        accessed: accessed.map(days_ago),
        changed: changed.map(days_ago),
        ..FileInfo::from_path(PathBuf::from(path), size)
    }
}

fn verdict(modified: Option<i64>, accessed: Option<i64>, changed: Option<i64>) -> Verdict {
    judge(&file("f", 1, modified, accessed, changed), now(), THRESHOLD)
}

#[test]
fn test_forgotten_needs_no_change_and_no_read() {
    assert_eq!(
        verdict(Some(200), Some(190), Some(400)),
        Verdict::Forgotten(DAY * 190),
        "untouched since the most recent of change and read"
    );
    assert_eq!(
        verdict(Some(200), Some(10), Some(400)),
        Verdict::InUse,
        "read recently"
    );
    assert_eq!(
        verdict(Some(10), Some(300), Some(400)),
        Verdict::InUse,
        "changed recently"
    );
    assert_eq!(
        verdict(Some(200), None, Some(400)),
        Verdict::Forgotten(DAY * 200),
        "no atime: judged on mtime"
    );
}

#[test]
fn test_threshold_is_strict() {
    assert_eq!(
        verdict(Some(180), None, None),
        Verdict::InUse,
        "exactly N days"
    );
    assert_eq!(
        verdict(Some(181), None, None),
        Verdict::Forgotten(DAY * 181)
    );
}

#[test]
fn test_recent_arrivals_are_protected_for_a_month() {
    // An archive unpacked recently keeps its old mtime; its ctime is new
    assert_eq!(verdict(Some(400), None, Some(10)), Verdict::JustArrived);
    assert_eq!(verdict(Some(400), None, Some(29)), Verdict::JustArrived);
    assert_eq!(
        verdict(Some(400), None, Some(30)),
        Verdict::Forgotten(DAY * 400),
        "the grace ends after 30 days"
    );
    assert_eq!(
        verdict(Some(400), None, None),
        Verdict::Forgotten(DAY * 400),
        "no ctime (only off Unix, without a creation time): no grace"
    );
    // The grace only matters for files that would be forgotten
    assert_eq!(verdict(Some(5), None, Some(5)), Verdict::InUse);
}

#[test]
fn test_files_that_cant_be_judged() {
    assert_eq!(verdict(None, Some(400), Some(400)), Verdict::NoDate);
    assert_eq!(verdict(Some(-1), None, None), Verdict::FutureDate);
    assert_eq!(verdict(Some(400), Some(-1), None), Verdict::FutureDate);
    assert_eq!(verdict(Some(400), None, Some(-1)), Verdict::FutureDate);
}

#[test]
fn test_days_rejects_values_too_large() {
    assert_eq!(days(1), Some(DAY));
    assert_eq!(days(0), None, "0 would make every file forgotten");
    assert_eq!(days(u64::MAX), None);
}

#[test]
fn test_find_old_files_sorts_and_counts() {
    let files = vec![
        file("small", 10, Some(300), None, None),
        file("big_newer", 500, Some(200), None, None),
        file("big_older", 500, Some(900), None, None),
        file("in_use", 9_999, Some(1), None, None),
        file("arrived", 9_999, Some(900), None, Some(3)),
        file("no_date", 9_999, None, None, None),
        file("future", 9_999, Some(-3), None, None),
    ];
    let found = find_old_files(&files, now(), THRESHOLD);

    let order: Vec<&str> = found.forgotten.iter().map(|f| f.file.name()).collect();
    assert_eq!(
        order,
        vec!["big_older", "big_newer", "small"],
        "biggest first, then longest untouched"
    );
    assert_eq!(
        found.total,
        Usage {
            bytes: 1010,
            files: 3
        }
    );
    assert_eq!(
        (found.just_arrived, found.no_date, found.future_date),
        (1, 1, 1)
    );
}

#[cfg(unix)]
#[test]
fn test_hardlinks_count_once() {
    use crate::core::test_support::temp_dir;
    use std::fs;

    let dir = temp_dir("old_hardlinks");
    fs::write(dir.join("a.bin"), vec![0u8; 100]).unwrap();
    fs::hard_link(dir.join("a.bin"), dir.join("b.bin")).unwrap();
    fs::write(dir.join("c.bin"), vec![0u8; 50]).unwrap();
    let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
    // Real paths (for their identity on disk), made-up dates
    let files: Vec<FileInfo> = ["b.bin", "a.bin", "c.bin"]
        .iter()
        .map(|name| file(&path(name), 100, Some(400), None, None))
        .collect();
    let found = find_old_files(&files, now(), THRESHOLD);
    fs::remove_dir_all(&dir).unwrap();

    let names: Vec<&str> = found.forgotten.iter().map(|f| f.file.name()).collect();
    assert_eq!(
        names,
        vec!["a.bin", "c.bin"],
        "a.bin and b.bin are one file"
    );
    assert_eq!(found.total.bytes, 200);
}

/// The rules of ADR-0004 written as plainly as possible, independently of
/// `judge`: compares instants instead of ages.
fn spec(f: &FileInfo, now: SystemTime) -> Verdict {
    let Some(modified) = f.modified else {
        return Verdict::NoDate;
    };
    let stamps: Vec<SystemTime> = [f.modified, f.accessed, f.changed]
        .into_iter()
        .flatten()
        .collect();
    if stamps.iter().any(|&t| t > now) {
        return Verdict::FutureDate;
    }
    let last_touch = f.accessed.map_or(modified, |read| read.max(modified));
    if last_touch >= now - THRESHOLD {
        return Verdict::InUse;
    }
    if f.changed.is_some_and(|c| c > now - DAY * 30) {
        return Verdict::JustArrived;
    }
    Verdict::Forgotten(now.duration_since(last_touch).unwrap())
}

#[test]
fn prop_old_files_match_the_spec_and_account_for_every_file() {
    for seed in 1..=200 {
        let mut rng = Rng::new(seed);
        // Instants from 5 days in the future to 400 days ago, to the second,
        // and sometimes missing
        let stamp = |rng: &mut Rng| {
            (rng.below(5) != 0).then(|| {
                let secs = rng.below(405 * 86_400) as u64;
                now() + DAY * 5 - Duration::from_secs(secs)
            })
        };
        let files: Vec<FileInfo> = (0..60)
            .map(|i| FileInfo {
                modified: stamp(&mut rng),
                accessed: stamp(&mut rng),
                changed: stamp(&mut rng),
                ..FileInfo::from_path(PathBuf::from(format!("f{}", i)), rng.below(1000) as u64)
            })
            .collect();

        let found = find_old_files(&files, now(), THRESHOLD);

        for f in &files {
            assert_eq!(
                judge(f, now(), THRESHOLD),
                spec(f, now()),
                "seed {}: {:?}",
                seed,
                f
            );
        }
        let in_use = files
            .iter()
            .filter(|f| spec(f, now()) == Verdict::InUse)
            .count();
        assert_eq!(
            found.forgotten.len() + in_use + found.just_arrived + found.no_date + found.future_date,
            files.len(),
            "seed {}: every file lands in exactly one group",
            seed
        );
        let expected_bytes: u64 = files
            .iter()
            .filter(|f| matches!(spec(f, now()), Verdict::Forgotten(_)))
            .map(|f| f.size_bytes)
            .sum();
        assert_eq!(found.total.bytes, expected_bytes, "seed {}", seed);
        assert!(
            found
                .forgotten
                .windows(2)
                .all(|w| w[0].file.size_bytes >= w[1].file.size_bytes),
            "seed {}: biggest first",
            seed
        );
    }
}
