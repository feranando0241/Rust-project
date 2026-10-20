use std::collections::{BTreeMap, HashMap};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use crate::actions::{DuplicateGroup, SameNameGroup};
use crate::models::FileInfo;
use crate::similarity::SimilarityMatch;

/// Width of horizontal rules.
const WIDTH: usize = 72;
/// Max characters shown for a file name in table columns.
const NAME_W: usize = 34;
/// How many rows the scan summary shows per section.
const SUMMARY_ROWS: usize = 5;

// ─────────────────────────────────────────────────────────────
// STYLING HELPERS
// ─────────────────────────────────────────────────────────────

/// Colors are used only on a real terminal and when `NO_COLOR` is not set.
fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
    })
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

pub fn bold(text: &str) -> String { paint("1", text) }
pub fn dim(text: &str) -> String { paint("2", text) }
fn red(text: &str) -> String { paint("31", text) }
fn green(text: &str) -> String { paint("32", text) }
fn yellow(text: &str) -> String { paint("33", text) }

/// Prints a bold section title followed by a rule.
fn header(title: &str) {
    println!("\n{}", bold(title));
    rule();
}

fn rule() {
    println!("{}", dim(&"─".repeat(WIDTH)));
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
fn relative(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => format!("./{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Relative folder that contains a file.
fn folder_of(file: &FileInfo, root: &Path) -> String {
    file.path
        .parent()
        .map(|p| relative(p, root))
        .unwrap_or_else(|| ".".to_string())
}

/// A proportional bar like `██████░░░░`.
fn bar(part: f64, total: f64, width: usize) -> String {
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

// ─────────────────────────────────────────────────────────────
// SCAN OVERVIEW
// ─────────────────────────────────────────────────────────────

/// Short overview shown right after a scan: totals, heaviest folders and
/// heaviest file types. The full tree is available from the action menu.
pub fn print_summary(root: &Path, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    let folder_count = group_by_folder(files).len();

    header(&format!("Scan complete: {}", root.display()));
    println!(
        "  {} files · {} folders · {}",
        bold(&files.len().to_string()),
        folder_count,
        bold(&format_size(total_size))
    );

    // Size per top-level subfolder (recursive), plus loose files in the root
    let mut by_folder: HashMap<String, (u64, usize)> = HashMap::new();
    for file in files {
        let mut parts = file.path.strip_prefix(root).unwrap_or(&file.path).components();
        let first = parts.next();
        let key = match (first, parts.next()) {
            (Some(dir), Some(_)) => format!("./{}", dir.as_os_str().to_string_lossy()),
            _ => "(files in root)".to_string(),
        };
        let entry = by_folder.entry(key).or_default();
        entry.0 += file.size_bytes;
        entry.1 += 1;
    }

    // Size per extension
    let mut by_type: HashMap<String, (u64, usize)> = HashMap::new();
    for file in files {
        let entry = by_type.entry(file.extension_or("no ext").to_string()).or_default();
        entry.0 += file.size_bytes;
        entry.1 += 1;
    }

    print_ranking("Largest folders", by_folder, total_size);
    print_ranking("Largest file types", by_type, total_size);
    println!();
}

/// Prints the top entries of a (name → size, count) map with share-of-total bars.
fn print_ranking(title: &str, map: HashMap<String, (u64, usize)>, total_size: u64) {
    let mut rows: Vec<_> = map.into_iter().collect();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    println!("\n  {}", bold(title));
    for (name, (size, count)) in rows.iter().take(SUMMARY_ROWS) {
        println!(
            "  {:<30} {:>10}  {}  {}",
            truncate(name, 30),
            format_size(*size),
            bar(*size as f64, total_size as f64, 12),
            dim(&format!("{} files", count))
        );
    }
    if rows.len() > SUMMARY_ROWS {
        println!("  {}", dim(&format!("… and {} more", rows.len() - SUMMARY_ROWS)));
    }
}

// ─────────────────────────────────────────────────────────────
// FILE LISTINGS
// ─────────────────────────────────────────────────────────────

/// Groups a list of files by their containing folder.
pub fn group_by_folder(files: &[FileInfo]) -> BTreeMap<PathBuf, Vec<FileInfo>> {
    let mut grouped: BTreeMap<PathBuf, Vec<FileInfo>> = BTreeMap::new();

    for file in files {
        let parent = file
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        grouped.entry(parent).or_default().push(file.clone());
    }

    for file_list in grouped.values_mut() {
        file_list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    }

    grouped
}

/// Displays the scan results organized as a visual tree.
pub fn print_tree(root: &Path, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    let grouped = group_by_folder(files);

    header(&format!("File tree: {}", root.display()));

    for (folder, folder_files) in &grouped {
        let folder_size: u64 = folder_files.iter().map(|f| f.size_bytes).sum();

        println!(
            "\n{}  {}",
            bold(&relative(folder, root)),
            dim(&format!("{} files · {}", folder_files.len(), format_size(folder_size)))
        );

        for (index, file) in folder_files.iter().enumerate() {
            let branch = if index == folder_files.len() - 1 { "└─" } else { "├─" };
            println!(
                "  {} {:<NAME_W$} {:>10}  {}",
                dim(branch),
                truncate(file.name(), NAME_W),
                format_size(file.size_bytes),
                dim(file.extension_or("no ext"))
            );
        }
    }

    println!();
    rule();
    println!(
        "Total: {} files in {} folders · {}\n",
        files.len(),
        grouped.len(),
        bold(&format_size(total_size))
    );
}

/// Displays the top largest files table.
pub fn print_top_largest(root: &Path, files: &[FileInfo]) {
    header("Top largest files");
    println!("{}", dim(&format!("{:<4} {:<NAME_W$} {:>10}  {}", "#", "NAME", "SIZE", "FOLDER")));

    for (i, file) in files.iter().enumerate() {
        println!(
            "{:<4} {:<NAME_W$} {}  {}",
            i + 1,
            truncate(file.name(), NAME_W),
            bold(&format!("{:>10}", format_size(file.size_bytes))),
            dim(&folder_of(file, root))
        );
    }
    println!();
}

/// Displays the list of files filtered by extension.
pub fn print_filtered_files(root: &Path, ext: &str, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    header(&format!(".{} files ({} found)", ext, files.len()));
    println!("{}", dim(&format!("{:<NAME_W$} {:>10}  {}", "NAME", "SIZE", "FOLDER")));

    for file in files {
        println!(
            "{:<NAME_W$} {:>10}  {}",
            truncate(file.name(), NAME_W),
            format_size(file.size_bytes),
            dim(&folder_of(file, root))
        );
    }
    rule();
    println!("Total space used by .{}: {}\n", ext, bold(&format_size(total_size)));
}

/// Displays exact duplicates verified by hash and the recoverable space.
pub fn print_exact_duplicates(root: &Path, groups: &[DuplicateGroup]) {
    if groups.is_empty() {
        println!("\nNo exact duplicate files found.");
        return;
    }

    let mut total_wasted: u64 = 0;
    let mut total_dupe_files: usize = 0;

    header("Duplicate files (identical SHA-256)");

    for (i, group) in groups.iter().enumerate() {
        let wasted_in_group = group.file_size * group.duplicates.len() as u64;
        total_wasted += wasted_in_group;
        total_dupe_files += group.duplicates.len();

        println!(
            "\n{}  {}",
            bold(&format!("Group {}", i + 1)),
            dim(&format!(
                "{} each · {} wasted",
                format_size(group.file_size),
                format_size(wasted_in_group)
            ))
        );
        println!("  {} {}", green("keep  "), relative(&group.original.path, root));
        for dupe in &group.duplicates {
            println!("  {} {}", red("delete"), relative(&dupe.path, root));
        }
    }

    println!();
    rule();
    println!(
        "{} duplicate file(s) in {} group(s) · {} recoverable\n",
        total_dupe_files,
        groups.len(),
        yellow(&bold(&format_size(total_wasted)))
    );
}

/// Displays the interactive list of accessible directories.
pub fn print_accessible_directories(parent_dir: &Path, dirs: &[PathBuf]) {
    header(&format!("Folders in {}", parent_dir.display()));
    if dirs.is_empty() {
        println!("{}", dim("No subfolders here."));
    } else {
        for (i, dir) in dirs.iter().enumerate() {
            let folder_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("folder");
            println!(" {} {}/", dim(&format!("[{:>2}]", i + 1)), folder_name);
        }
    }
    println!();
}

/// Displays the results of the content similarity comparison.
pub fn print_similarity_results(
    root: &Path,
    target_name: &str,
    matches: &[SimilarityMatch],
    threshold: f64,
) {
    header(&format!("Documents similar to '{}'", target_name));
    println!(
        "{}",
        dim(&format!(
            "Threshold >= {:.1}% · score = 50% vocabulary + 40% phrases + 10% length",
            threshold
        ))
    );

    if matches.is_empty() {
        println!("\nNo documents found with similarity >= {:.1}%.", threshold);
        println!("Suggestions:");
        println!("  • Lower the threshold (e.g. 10% or 20%) to catch weaker similarities.");
        println!("  • Make sure the files contain real text (not scanned images).");
        println!("  • Make sure the folder contains other text/PDF documents.\n");
        return;
    }

    println!();
    for m in matches {
        let pct = format!("{:>5.1}%", m.similarity_percentage);
        let pct = match m.similarity_percentage {
            p if p >= 70.0 => green(&bold(&pct)),
            p if p >= 40.0 => yellow(&pct),
            _ => pct,
        };
        println!(
            "{}  {}  {}",
            pct,
            bar(m.similarity_percentage, 100.0, 20),
            truncate(m.candidate_file.name(), NAME_W)
        );
        println!(
            "        {}",
            dim(&format!(
                "{} shared words · {} shared phrases · {}",
                m.shared_keyword_count,
                m.shared_bigram_count,
                folder_of(&m.candidate_file, root)
            ))
        );
    }
    println!();
}

/// Displays groups of files that share the same name in different locations.
pub fn print_same_name_results(root: &Path, groups: &[SameNameGroup]) {
    if groups.is_empty() {
        println!("\nNo files with the same name were found in this folder.\n");
        return;
    }

    let total_files: usize = groups.iter().map(|g| g.files.len()).sum();

    header("Files with the same name");

    for group in groups {
        println!(
            "\n{}  {}",
            bold(&group.shared_name),
            dim(&format!("{} copies", group.files.len()))
        );
        for file in &group.files {
            println!(
                "  {:>10}  {}",
                format_size(file.size_bytes),
                dim(&folder_of(file, root))
            );
        }
    }

    println!();
    rule();
    println!("{} repeated names · {} files total", groups.len(), total_files);
    println!(
        "{}\n",
        dim("Tip: option [4] checks whether they also share the same content.")
    );
}
