use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::actions::{DuplicateGroup, SameNameGroup};
use crate::models::FileInfo;
use crate::similarity::SimilarityMatch;

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
pub fn print_tree(root_dir: &Path, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    let grouped = group_by_folder(files);

    println!("\nScan complete for: {}", root_dir.display());
    println!("══════════════════════════════════════════════════════════════════");

    for (folder, folder_files) in &grouped {
        let folder_size: u64 = folder_files.iter().map(|f| f.size_bytes).sum();

        let display_folder = if folder == root_dir {
            "[Root]".to_string()
        } else if let Ok(relative) = folder.strip_prefix(root_dir) {
            format!("./{}", relative.display())
        } else {
            format!("{}", folder.display())
        };

        println!(
            "\n{} ({} files - {})",
            display_folder,
            folder_files.len(),
            format_size(folder_size)
        );
        println!("  ├─────────────────────────────────────────────────────────────");

        for (index, file) in folder_files.iter().enumerate() {
            let is_last = index == folder_files.len() - 1;
            let branch = if is_last { "  └─" } else { "  ├─" };
            let ext = file.extension_or("no ext");
            let size = format_size(file.size_bytes);

            println!("{} {:<38} {:>10}  [{}]", branch, file.name(), size, ext);
        }
    }

    println!("\n══════════════════════════════════════════════════════════════════");
    println!(
        "Total: {} file(s) in {} folder(s) | Total size: {}",
        files.len(),
        grouped.len(),
        format_size(total_size)
    );
    println!("══════════════════════════════════════════════════════════════════\n");
}

/// Displays the top largest files table.
pub fn print_top_largest(files: &[FileInfo]) {
    println!("\n=== Top Largest Files ===");
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("{:<4} {:<35} {:>10}   {}", "RANK", "NAME", "SIZE", "PATH");
    println!("──────────────────────────────────────────────────────────────────────────");

    for (i, file) in files.iter().enumerate() {
        println!(
            "#{:<3} {:<35} {:>10}   {}",
            i + 1,
            file.name(),
            format_size(file.size_bytes),
            file.path.display()
        );
    }
    println!("──────────────────────────────────────────────────────────────────────────\n");
}

/// Displays the list of files filtered by extension.
pub fn print_filtered_files(ext: &str, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    println!("\n=== Files with extension '.{}' ({} found) ===", ext, files.len());
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("{:<38} {:>10}   {}", "NAME", "SIZE", "PATH");
    println!("──────────────────────────────────────────────────────────────────────────");

    for file in files {
        println!(
            "{:<38} {:>10}   {}",
            file.name(),
            format_size(file.size_bytes),
            file.path.display()
        );
    }
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("Total space used by '.{}': {}\n", ext, format_size(total_size));
}

/// Displays exact duplicates verified by hash and the recoverable space.
pub fn print_exact_duplicates(groups: &[DuplicateGroup]) {
    if groups.is_empty() {
        println!("\nNo exact duplicate files found.");
        return;
    }

    let mut total_wasted: u64 = 0;
    let mut total_dupe_files: usize = 0;

    println!("\n=== Verified Duplicate Files (Identical SHA-256 Hash) ===");
    println!("══════════════════════════════════════════════════════════════════");

    for (i, group) in groups.iter().enumerate() {
        let wasted_in_group = group.file_size * group.duplicates.len() as u64;
        total_wasted += wasted_in_group;
        total_dupe_files += group.duplicates.len();

        println!(
            "\n[Group #{}] Size: {} each | Wasted: {}",
            i + 1,
            format_size(group.file_size),
            format_size(wasted_in_group)
        );
        println!("  [KEEP]      {} ({})", group.original.name(), group.original.path.display());
        for dupe in &group.duplicates {
            println!("  [DUPLICATE] {} ({})", dupe.name(), dupe.path.display());
        }
    }

    println!("\n══════════════════════════════════════════════════════════════════");
    println!(
        "Summary: {} duplicate file(s) found in {} group(s)",
        total_dupe_files,
        groups.len()
    );
    println!("Recoverable space by moving to Trash: {}", format_size(total_wasted));
    println!("══════════════════════════════════════════════════════════════════\n");
}

