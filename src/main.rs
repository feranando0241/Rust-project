//! # OxideClean
//!
//! A file scanning, organization, and optimization tool built in Rust.

mod models;
mod scanner;
mod formatter;
mod actions;
mod similarity;

use std::io;
use std::path::{Path, PathBuf};

use actions::{
    filter_by_extension, find_exact_duplicates, find_same_name_files, get_top_largest,
    send_files_to_trash,
};
use formatter::{
    print_accessible_directories, print_exact_duplicates, print_filtered_files,
    print_same_name_results, print_similarity_results, print_top_largest, print_tree,
};
use models::FileInfo;
use scanner::{list_accessible_subdirectories, scan_directory};
use similarity::find_similar_documents;

/// Application entry point.
fn main() {
    println!(" OxideClean — File Organizer & Scanner");
    println!("=========================================");

    loop {
        println!("\nStart options:");
        println!("   - Type a path to scan (e.g. ~/Downloads, . for current)");
        println!("   - Type 'dirs' or 'ls' to browse your accessible folders");
        println!("   - Type '0' or 'exit' to quit");
        print!("Enter your option or path: ");

        let user_input = read_line_from_user();

        if is_exit_command(&user_input) {
            println!("Goodbye! Thanks for using OxideClean.");
            break;
        }

        if user_input.eq_ignore_ascii_case("dirs") || user_input.eq_ignore_ascii_case("ls") {
            if let Some(selected_path) = run_directory_browser() {
                process_target_path(&selected_path);
            }
            continue;
        }

        let target_path = resolve_user_path(&user_input);

        if !target_path.exists() {
            eprintln!("Path '{}' does not exist. Try another.", target_path.display());
            continue;
        }

        if !target_path.is_dir() {
            eprintln!("'{}' is not a directory. Try another path.", target_path.display());
            continue;
        }

        process_target_path(&target_path);
    }
}

/// Runs the interactive accessible-directory browser.
fn run_directory_browser() -> Option<PathBuf> {
    let mut current_browser_dir = if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    };

    loop {
        let subdirs = match list_accessible_subdirectories(&current_browser_dir) {
            Ok(dirs) => dirs,
            Err(err) => {
                eprintln!("Could not list '{}': {}", current_browser_dir.display(), err);
                return None;
            }
        };

        print_accessible_directories(&current_browser_dir, &subdirs);

        println!("Navigation:");
        println!("   - Enter a folder number [#] to enter or scan it");
        println!("   - Enter '..' to go up one level");
        println!("   - Enter 'scan' to scan the current folder ({})", current_browser_dir.display());
        println!("   - Enter '0', 'cancel', or 'exit' to go back");
        print!("Select: ");

        let choice = read_line_from_user();

        if choice == "0"
            || choice.eq_ignore_ascii_case("cancel")
            || choice.eq_ignore_ascii_case("back")
            || choice.eq_ignore_ascii_case("exit")
        {
            return None;
        }

        if choice.eq_ignore_ascii_case("scan") || choice.eq_ignore_ascii_case("analyze") {
            return Some(current_browser_dir);
        }

        if choice == ".." {
            if let Some(parent) = current_browser_dir.parent() {
                current_browser_dir = parent.to_path_buf();
            } else {
                println!("You are already at the root of the file system.");
            }
            continue;
        }

        if let Ok(idx) = choice.parse::<usize>() {
            if idx >= 1 && idx <= subdirs.len() {
                let chosen = &subdirs[idx - 1];
                println!("\nWhat do you want to do with '{}'?", chosen.display());
                println!("  [1] Scan and analyze this folder");
                println!("  [2] Enter and browse its subfolders");
                println!("  [0] Go back");
                print!("        Option: ");

                let sub_choice = read_line_from_user();
                match sub_choice.as_str() {
                    "1" => return Some(chosen.clone()),
                    "2" => {
                        current_browser_dir = chosen.clone();
                    }
                    _ => {}
                }
            } else {
                println!("Number out of range.");
            }
        } else {
            println!("Unrecognized option.");
        }
    }
}

/// Scans and processes the selected path.
fn process_target_path(target_path: &Path) {
    println!("\n Scanning '{}' recursively...", target_path.display());

    match scan_directory(target_path) {
        Ok(files) if files.is_empty() => {
            println!(" No files found in '{}'.", target_path.display());
        }
        Ok(files) => {
            print_tree(target_path, &files);
            run_action_menu(target_path, &files);
        }
        Err(error) => {
            eprintln!("Scan error: {}", error);
        }
    }
}

