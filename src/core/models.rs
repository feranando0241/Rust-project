//! The data types shared by every part of `core`: a scanned file and the
//! identity of the data it points to.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Represents the essential information about a file on disk.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// Full path to the file in the file system.
    pub path: PathBuf,
    /// Exact file size in bytes.
    pub size_bytes: u64,
    /// File extension in lowercase (e.g. "pdf", "png"), or `None` if there is none.
    pub extension: Option<String>,
    /// When the content last changed, if the system reports it.
    pub modified: Option<SystemTime>,
    /// When the file was last read, if the system reports it. Less reliable
    /// than `modified`: some volumes don't update it on every read, and
    /// reading a file (for example to hash it) can update it.
    pub accessed: Option<SystemTime>,
}

impl FileInfo {
    /// Builds a `FileInfo` without timestamps, normalizing the extension to
    /// lowercase. Use `from_metadata` when the file's metadata is at hand.
    pub fn from_path(path: PathBuf, size_bytes: u64) -> FileInfo {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase());
        FileInfo {
            path,
            size_bytes,
            extension,
            modified: None,
            accessed: None,
        }
    }

    /// Builds a `FileInfo` with its size and timestamps from `metadata`.
    pub fn from_metadata(path: PathBuf, metadata: &fs::Metadata) -> FileInfo {
        FileInfo {
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
            ..FileInfo::from_path(path, metadata.len())
        }
    }

    /// Returns the file name (e.g. "document.pdf") as a string slice.
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    /// Returns the extension in lowercase, or a provided default if there is none.
    pub fn extension_or<'a>(&'a self, default: &'a str) -> &'a str {
        self.extension.as_deref().unwrap_or(default)
    }
}

/// Where a file's data lives on disk (device + inode). Every path to the same
/// data (hardlinks) has the same `FileId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId {
    pub dev: u64,
    pub ino: u64,
}

impl FileId {
    /// The identity of the file at `path`, or `None` if it can't be read or
    /// the platform has no stable identity.
    #[cfg(unix)]
    pub fn of(path: &Path) -> Option<FileId> {
        fs::metadata(path).ok().map(|m| FileId::from_metadata(&m))
    }

    // ponytail: no file identity off Unix (Windows file IDs need an unstable API),
    // so hardlinks aren't detected and the hash cache is off there.
    #[cfg(not(unix))]
    pub fn of(_path: &Path) -> Option<FileId> {
        None
    }

    /// The identity from metadata already read (avoids a second `stat`).
    #[cfg(unix)]
    pub fn from_metadata(m: &fs::Metadata) -> FileId {
        use std::os::unix::fs::MetadataExt;
        FileId {
            dev: m.dev(),
            ino: m.ino(),
        }
    }
}
