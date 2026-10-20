//! Result screens: everything the app prints after a scan or an action.

use crate::core::duplicates::DuplicateGroup;
use crate::core::files::{
    SameNameGroup, Usage, group_by_folder, usage_by_extension, usage_by_top_folder,
};
use crate::core::models::FileInfo;
use crate::core::similarity::{RELATED, SimilarityMatch, VERY_SIMILAR};
use crate::core::trash::{SkipReason, TrashReport};
use crate::ui::style::{
    bar, bold, dim, folder_of, format_size, green, header, hint, plural, red, relative, rule,
    truncate, yellow,
};
use std::path::{Path, PathBuf};

/// Max characters shown for a file name in table columns.
const NAME_W: usize = 34;
/// How many rows the scan summary shows per section.
const SUMMARY_ROWS: usize = 5;

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
        "  {} in {} · {} in total",
        bold(&plural(files.len(), "file", "files")),
        plural(folder_count, "folder", "folders"),
        bold(&format_size(total_size))
    );

    let by_folder: Vec<(String, Usage)> = usage_by_top_folder(root, files)
        .into_iter()
        .map(|(folder, usage)| {
            let label = folder.map_or("(directly in this folder)".to_string(), |f| {
                format!("./{}", f)
            });
            (label, usage)
        })
        .collect();
    let by_type: Vec<(String, Usage)> = usage_by_extension(files)
        .into_iter()
        .map(|(ext, usage)| (ext.unwrap_or_else(|| "no ext".to_string()), usage))
        .collect();

    print_ranking("Where the space goes, by folder", &by_folder, total_size);
    print_ranking("Where the space goes, by file type", &by_type, total_size);
    println!();
}

/// Prints the top rows (already sorted, most space first) with share-of-total bars.
fn print_ranking(title: &str, rows: &[(String, Usage)], total_size: u64) {
    println!("\n  {}", bold(title));
    for (name, usage) in rows.iter().take(SUMMARY_ROWS) {
        println!(
            "  {:<30} {:>10}  {}  {}",
            truncate(name, 30),
            format_size(usage.bytes),
            bar(usage.bytes as f64, total_size as f64, 12),
            dim(&plural(usage.files, "file", "files"))
        );
    }
    if rows.len() > SUMMARY_ROWS {
        println!(
            "  {}",
            dim(&format!("… and {} more", rows.len() - SUMMARY_ROWS))
        );
    }
}

// ─────────────────────────────────────────────────────────────
// FILE LISTINGS
// ─────────────────────────────────────────────────────────────

