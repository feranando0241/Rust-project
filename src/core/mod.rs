//! Core logic: scanning, duplicate detection, file queries and document
//! similarity.
//!
//! Rule: nothing in `core` prints, reads the keyboard or imports `ui` or
//! `app` (CI checks this). Functions return data (or a typed error) and the
//! caller decides how to show it. That keeps this layer testable and lets the
//! terminal UI change without touching the logic that can delete files.
//!
//! - `models`      FileInfo and FileId, shared by everything below
//! - `scanner`     directory walk and ignore rules
//! - `files`       simple queries: biggest, by type, same name, space used
//! - `hashing`     full and 4 KB prefix SHA-256, parallel hashing
//! - `hash_cache`  hashes saved between runs
//! - `duplicates`  finding identical files
//! - `trash`       moving extra copies to the Trash, re-verified from disk
//! - `similarity`  comparing documents by content

pub mod duplicates;
pub mod files;
pub mod hash_cache;
pub mod hashing;
pub mod models;
pub mod scanner;
pub mod similarity;
pub mod trash;

#[cfg(all(test, unix))]
mod duplicates_props;
#[cfg(test)]
mod test_support;

/// Progress callback for long operations, called as `(done, total)`.
/// `total` is 0 when it isn't known yet (e.g. while scanning). This is how
/// `core` reports progress without printing: the UI decides how to show it.
/// `Sync` because parallel work reports from several threads.
pub type Progress<'a> = &'a (dyn Fn(usize, usize) + Sync);