/// Displays the interactive list of accessible directories.
pub fn print_accessible_directories(parent_dir: &Path, dirs: &[PathBuf]) {
    println!("\n=== Accessible Folders in '{}' ===", parent_dir.display());
    println!("──────────────────────────────────────────────────────────────────────────");
    if dirs.is_empty() {
        println!("No additional subfolders in this directory.");
    } else {
        for (i, dir) in dirs.iter().enumerate() {
            let folder_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("folder");
            println!(" [{:>2}] {:<30} ({})", i + 1, folder_name, dir.display());
        }
    }
    println!("──────────────────────────────────────────────────────────────────────────\n");
}

/// Displays the results of the content similarity comparison.
///
/// For each result it prints:
/// - Name of the compared document
/// - Composite similarity percentage with a visual bar
/// - How many keywords and bigrams they share
/// - Full file path
pub fn print_similarity_results(target_name: &str, matches: &[SimilarityMatch], threshold: f64) {
    println!("\n=== Similarity Report for: '{}' ===", target_name);
    println!("    (Min threshold: >= {:.1}% | Score = 50% vocabulary + 40% phrases + 10% length)", threshold);
    println!("══════════════════════════════════════════════════════════════════════════════════");

    if matches.is_empty() {
        println!("No documents found with similarity >= {:.1}%.", threshold);
        println!("Suggestions:");
        println!("   • Try lowering the threshold (e.g. 10% or 20%) to catch weaker similarities.");
        println!("   • Make sure the compared files contain real text (not scanned images).");
        println!("   • Ensure the scanned folder contains other text/PDF documents.");
    } else {
        println!(
            "{:<35} {:>10}   {:>9}   {:>8}   {}",
            "COMPARED DOCUMENT", "SIMILARITY", "KEYWORDS", "PHRASES", "PATH"
        );
        println!("──────────────────────────────────────────────────────────────────────────────────────");

        for m in matches {
            // Visual progress bar of 20 blocks
            let bar_filled = ((m.similarity_percentage / 100.0) * 20.0).round() as usize;
            let bar_filled = bar_filled.min(20);
            let bar = "█".repeat(bar_filled);
            let empty = "░".repeat(20 - bar_filled);

            println!(
                "{:<35} {:>5.1}% [{}{}]   +{} words   +{} phrases   {}",
                m.candidate_file.name(),
                m.similarity_percentage,
                bar,
                empty,
                m.shared_keyword_count,
                m.shared_bigram_count,
                m.candidate_file.path.display()
            );
        }
    }
    println!("══════════════════════════════════════════════════════════════════════════════════\n");
}

/// Displays groups of files that share the same name in different locations.
///
/// For each group it prints the shared name, how many files have it,
/// and the full path of each so the user can decide what to do.
pub fn print_same_name_results(groups: &[SameNameGroup]) {
    if groups.is_empty() {
        println!("\nNo files with the same name were found in this folder.\n");
        return;
    }

    let total_files: usize = groups.iter().map(|g| g.files.len()).sum();

    println!("\n=== Files with the Same Name ===");
    println!("══════════════════════════════════════════════════════════════════");

    for (i, group) in groups.iter().enumerate() {
        println!(
            "\n[Group #{}] '{}' — {} files found",
            i + 1,
            group.shared_name,
            group.files.len()
        );
        println!("  ─────────────────────────────────────────────────────────────────");
        for (j, file) in group.files.iter().enumerate() {
            println!(
                "  [{}.{}] {}   ({})",
                i + 1,
                j + 1,
                format_size(file.size_bytes),
                file.path.display()
            );
        }
    }

    println!("\n══════════════════════════════════════════════════════════════════");
    println!(
        "Summary: {} group(s) with repeated names | {} files total",
        groups.len(),
        total_files
    );
    println!("   Tip: use option [4] to check if they also share the same content (exact duplicates).");
    println!("══════════════════════════════════════════════════════════════════\n");
}
