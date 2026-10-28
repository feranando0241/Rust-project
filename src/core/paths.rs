//! Making paths given by the user comparable with the paths the scanner
//! builds. Matching a folder against scanned files decides what gets excluded
//! (`--exclude`) and what is protected from the Trash (`--keep-in`), so it
//! lives in core and goes through the disk rather than comparing spellings.

use std::path::{Path, PathBuf};

/// Expresses `path` the way the scanner builds paths for `root` (root +
/// relative part), so it can be compared with scanned files. A path outside
/// `root`, or one that can't be resolved, is returned unchanged.
pub fn in_scan_form(root: &Path, path: PathBuf) -> PathBuf {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(abs), Ok(root_abs)) => match abs.strip_prefix(&root_abs) {
            Ok(rel) => root.join(rel),
            Err(_) => abs,
        },
        _ => path,
    }
}

/// A folder relative to `root` (or absolute), in scan form. It is resolved on
/// disk, so `Photos/../Photos`, a symlink to a folder inside the scan, or
/// another spelling of `root` all match the scanned files. A folder that
/// doesn't exist comes back as `root` + `folder`; callers check `is_dir`.
pub fn folder_in_scan(root: &Path, folder: &Path) -> PathBuf {
    let path = if folder.is_absolute() {
        folder.to_path_buf()
    } else {
        root.join(folder)
    };
    in_scan_form(root, path)
}

#[cfg(test)]
#[path = "../tests/core_paths.rs"]
mod tests;
