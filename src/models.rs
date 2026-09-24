use std::path::PathBuf;

/// Represents the essential information about a file on disk.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// Full path to the file in the file system.
    pub path: PathBuf,
    /// Exact file size in bytes.
    pub size_bytes: u64,
    /// File extension (e.g. "pdf", "png"), or `None` if there is none.
    pub extension: Option<String>,
}

impl FileInfo {
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
