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