/// Displays the scan results organized as a visual tree.
pub fn print_tree(root: &Path, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    let grouped = group_by_folder(files);

    header(&format!(
        "All files in {}, folder by folder",
        root.display()
    ));

    for (folder, folder_files) in &grouped {
        let folder_size: u64 = folder_files.iter().map(|f| f.size_bytes).sum();

        println!(
            "\n{}  {}",
            bold(&relative(folder, root)),
            dim(&format!(
                "{} · {}",
                plural(folder_files.len(), "file", "files"),
                format_size(folder_size)
            ))
        );

        for (index, file) in folder_files.iter().enumerate() {
            let branch = if index == folder_files.len() - 1 {
                "└─"
            } else {
                "├─"
            };
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
        "Total: {} in {} · {}\n",
        plural(files.len(), "file", "files"),
        plural(grouped.len(), "folder", "folders"),
        bold(&format_size(total_size))
    );
}

/// Displays the top largest files table.
pub fn print_top_largest(root: &Path, files: &[FileInfo]) {
    header(&format!("The {} biggest files", files.len()));
    println!(
        "{}",
        dim(&format!(
            "{:<4} {:<NAME_W$} {:>10}  {}",
            "#", "NAME", "SIZE", "FOLDER"
        ))
    );

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
    header(&format!(".{} files: {} found", ext, files.len()));
    println!(
        "{}",
        dim(&format!("{:<NAME_W$} {:>10}  {}", "NAME", "SIZE", "FOLDER"))
    );

    for file in files {
        println!(
            "{:<NAME_W$} {:>10}  {}",
            truncate(file.name(), NAME_W),
            format_size(file.size_bytes),
            dim(&folder_of(file, root))
        );
    }
    rule();
    println!(
        "Total space used by .{}: {}\n",
        ext,
        bold(&format_size(total_size))
    );
}

/// Displays exact duplicates verified by hash and the recoverable space.
pub fn print_exact_duplicates(root: &Path, groups: &[DuplicateGroup]) {
    if groups.is_empty() {
        println!("\nNo exact duplicate files found.");
        return;
    }

    let mut total_wasted: u64 = 0;
    let mut total_dupe_files: usize = 0;

    header("Identical files");
    hint("These files have exactly the same content. In each group the oldest");
    hint("copy is kept; the extra copies can be moved to the Trash.");

    for (i, group) in groups.iter().enumerate() {
        let wasted_in_group = group.file_size * group.extra_copies as u64;
        total_wasted += wasted_in_group;
        total_dupe_files += group.extra_copies;

        println!(
            "\n{}  {}",
            bold(&format!("Group {}", i + 1)),
            dim(&format!(
                "{} · {} each · {} can be freed",
                group.original.name(),
                format_size(group.file_size),
                format_size(wasted_in_group)
            ))
        );
        println!(
            "  {}  {}",
            green("keep "),
            relative(&group.original.path, root)
        );
        for dupe in &group.duplicates {
            println!("  {}  {}", red("extra"), relative(&dupe.path, root));
        }
    }

    println!();
    rule();
    println!(
        "{} in {} · {} can be freed\n",
        plural(total_dupe_files, "extra copy", "extra copies"),
        plural(groups.len(), "group", "groups"),
        yellow(&bold(&format_size(total_wasted)))
    );
}

/// Displays what happened when moving duplicates to the Trash.
pub fn print_trash_report(root: &Path, report: &TrashReport) {
    println!(
        "\n{} moved to the Trash · {} freed",
        bold(&plural(report.moved, "file", "files")),
        bold(&format_size(report.freed_bytes))
    );
    if report.moved > 0 {
        hint("Changed your mind? You can restore them from the Trash.");
    }

    if !report.failed.is_empty() {
        let title = format!(
            "{} not moved:",
            plural(report.failed.len(), "file was", "files were")
        );
        println!("\n{}", yellow(&bold(&title)));
        for (path, reason) in &report.failed {
            let why = match reason {
                SkipReason::KeptCopyChanged(kept) => format!(
                    "skipped: the kept copy '{}' is missing or changed since the scan",
                    relative(kept, root)
                ),
                SkipReason::ChangedSinceScan => {
                    "skipped: missing or changed since the scan".to_string()
                }
                SkipReason::CouldNotMove(err) => format!("could not be moved: {}", err),
            };
            println!("  {} {}", relative(path, root), dim(&format!("— {}", why)));
        }
    }
    println!();
}

/// Displays the interactive list of accessible directories.
pub fn print_accessible_directories(parent_dir: &Path, dirs: &[PathBuf]) {
    header(&format!("Folders in {}", parent_dir.display()));
    if dirs.is_empty() {
        hint("There are no folders inside this one.");
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
    hint(&format!("Showing matches of {:.0}% or more.", threshold));
    hint(&format!(
        "{:.0}%+ very similar · {:.0}-{:.0}% related · below {:.0}% loosely related",
        VERY_SIMILAR,
        RELATED,
        VERY_SIMILAR - 1.0,
        RELATED
    ));

    if matches.is_empty() {
        println!("\nNo document is at least {:.0}% similar.", threshold);
        hint("Try a lower percentage. Scanned PDFs (images of text) can't be compared.\n");
        return;
    }

    println!();
    for m in matches {
        let pct = format!("{:>5.1}%", m.similarity_percentage);
        let pct = match m.similarity_percentage {
            p if p >= VERY_SIMILAR => green(&bold(&pct)),
            p if p >= RELATED => yellow(&pct),
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
                "{} and {} in common · {}",
                plural(m.shared_keyword_count, "word", "words"),
                plural(m.shared_bigram_count, "phrase", "phrases"),
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
    hint("Same name doesn't mean same content: these may be different versions.");

    for group in groups {
        println!(
            "\n{}  {}",
            bold(&group.shared_name),
            dim(&format!("in {} places", group.files.len()))
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
    println!(
        "{} · {} in total",
        plural(groups.len(), "repeated name", "repeated names"),
        plural(total_files, "file", "files")
    );
    hint("Tip: \"Find identical copies\" (option 5) shows which ones really are the same.\n");
}
