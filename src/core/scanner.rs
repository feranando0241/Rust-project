//! Recursive directory scanning and the rules for which folders and files to skip.

use crate::core::Progress;
use crate::core::models::FileInfo;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// List of technical, dependency, and system folders ignored by default.
///
/// These folders generate a lot of "noise" because they contain hundreds of
/// library files, build artifacts, or OS files that are not relevant to the user.
pub const IGNORED_DIRECTORIES: &[&str] = &[
    // Version control
    ".git",
    // Rust build output
    "target",
    // JavaScript/Node dependencies
    "node_modules",
    // IDE and editor cache / config
    ".cache",
    ".vscode",
    ".idea",
    // System Trash
    ".Trash",
    // Python virtual environments (most common exact names)
    "venv",
    ".venv",
    "env",
    ".env",
    "__pycache__",
    // macOS system library
    "Library",
    // Build output folders for web / Java / Python projects
    "dist",
    "build",
    ".next",
    ".nuxt",
    "out",
    // Python and Ruby package managers
    "site-packages",
    ".gem",
    // Tool cache folders
    ".npm",
    ".yarn",
    ".pnpm-store",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
];

/// List of hidden, system, or binary files ignored during scanning.
pub const IGNORED_FILES: &[&str] = &[
    // macOS metadata files
    ".DS_Store",
    ".localized",
    // Windows metadata files
    "Thumbs.db",
    "desktop.ini",
    // Dependency lock files (auto-generated)
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Pipfile.lock",
    "poetry.lock",
];

/// Compiled or binary file extensions to ignore (without the leading dot).
const IGNORED_EXTENSIONS: &[&str] = &[
    "pyc",   // Python compiled bytecode
    "pyo",   // Python optimized bytecode
    "class", // Java compiled bytecode
    "o",     // C/C++ compiled object file
];

/// Determines whether an individual file should be ignored.
///
/// Ignores files in the exact-match list and those with bytecode/binary extensions.
/// Note: `rsplit('.')` returns the file name itself when there is no dot, so
/// single-extension-less files are safely handled without false positives.
pub fn should_ignore_file(file_name: &str) -> bool {
    if IGNORED_FILES.contains(&file_name) {
        return true;
    }
    // Ignore by extension (bytecode and compiled files)
    if let Some(ext) = file_name.rsplit('.').next()
        && IGNORED_EXTENSIONS.contains(&ext)
    {
        return true;
    }
    false
}

/// Determines whether a directory name should be ignored during scanning.
///
/// Ignores directories in the explicit list, all hidden folders (`.something`),
/// those ending in `_venv` or `-env` (Python virtual environment variants),
/// and macOS application bundles (`.app`, `.framework`, `.bundle`).
pub fn should_ignore_directory(dir_name: &str) -> bool {
    if IGNORED_DIRECTORIES.contains(&dir_name) {
        return true;
    }
    // Hidden system folders (start with a dot), except "." which is the current directory
    if dir_name.starts_with('.') && dir_name != "." {
        return true;
    }
    // Python virtual environment variants: project_venv, my-env, report_venv, etc.
    if dir_name.ends_with("_venv") || dir_name.ends_with("-venv") || dir_name.ends_with("-env") {
        return true;
    }
    // macOS application bundles (.app, .framework, .bundle)
    if dir_name.ends_with(".app")
        || dir_name.ends_with(".framework")
        || dir_name.ends_with(".bundle")
    {
        return true;
    }
    false
}

/// Scans a directory and lists only its immediately accessible subdirectories.
pub fn list_accessible_subdirectories(dir_path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let entries = fs::read_dir(dir_path)?;

    // Unreadable entries are skipped instead of failing the whole listing
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            let file_name = entry.file_name();
            let name_str = file_name.to_str().unwrap_or("");
            if !should_ignore_directory(name_str) {
                dirs.push(entry.path());
            }
        }
    }

    dirs.sort();
    Ok(dirs)
}

/// Scans a directory and all its subfolders recursively.
/// Returns a list of all files found. `on_progress` gets the running count
/// of files found (the total is unknown during a scan, so it is always 0).
pub fn scan_directory(dir_path: &Path, on_progress: Progress) -> io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    scan_recursive(dir_path, &mut files, on_progress)?;
    Ok(files)
}

/// Recursive helper that traverses the folder hierarchy.
fn scan_recursive(
    current_path: &Path,
    files: &mut Vec<FileInfo>,
    on_progress: Progress,
) -> io::Result<()> {
    let entries = match fs::read_dir(current_path) {
        Ok(read_dir) => read_dir,
        Err(_) => return Ok(()), // No read permission — skip silently
    };

    // Entries that vanish or can't be inspected mid-scan are skipped silently
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_str().unwrap_or("");

        if file_type.is_dir() {
            if should_ignore_directory(name_str) {
                continue;
            }
            scan_recursive(&path, files, on_progress)?;
        } else if file_type.is_file() {
            if should_ignore_file(name_str) {
                continue;
            }

            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            files.push(FileInfo::from_path(path, metadata.len()));
            on_progress(files.len(), 0);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore_directory() {
        assert!(should_ignore_directory("node_modules"));
        assert!(should_ignore_directory("target"));
        assert!(should_ignore_directory(".git"));
        assert!(should_ignore_directory(".cache"));
        assert!(!should_ignore_directory("photos"));
        assert!(!should_ignore_directory("documents"));
    }
}
