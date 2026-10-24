//! Property tests for duplicate detection and removal.
//!
//! Instead of hand-picked examples, each case builds a random folder (exact
//! copies, same size and same 4 KB prefix but a different tail, hardlinks,
//! empty files, subfolders, sizes around the 4 KB boundary) and checks the
//! program against a brute-force answer: read every file and group by bytes.
//!
//! Hand-rolled with a tiny seeded PRNG instead of the `proptest` crate, so the
//! project keeps its three dependencies. Every case is reproducible: a failure
//! prints its seed. Raise `CASES` locally to search harder.

use crate::core::duplicates::hash_cache::HashCache;
use crate::core::duplicates::trash::remove_duplicates;
use crate::core::duplicates::{DuplicateGroup, find_exact_duplicates};
use crate::core::models::FileId;
use crate::core::scanner::scan_directory;
use crate::core::test_support::{fake_trash, no_progress, temp_dir};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Random folders checked per test run.
const CASES: u64 = 150;

/// xorshift64*: small, fast, and deterministic for a given seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

/// Fills `dir` with a random mix of files designed to reach every branch.
fn random_folder(rng: &mut Rng, dir: &Path) {
    // Sizes around the 4 KB prefix boundary, plus empty and tiny files
    const SIZES: [usize; 6] = [0, 1, 100, 4096, 4097, 9000];
    fs::create_dir_all(dir.join("sub")).unwrap();

    let templates: Vec<Vec<u8>> = (0..1 + rng.below(4))
        .map(|_| {
            let size = SIZES[rng.below(SIZES.len())];
            rng.bytes(size)
        })
        .collect();
    let mut written: Vec<PathBuf> = Vec::new();

    for i in 0..2 + rng.below(12) {
        let folder = if rng.below(3) == 0 {
            dir.join("sub")
        } else {
            dir.to_path_buf()
        };
        let path = folder.join(format!("f{}.bin", i));

        if rng.below(5) == 0 && !written.is_empty() {
            // A hardlink: a second path to an existing file's data
            let target = &written[rng.below(written.len())];
            fs::hard_link(target, &path).unwrap();
        } else {
            let mut content = templates[rng.below(templates.len())].clone();
            match rng.below(3) {
                // Same size, one byte different anywhere
                0 if !content.is_empty() => {
                    let at = rng.below(content.len());
                    content[at] ^= 0xFF;
                }
                // Same size and same 4 KB prefix, different tail
                1 if content.len() > 4096 => {
                    let at = 4096 + rng.below(content.len() - 4096);
                    content[at] ^= 0xFF;
                }
                // Exact copy of the template
                _ => {}
            }
            fs::write(&path, content).unwrap();
        }
        written.push(path);
    }
}

/// Every non-empty regular file under `dir` (skipping the fake Trash), read
/// independently of the scanner: `path → (identity, bytes)`.
fn read_all(dir: &Path) -> BTreeMap<PathBuf, (FileId, Vec<u8>)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if entry.file_name() == ".fake_trash" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = fs::read(&path).unwrap();
                if !bytes.is_empty() {
                    out.insert(path.clone(), (FileId::of(&path).unwrap(), bytes));
                }
            }
        }
    }
    out
}

/// The correct answer by brute force: distinct underlying files (inodes) with
/// exactly the same bytes, as a set of groups.
fn expected_groups(dir: &Path) -> BTreeSet<BTreeSet<FileId>> {
    let mut by_content: HashMap<Vec<u8>, BTreeSet<FileId>> = HashMap::new();
    for (id, bytes) in read_all(dir).into_values() {
        by_content.entry(bytes).or_default().insert(id);
    }
    by_content
        .into_values()
        .filter(|ids| ids.len() > 1)
        .collect()
}

