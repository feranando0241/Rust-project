//! Terminal styling: colors, titles, prompts and small text helpers shared by
//! every screen.

use crate::core::models::FileInfo;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::OnceLock;

/// Width of horizontal rules.
const WIDTH: usize = 72;

/// Colors are used only on a real terminal and when `NO_COLOR` is not set.
fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

/// Wraps text in an ANSI style code. Pad text *before* styling it,
/// otherwise the escape codes break column alignment.
fn paint(code: &str, text: &str) -> String {
    if color_enabled() {
        format!("\x1b[{}m{}\x1b[0m", code, text)
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    paint("1", text)
}
pub fn dim(text: &str) -> String {
    paint("2", text)
}
pub fn red(text: &str) -> String {
    paint("31", text)
}
pub fn green(text: &str) -> String {
    paint("32", text)
}
pub fn yellow(text: &str) -> String {
    paint("33", text)
}

/// Prints a bold section title followed by a rule.
pub fn header(title: &str) {
    println!("\n{}", bold(title));
    rule();
}

pub fn rule() {
    println!("{}", dim(&"─".repeat(WIDTH)));
}

/// Prints the input marker; the answer is typed on the same line.
pub fn prompt() {
    print!("{} ", bold("›"));
}

/// Prints a secondary, explanatory line.
pub fn hint(text: &str) {
    println!("{}", dim(text));
}

/// Prints a problem the user should notice.
pub fn error(text: &str) {
    eprintln!("{} {}", red("✗"), text);
}

/// "1 file" / "3 files".
pub fn plural(n: usize, singular: &str, plural: &str) -> String {
    format!("{} {}", n, if n == 1 { singular } else { plural })
}

/// Truncates a string to the given maximum character count, appending '…' if cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut)
    }
}

/// Shows a path relative to the scanned root (e.g. "./photos/2024").
pub fn relative(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) if rel.as_os_str().is_empty() => "(this folder)".to_string(),
        Ok(rel) => format!("./{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Relative folder that contains a file.
pub fn folder_of(file: &FileInfo, root: &Path) -> String {
    file.path
        .parent()
        .map(|p| relative(p, root))
        .unwrap_or_else(|| ".".to_string())
}

/// A proportional bar like `██████░░░░`.
pub fn bar(part: f64, total: f64, width: usize) -> String {
    let filled = if total > 0.0 {
        ((part / total) * width as f64).round() as usize
    } else {
        0
    }
    .min(width);
    format!("{}{}", "█".repeat(filled), dim(&"░".repeat(width - filled)))
}

/// Converts a byte count to a human-readable string (B, KB, MB, GB).
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    match bytes {
        b if b >= GB => format!("{:.2} GB", b as f64 / GB as f64),
        b if b >= MB => format!("{:.2} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.2} KB", b as f64 / KB as f64),
        b => format!("{} B", b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_sizes() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_size(10 * 1024 * 1024 * 1024), "10.00 GB");
    }
}
