//! Core logic: scanning, duplicate detection, file queries and document
//! similarity.
//!
//! Rule: nothing in `core` prints, reads the keyboard or imports `ui`.
//! Functions return data (or `Err` with a message) and the caller decides
//! how to show it. That keeps this layer testable and lets the terminal UI
//! change without touching the logic that can delete files.

pub mod duplicates;
pub mod files;
pub mod models;
pub mod scanner;
pub mod similarity;

/// Progress callback for long operations, called as `(done, total)`.
/// `total` is 0 when it isn't known yet (e.g. while scanning). This is how
/// `core` reports progress without printing: the UI decides how to show it.
pub type Progress<'a> = &'a dyn Fn(usize, usize);