/// What the program found, in the same shape as `expected_groups`.
fn found_groups(groups: &[DuplicateGroup]) -> BTreeSet<BTreeSet<FileId>> {
    groups
        .iter()
        .map(|g| {
            g.copies
                .iter()
                .flat_map(|c| &c.paths)
                .map(|f| FileId::of(&f.path).unwrap())
                .collect()
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn prop_detection_matches_brute_force_with_and_without_cache() {
    for seed in 1..=CASES {
        let dir = temp_dir(&format!("prop_detect_{}", seed));
        random_folder(&mut Rng::new(seed), &dir);
        let files = scan_directory(&dir, &no_progress).unwrap();
        let expected = expected_groups(&dir);

        let no_cache = found_groups(&find_exact_duplicates(
            &files,
            &HashCache::disabled(),
            &no_progress,
        ));

        // Cold cache, saved to disk, then a warm cache loaded back from disk
        let cache_file = dir.with_extension("cache.tsv");
        let cold = HashCache::load(&cache_file);
        let cold_found = found_groups(&find_exact_duplicates(&files, &cold, &no_progress));
        cold.save(&cache_file).unwrap();
        let warm = HashCache::load(&cache_file);
        let warm_found = found_groups(&find_exact_duplicates(&files, &warm, &no_progress));

        fs::remove_dir_all(&dir).unwrap();
        let _ = fs::remove_file(&cache_file);

        assert_eq!(
            no_cache, expected,
            "seed {}: wrong groups without cache",
            seed
        );
        assert_eq!(
            cold_found, expected,
            "seed {}: wrong groups with a cold cache",
            seed
        );
        assert_eq!(
            warm_found, expected,
            "seed {}: wrong groups with a warm cache",
            seed
        );
    }
}

#[cfg(unix)]
#[test]
fn prop_removal_never_loses_content() {
    for seed in 1..=CASES {
        let dir = temp_dir(&format!("prop_remove_{}", seed));
        let mut rng = Rng::new(seed);
        random_folder(&mut rng, &dir);
        let before = read_all(&dir);
        let contents_before: BTreeSet<&Vec<u8>> = before.values().map(|(_, bytes)| bytes).collect();

        let files = scan_directory(&dir, &no_progress).unwrap();
        let mut groups = find_exact_duplicates(&files, &HashCache::disabled(), &no_progress);
        // The user may choose to keep any copies (e.g. with --keep-in): the
        // guarantees below must hold whichever ones they are.
        for group in &mut groups {
            let wanted: Vec<bool> = (0..group.copies.len()).map(|_| rng.below(2) == 0).collect();
            group.keep_where(|i, _| wanted[i]);
        }
        // Groups that keep several copies still have exactly those as duplicates
        let should_remain: BTreeSet<BTreeSet<FileId>> = groups
            .iter()
            .filter(|g| g.copies.len() - g.extra_copies() > 1)
            .map(|g| {
                g.copies
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| g.is_kept(*i))
                    .flat_map(|(_, c)| &c.paths)
                    .map(|f| FileId::of(&f.path).unwrap())
                    .collect()
            })
            .collect();
        let copies: usize = groups.iter().map(|g| g.paths_to_remove().len()).sum();
        let promised_bytes: u64 = groups.iter().map(|g| g.wasted_bytes()).sum();
        let report = remove_duplicates(&groups, &no_progress, &fake_trash(&dir));

        let after = read_all(&dir);
        let contents_after: BTreeSet<&Vec<u8>> = after.values().map(|(_, bytes)| bytes).collect();
        let originals_kept = groups.iter().all(|g| {
            g.copies
                .iter()
                .enumerate()
                .filter(|(i, _)| g.is_kept(*i))
                .all(|(_, c)| c.paths.iter().all(|p| p.path.exists()))
        });
        let duplicates_left = expected_groups(&dir);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            duplicates_left, should_remain,
            "seed {}: only the copies chosen to keep may remain as duplicates",
            seed
        );

        assert_eq!(
            contents_after, contents_before,
            "seed {}: some content was lost",
            seed
        );
        assert!(originals_kept, "seed {}: a kept copy was removed", seed);
        assert_eq!(
            report.moved, copies,
            "seed {}: not every verified copy was moved",
            seed
        );
        assert_eq!(
            report.freed_bytes, promised_bytes,
            "seed {}: freed space differs from what was shown",
            seed
        );
        assert!(
            report.failed.is_empty(),
            "seed {}: unexpected failures {:?}",
            seed,
            report.failed
        );
    }
}