// ─────────────────────────────────────────────────────────────
// POST-SCAN ACTION MENU
// ─────────────────────────────────────────────────────────────

/// Displays and runs the action sub-menu for the currently scanned folder.
fn run_action_menu(target_path: &Path, files: &[FileInfo]) {
    loop {
        println!("┌────────────────────────────────────────────────────────────┐");
        println!("│ What do you want to do with this folder?                   │");
        println!("│ [1] View full file tree                                    │");
        println!("│ [2] View Top 10 largest files                              │");
        println!("│ [3] Filter by type / extension (e.g. pdf, png)             │");
        println!("│ [4] Find duplicates and move to Trash                      │");
        println!("│ [5] View accessible subdirectories in this path            │");
        println!("│ [6] Analyze content similarity between documents           │");
        println!("│ [7] Find files with the same name                          │");
        println!("│ [0] Back to main menu / Scan another folder                │");
        println!("└────────────────────────────────────────────────────────────┘");
        print!("Select an option [0-7]: ");

        let option = read_line_from_user();

        match option.as_str() {
            "1" => {
                print_tree(target_path, files);
            }
            "2" => {
                let top_10 = get_top_largest(files, 10);
                print_top_largest(&top_10);
            }
            "3" => {
                println!("\nEnter the extension to search for (e.g. pdf, mov, png, jpg):");
                let ext = read_line_from_user();
                if ext.is_empty() {
                    println!("No extension entered.");
                } else {
                    let filtered = filter_by_extension(files, &ext);
                    if filtered.is_empty() {
                        println!("No files found with extension '.{}'.", ext);
                    } else {
                        print_filtered_files(&ext, &filtered);
                    }
                }
            }
            "4" => {
                handle_duplicates_and_trash(files);
            }
            "5" => {
                if let Ok(subdirs) = list_accessible_subdirectories(target_path) {
                    print_accessible_directories(target_path, &subdirs);
                } else {
                    eprintln!(" Could not list subdirectories.");
                }
            }
            "6" => {
                handle_similarity_analysis(files);
            }
            "7" => {
                handle_same_name_files(files);
            }
            "0" | "exit" | "back" => {
                println!("Returning to main menu...");
                break;
            }
            _ => {
                println!("Invalid option. Please enter a number from 0 to 7.");
            }
        }
    }
}

/// Runs the interactive document similarity analysis.
///
/// Compares a base file against all other documents in the scanned folder
/// using a composite score (vocabulary + phrases + length).
fn handle_similarity_analysis(files: &[FileInfo]) {
    // Filter only text files and PDFs
    let text_or_pdf_files: Vec<&FileInfo> = files
        .iter()
        .filter(|f| {
            let ext = f.extension.as_deref().unwrap_or("").to_lowercase();
            matches!(
                ext.as_str(),
                "pdf" | "txt" | "md" | "rs" | "json" | "csv" | "py" | "js" | "c" | "cpp" | "docx"
            )
        })
        .collect();

    if text_or_pdf_files.len() < 2 {
        println!("\nAt least 2 text/PDF files are needed in this folder to compare.");
        println!("Supported types: PDF, TXT, MD, source code (rs, py, js, c, cpp), JSON, CSV, DOCX\n");
        return;
    }

    println!("\nSelect the BASE file to compare against the others:");
    println!("──────────────────────────────────────────────────────────────────────────");

    let selected = match select_file_paginated(&text_or_pdf_files) {
        Some(f) => f,
        None => return,
    };

    // Threshold guide for the user
    println!("\nConfigure the minimum similarity threshold:");
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("   The score combines shared vocabulary, common phrases, and length.");
    println!("   Reference guide for choosing your threshold:");
    println!("   ─────────────────────────────────────────────────────────────────────");
    println!("   │  >= 70%  │ Very similar: same topic, structure, and phrasing      │");
    println!("   │  40-69%  │ Related: shared vocabulary, different focus            │");
    println!("   │  15-39%  │ Somewhat related: share some terms in the field        │");
    println!("   │  < 15%   │ Distant: very little content overlap                  │");
    println!("   ─────────────────────────────────────────────────────────────────────");
    println!("  [1] Automatic  — Shows documents with >= 30% similarity");
    println!("  [2] Custom     — You choose the minimum percentage");
    print!("Option [1 or 2]: ");

    let mode_choice = read_line_from_user();
    let threshold: f64 = match mode_choice.as_str() {
        "2" => {
            print!("Enter the minimum percentage (e.g. 10, 30, 50, 80): ");
            let custom_input = read_line_from_user();
            custom_input.parse::<f64>().unwrap_or(30.0).clamp(0.0, 100.0)
        }
        _ => 30.0, // Automatic mode: 30% is more useful than 50%
    };

    println!(
        "\n Analyzing similarities against '{}' (threshold: >= {:.1}%)...",
        selected.name(),
        threshold
    );

    let matches = find_similar_documents(selected, files, threshold);
    print_similarity_results(selected.name(), &matches, threshold);
}

