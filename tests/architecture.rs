//! Structure rules, checked on every `cargo test` (and so in CI).
//!
//! They keep the layout from docs/adr/0002-modular-architecture.md from
//! eroding one convenient shortcut at a time. If one fails, the message says
//! what to move where.

use std::fs;
use std::path::{Path, PathBuf};

/// Source files longer than this should be split by responsibility.
const MAX_LINES_PER_FILE: usize = 500;

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    files
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// `(file, line number, line)` for every line in `files` containing a pattern.
fn find(files: &[PathBuf], patterns: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    for file in files {
        let text = fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(""); // ignore comments
            if patterns.iter().any(|p| code.contains(p)) {
                hits.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }
    hits
}

#[test]
fn core_never_depends_on_ui_or_app() {
    let hits = find(
        &rust_files(&src().join("core")),
        &["crate::ui", "crate::app", "crate::cli"],
    );
    assert!(
        hits.is_empty(),
        "core/ must not import ui, app or cli. Return data from core and show it from ui/ or app.rs:\n{}",
        hits.join("\n")
    );
}

#[test]
fn core_never_prints_or_reads_input() {
    let hits = find(
        &rust_files(&src().join("core")),
        &[
            "println!",
            "print!(",
            "eprintln!",
            "eprint!(",
            "dbg!",
            "stdin()",
        ],
    );
    assert!(
        hits.is_empty(),
        "core/ must not print or read input. Return the data (or a typed error) and let ui/ show it:\n{}",
        hits.join("\n")
    );
}

#[test]
fn source_files_stay_small() {
    let too_long: Vec<String> = rust_files(&src())
        .into_iter()
        .filter_map(|file| {
            let lines = fs::read_to_string(&file).unwrap().lines().count();
            (lines > MAX_LINES_PER_FILE).then(|| format!("{} ({} lines)", file.display(), lines))
        })
        .collect();
    assert!(
        too_long.is_empty(),
        "these files are over {} lines; split them by responsibility (see ADR-0002):\n{}",
        MAX_LINES_PER_FILE,
        too_long.join("\n")
    );
}