/// Displays a paginated file list (20 per page) and lets the user
/// navigate pages or search by name before selecting.
///
/// Returns the chosen file, or `None` if the user cancels.
fn select_file_paginated<'a>(files: &[&'a FileInfo]) -> Option<&'a FileInfo> {
    const PAGE_SIZE: usize = 20;
    let total = files.len();
    // total_pages is fixed: used in the header when no search is active.
    // page_total is recalculated each iteration to reflect the active list (may be filtered).
    let total_pages = total.div_ceil(PAGE_SIZE);
    let mut current_page: usize = 0;
    // Active list: may be the full listing or a search result.
    let mut active_list: Vec<&FileInfo> = files.to_vec();
    let mut search_active = false;

    loop {
        let page_total = active_list.len().div_ceil(PAGE_SIZE).max(1);
        let page = current_page.min(page_total - 1);
        let start = page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(active_list.len());
        let page_slice = &active_list[start..end];

        // Page header
        if search_active {
            println!(
                "\n Search results ({} found) — Page {}/{}",
                active_list.len(), page + 1, page_total
            );
        } else {
            println!(
                "\n Available files ({} total) — Page {}/{}",
                total, page + 1, total_pages
            );
        }
        println!("──────────────────────────────────────────────────────────────────────────");
        println!("  {:<4}  {:<30}  {:<6}  {}", "#", "NAME", "TYPE", "FOLDER");
        println!("  ────  ──────────────────────────────  ──────  ──────────────────────────");

        for (i, f) in page_slice.iter().enumerate() {
            let global_num = start + i + 1;
            let ext = f.extension.as_deref().unwrap_or("?").to_uppercase();
            // Show only the containing folder name to avoid cluttering the screen
            let folder = f.path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("root");
            println!(
                "  {:<4}  {:<30}  {:<6}  {}",
                format!("[{}]", global_num),
                truncate_name(f.name(), 30),
                ext,
                truncate_name(folder, 26),
            );
        }

        println!("──────────────────────────────────────────────────────────────────────────");
        println!("Navigation:");

        // Only show pagination options when there is more than one page
        if page_total > 1 {
            if page + 1 < page_total {
                println!("  [n] Next page →");
            }
            if page > 0 {
                println!("  [p] Previous page ←");
            }
        }
        if search_active {
            println!("  [r] Reset search / show all");
        } else {
            println!("  [b] Search by file name");
        }
        println!("  [0] Cancel");
        print!("Enter a number or command: ");

        let input = read_line_from_user();

        match input.to_lowercase().as_str() {
            "0" | "cancel" => return None,

            "n" if page + 1 < page_total => {
                current_page = page + 1;
            }
            "p" if page > 0 => {
                current_page = page - 1;
            }

            "b" => {
                print!("Type part of the file name to search: ");
                let query = read_line_from_user().to_lowercase();
                if query.is_empty() {
                    println!("Empty search. Showing all files.");
                    active_list = files.to_vec();
                    search_active = false;
                } else {
                    active_list = files
                        .iter()
                        .filter(|f| f.name().to_lowercase().contains(&query))
                        .copied()
                        .collect();
                    search_active = true;
                    current_page = 0;

                    if active_list.is_empty() {
                        println!("No files found with '{}'. Returning to full list.", query);
                        active_list = files.to_vec();
                        search_active = false;
                    } else {
                        println!(" {} file(s) found with '{}'.", active_list.len(), query);
                    }
                }
            }

            "r" => {
                active_list = files.to_vec();
                search_active = false;
                current_page = 0;
            }

            raw => {
                // Try to interpret as a selection number
                if let Ok(num) = raw.parse::<usize>() {
                    if num >= 1 && num <= active_list.len() {
                        let chosen = active_list[num - 1];
                        println!("\n Selected: {} ({})", chosen.name(), chosen.path.display());
                        return Some(chosen);
                    } else {
                        println!("Number out of range (1-{}). Try again.", active_list.len());
                    }
                } else {
                    println!("Unrecognized option. Use a number, 'n', 'p', 'b', 'r', or '0'.");
                }
            }
        }
    }
}

/// Truncates a string to the given maximum character count, appending '…' if cut.
fn truncate_name(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut)
    }
}

/// Finds and displays files that share the same name within the scanned folder.
///
/// Unlike exact duplicates (option 4), content is not analyzed here:
/// two files with the same name but different content (e.g. different versions
/// of a document) will also appear in the list.
fn handle_same_name_files(files: &[FileInfo]) {
    println!("\nSearching for files with the same name...");
    let groups = find_same_name_files(files);
    print_same_name_results(&groups);
}

/// Runs the SHA-256 duplicate analysis and offers to move them to the Trash.
fn handle_duplicates_and_trash(files: &[FileInfo]) {
    println!("\nCalculating SHA-256 hashes to find exact duplicates...");
    let duplicate_groups = find_exact_duplicates(files);

    if duplicate_groups.is_empty() {
        println!("Great! No exact duplicate files were found in this folder.\n");
        return;
    }

    print_exact_duplicates(&duplicate_groups);

    let files_to_trash: Vec<FileInfo> = duplicate_groups
        .iter()
        .flat_map(|g| g.duplicates.clone())
        .collect();

    println!("Do you want to move these {} duplicate file(s) to the Trash?", files_to_trash.len());
    println!("Enter 'y' or 'yes' to confirm, anything else to cancel:");

    let confirmation = read_line_from_user();

    if confirmation.eq_ignore_ascii_case("y") || confirmation.eq_ignore_ascii_case("yes") {
        match send_files_to_trash(&files_to_trash) {
            Ok(count) => {
                println!("\nSuccess! {} file(s) moved to the Trash.", count);
                println!("You can restore them from the Trash if needed.\n");
            }
            Err(err) => {
                eprintln!("\n{}", err);
            }
        }
    } else {
        println!("\nOperation cancelled. No files were modified.\n");
    }
}

// ─────────────────────────────────────────────────────────────
// INPUT AND PATH HELPER FUNCTIONS
// ─────────────────────────────────────────────────────────────

/// Reads a line from standard input (keyboard) and trims whitespace and newlines.
fn read_line_from_user() -> String {
    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .expect("Failed to read user input");
    buffer.trim().to_string()
}

/// Returns true if the given input matches a quit command.
fn is_exit_command(input: &str) -> bool {
    input == "0" || input.eq_ignore_ascii_case("exit")
}

/// Converts user input into a valid `PathBuf`, expanding `~` to the home directory.
fn resolve_user_path(input: &str) -> PathBuf {
    if input.starts_with("~/") || input == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            if input.len() > 2 {
                path.push(&input[2..]);
            }
            path
        } else {
            PathBuf::from(input)
        }
    } else if input.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(input)
    }
}

// ─────────────────────────────────────────────────────────────
// AUTOMATED TESTS
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use formatter::format_size;
    use scanner::should_ignore_directory;
    use similarity::calculate_jaccard_similarity;

    #[test]
    fn test_format_sizes() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_size(10 * 1024 * 1024 * 1024), "10.00 GB");
    }

    #[test]
    fn test_is_exit_command() {
        assert!(is_exit_command("0"));
        assert!(is_exit_command("exit"));
        assert!(is_exit_command("EXIT"));
        assert!(is_exit_command("Exit"));
        assert!(!is_exit_command("no"));
        assert!(!is_exit_command("~/Downloads"));
    }

    #[test]
    fn test_should_ignore_directory() {
        assert!(should_ignore_directory("node_modules"));
        assert!(should_ignore_directory("target"));
        assert!(should_ignore_directory(".git"));
        assert!(should_ignore_directory(".cache"));
        assert!(!should_ignore_directory("photos"));
        assert!(!should_ignore_directory("documents"));
    }

    #[test]
    fn test_similarity_algorithm() {
        use std::collections::HashSet;
        let mut set1 = HashSet::new();
        set1.insert("thesis about intelligence".to_string());
        set1.insert("applied artificial intelligence".to_string());
        set1.insert("experiment results".to_string());

        let mut set2 = HashSet::new();
        set2.insert("thesis about intelligence".to_string());
        set2.insert("applied artificial intelligence".to_string());
        set2.insert("another different section".to_string());

        let (sim, count) = calculate_jaccard_similarity(&set1, &set2);
        assert_eq!(count, 2);
        assert!(sim >= 50.0, "Should have at least 50% similarity");
    }
}
