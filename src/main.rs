//! # OxideClean
//!
//! A file scanning, organization, and optimization tool built in Rust.

mod models;
mod scanner;
mod formatter;
mod actions;
mod similarity;
mod cli;

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cli::{Action, Command};
use actions::{
    filter_by_extension, find_exact_duplicates, find_same_name_files, get_top_largest,
    trash_duplicates,
};
use formatter::{
    print_accessible_directories, print_exact_duplicates, print_filtered_files,
    print_same_name_results, print_similarity_results, print_summary, print_top_largest,
    error, folder_of, header, hint, plural, print_trash_report, print_tree, prompt, truncate,
};
use models::FileInfo;
use scanner::{list_accessible_subdirectories, scan_directory};
use similarity::find_similar_documents;

/// Application entry point.
///
/// Exit codes: 0 = ok, 1 = runtime error, 2 = usage error.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match cli::parse_args(&args) {
        Ok(Command::Interactive) => {
            run_interactive();
            ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Ok(Command::Run { path, action }) => match run_command(&path, action) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {}", err);
                ExitCode::from(1)
            }
        },
        Err(err) => {
            eprintln!("error: {}\nRun 'oxide_clean --help' for usage.", err);
            ExitCode::from(2)
        }
    }
}

/// Runs a command-line request: scans `path`, then either runs one action and
/// returns, or opens the interactive action menu when no action was given.
fn run_command(path: &str, action: Option<Action>) -> Result<(), String> {
    let root = resolve_user_path(path);
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", root.display()));
    }

    let Some(action) = action else {
        process_target_path(&root);
        return Ok(());
    };

    let files = scan_directory(&root).map_err(|e| format!("scan failed: {}", e))?;

    match action {
        Action::Largest(n) => print_top_largest(&root, &get_top_largest(&files, n)),
        Action::Ext(ext) => show_files_by_extension(&root, &files, &ext),
        Action::Dupes { trash, assume_yes } => handle_duplicates(&root, &files, trash, assume_yes)?,
        Action::SameName => handle_same_name_files(&root, &files),
        Action::Similar { file, threshold } => {
            let target = locate_base_file(&root, &file)?;
            let matches = find_similar_documents(&target, &files, threshold);
            print_similarity_results(&root, target.name(), &matches, threshold);
        }
    }
    Ok(())
}

/// Builds the `FileInfo` for the `--similar` base file. When the file lives
/// inside `root`, its path is expressed the same way the scanner builds paths
/// (root + relative part) so the comparison recognizes and skips it.
fn locate_base_file(root: &Path, file: &str) -> Result<FileInfo, String> {
    let path = resolve_user_path(file);
    let metadata = fs::metadata(&path).map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a file", path.display()));
    }

    let path = match (path.canonicalize(), root.canonicalize()) {
        (Ok(abs), Ok(root_abs)) => match abs.strip_prefix(&root_abs) {
            Ok(rel) => root.join(rel),
            Err(_) => abs,
        },
        _ => path,
    };
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase());

    Ok(FileInfo { path, size_bytes: metadata.len(), extension })
}

/// The classic interactive mode: pick a folder, then pick actions from a menu.
fn run_interactive() {
    println!("\n{}", formatter::bold("OxideClean"));
    hint("See what's using space, find identical copies and similar documents.");

    loop {
        println!("\n{}", formatter::bold("Which folder do you want to look at?"));
        println!("  • Type its path       e.g. ~/Downloads, or . for the current folder");
        println!("  • Type b              to browse your folders");
        println!("  • Type q              to quit");
        prompt();

        let user_input = read_line_from_user();

        if is_exit_command(&user_input) {
            println!("Bye!");
            break;
        }

        if ["b", "browse", "dirs", "ls"].iter().any(|c| user_input.eq_ignore_ascii_case(c)) {
            if let Some(selected_path) = run_directory_browser() {
                process_target_path(&selected_path);
            }
            continue;
        }

        let target_path = resolve_user_path(&user_input);

        if !target_path.exists() {
            error(&format!("Folder not found: {}", target_path.display()));
            hint("  Check the spelling, or type b to browse your folders.");
            continue;
        }

        if !target_path.is_dir() {
            error(&format!("That's a file, not a folder: {}", target_path.display()));
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

        let here = current_browser_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("this folder");
        if !subdirs.is_empty() {
            println!("  • Type a number   to open that folder");
        }
        println!("  • Type s          to scan '{}'", here);
        println!("  • Type ..         to go up one level");
        println!("  • Type 0          to go back");
        prompt();

        let choice = read_line_from_user();

        if ["0", "cancel", "back", "exit", "q"].iter().any(|c| choice.eq_ignore_ascii_case(c)) {
            return None;
        }

        if ["s", "scan", "analyze"].iter().any(|c| choice.eq_ignore_ascii_case(c)) {
            return Some(current_browser_dir);
        }

        if choice == ".." {
            if let Some(parent) = current_browser_dir.parent() {
                current_browser_dir = parent.to_path_buf();
            } else {
                hint("You're already at the top.");
            }
            continue;
        }

        match choice.parse::<usize>() {
            Ok(idx) if (1..=subdirs.len()).contains(&idx) => {
                current_browser_dir = subdirs[idx - 1].clone();
            }
            Ok(_) => error(&format!("Pick a number from 1 to {}.", subdirs.len())),
            Err(_) => error("That's not an option. Type a number, s, .. or 0."),
        }
    }
}

/// Scans and processes the selected path.
fn process_target_path(target_path: &Path) {
    hint(&format!("\nScanning {} …", target_path.display()));

    match scan_directory(target_path) {
        Ok(files) if files.is_empty() => {
            println!("No files found in {}.", target_path.display());
            hint("Hidden folders and system/build folders (like node_modules) are skipped.");
        }
        Ok(files) => {
            print_summary(target_path, &files);
            run_action_menu(target_path, &files);
        }
        Err(err) => {
            error(&format!("Could not scan the folder: {}", err));
        }
    }
}

// ─────────────────────────────────────────────────────────────
// POST-SCAN ACTION MENU
// ─────────────────────────────────────────────────────────────

/// Displays and runs the action sub-menu for the currently scanned folder.
fn run_action_menu(target_path: &Path, files: &[FileInfo]) {
    // Full menu the first time (or on '?'); a compact one-liner afterwards
    let mut show_full_menu = true;
    loop {
        if !show_full_menu {
            hint("1 all files · 2 biggest · 3 by type · 4 folders · 5 identical copies");
            hint("6 same name · 7 similar docs · ? full menu · 0 another folder");
        } else {
            show_full_menu = false;
            println!("\n{}", formatter::bold("What would you like to do?"));
            hint("\n  Look around");
            println!("    1  See every file, folder by folder");
            println!("    2  See the 10 biggest files");
            println!("    3  See only one type of file (pdf, jpg, mp4…)");
            println!("    4  List the folders inside this one");
            hint("\n  Clean up");
            println!("    5  Find identical copies of files (and free up space)");
            println!("    6  Find files with the same name in different places");
            hint("\n  Compare documents");
            println!("    7  Find documents with similar content");
            println!();
            println!("    0  Choose another folder        ?  Show this menu again");
        }
        prompt();

        let option = read_line_from_user();

        match option.as_str() {
            "1" => {
                print_tree(target_path, files);
            }
            "2" => {
                let top_10 = get_top_largest(files, 10);
                print_top_largest(target_path, &top_10);
            }
            "3" => {
                println!("\nWhich type of file? (e.g. pdf, jpg, mp4)");
                prompt();
                let ext = read_line_from_user();
                if ext.is_empty() {
                    hint("No type entered.");
                } else {
                    show_files_by_extension(target_path, files, &ext);
                }
            }
            "4" => {
                if let Ok(subdirs) = list_accessible_subdirectories(target_path) {
                    print_accessible_directories(target_path, &subdirs);
                } else {
                    error("Could not list the folders.");
                }
            }
            "5" => {
                if let Err(err) = handle_duplicates(target_path, files, true, false) {
                    error(&err);
                    println!();
                }
            }
            "6" => {
                handle_same_name_files(target_path, files);
            }
            "7" => {
                handle_similarity_analysis(target_path, files);
            }
            "?" | "help" | "menu" => {
                show_full_menu = true;
            }
            "0" | "exit" | "back" | "q" => {
                break;
            }
            _ => {
                error("That's not an option. Type a number from 0 to 7, or ? to see the menu.");
            }
        }
    }
}

/// Runs the interactive document similarity analysis.
///
/// Compares a base file against all other documents in the scanned folder
/// using a composite score (vocabulary + phrases + length).
fn handle_similarity_analysis(root: &Path, files: &[FileInfo]) {
    // Filter only text files and PDFs
    let text_or_pdf_files: Vec<&FileInfo> = files
        .iter()
        .filter(|f| {
            let ext = f.extension.as_deref().unwrap_or("").to_lowercase();
            matches!(
                ext.as_str(),
                "pdf" | "txt" | "md" | "rs" | "json" | "csv" | "py" | "js" | "c" | "cpp"
            )
        })
        .collect();

    if text_or_pdf_files.len() < 2 {
        println!("\nThis folder needs at least 2 documents to compare.");
        hint("Supported: PDF, TXT, MD, JSON, CSV and code files (rs, py, js, c, cpp).\n");
        return;
    }

    header("Step 1 of 2 · Pick a document");
    hint("It will be compared with every other document in this folder.");

    let selected = match select_file_paginated(root, &text_or_pdf_files) {
        Some(f) => f,
        None => return,
    };

    header("Step 2 of 2 · How similar should the matches be?");
    println!("    1  Similar or closer (30% or more)   {}", formatter::dim("← recommended"));
    println!("    2  Only very similar (70% or more)");
    println!("    3  Anything related (15% or more)");
    println!("    4  Let me type a percentage");
    hint("  Press Enter for the recommended option.");
    prompt();

    let threshold: f64 = match read_line_from_user().as_str() {
        "2" => 70.0,
        "3" => 15.0,
        "4" => {
            println!("Minimum percentage (0-100):");
            prompt();
            match read_line_from_user().trim_end_matches('%').parse::<f64>() {
                Ok(value) => value.clamp(0.0, 100.0),
                Err(_) => {
                    hint("Not a number, using 30%.");
                    30.0
                }
            }
        }
        _ => 30.0,
    };

    hint(&format!("\nComparing '{}' with the other documents …", selected.name()));

    let matches = find_similar_documents(selected, files, threshold);
    print_similarity_results(root, selected.name(), &matches, threshold);
}

/// Displays a paginated file list (20 per page) and lets the user
/// navigate pages or search by name before selecting.
///
/// Returns the chosen file, or `None` if the user cancels.
fn select_file_paginated<'a>(root: &Path, files: &[&'a FileInfo]) -> Option<&'a FileInfo> {
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
        let shown = if search_active {
            format!("{} matching your search", active_list.len())
        } else {
            plural(total, "document", "documents")
        };
        let page_info = if page_total > 1 {
            format!(" · page {} of {}", page + 1, if search_active { page_total } else { total_pages })
        } else {
            String::new()
        };
        hint(&format!("\n{}{}", shown, page_info));

        for (i, f) in page_slice.iter().enumerate() {
            println!(
                "  {:>3}  {:<34}  {}",
                start + i + 1,
                truncate(f.name(), 34),
                formatter::dim(&truncate(&folder_of(f, root), 30)),
            );
        }
        println!();

        println!("  • Type a number   to pick that document");
        // Only show pagination options when there is more than one page
        if page + 1 < page_total {
            println!("  • Type n          for the next page");
        }
        if page > 0 {
            println!("  • Type p          for the previous page");
        }
        if search_active {
            println!("  • Type r          to show all documents again");
        } else {
            println!("  • Type s          to search by name");
        }
        println!("  • Type 0          to cancel");
        prompt();

        let input = read_line_from_user();

        match input.to_lowercase().as_str() {
            "0" | "cancel" => return None,

            "n" if page + 1 < page_total => {
                current_page = page + 1;
            }
            "p" if page > 0 => {
                current_page = page - 1;
            }

            "s" | "b" => {
                println!("Type part of the name:");
                prompt();
                let query = read_line_from_user().to_lowercase();
                if query.is_empty() {
                    hint("Empty search, showing all documents.");
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
                        error(&format!("No document name contains '{}'. Showing all again.", query));
                        active_list = files.to_vec();
                        search_active = false;
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
                        println!("Picked: {}", formatter::bold(chosen.name()));
                        return Some(chosen);
                    } else {
                        error(&format!("Pick a number from 1 to {}.", active_list.len()));
                    }
                } else {
                    error("That's not an option. Type a number, or one of the letters shown.");
                }
            }
        }
    }
}

/// Finds and displays files that share the same name within the scanned folder.
///
/// Unlike identical copies (option 5), content is not analyzed here:
/// two files with the same name but different content (e.g. different versions
/// of a document) will also appear in the list.
fn handle_same_name_files(root: &Path, files: &[FileInfo]) {
    let groups = find_same_name_files(files);
    print_same_name_results(root, &groups);
}

/// Prints the files matching an extension, or a notice when there are none.
fn show_files_by_extension(root: &Path, files: &[FileInfo], ext: &str) {
    // Accept both "pdf" and ".pdf"
    let ext = ext.trim().trim_start_matches('.');
    let filtered = filter_by_extension(files, ext);
    if filtered.is_empty() {
        println!("No .{} files in this folder.", ext);
    } else {
        print_filtered_files(root, ext, &filtered);
    }
}

/// Runs the SHA-256 duplicate analysis and prints the groups.
///
/// With `offer_trash`, the redundant copies are then moved to the Trash after
/// a y/N confirmation, or straight away with `assume_yes`. Without a terminal
/// to ask on and without `assume_yes`, nothing is deleted and an error is returned.
fn handle_duplicates(
    root: &Path,
    files: &[FileInfo],
    offer_trash: bool,
    assume_yes: bool,
) -> Result<(), String> {
    hint("\nComparing file contents …");
    let duplicate_groups = find_exact_duplicates(files);

    if duplicate_groups.is_empty() {
        println!("No identical copies found. Nothing to clean up here.\n");
        return Ok(());
    }

    print_exact_duplicates(root, &duplicate_groups);

    if !offer_trash {
        return Ok(());
    }

    let copies: usize = duplicate_groups.iter().map(|g| g.duplicates.len()).sum();

    if !assume_yes {
        if !io::stdin().is_terminal() {
            return Err(
                "Nothing was moved: there's no terminal to ask for confirmation \
                 (pass --yes to skip it)"
                    .to_string(),
            );
        }

        println!(
            "Move the {} to the Trash? You can restore {} from there later.",
            plural(copies, "extra copy", "extra copies"),
            if copies == 1 { "it" } else { "them" }
        );
        hint("  Type y to move them, or press Enter to leave everything as it is.");
        prompt();
        let confirmation = read_line_from_user();
        if !(confirmation.eq_ignore_ascii_case("y") || confirmation.eq_ignore_ascii_case("yes")) {
            println!("Nothing was moved.\n");
            return Ok(());
        }
    }

    let report = trash_duplicates(&duplicate_groups);
    print_trash_report(root, &report);

    if report.failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} file(s) could not be moved to the Trash", report.failed.len()))
    }
}

// ─────────────────────────────────────────────────────────────
// INPUT AND PATH HELPER FUNCTIONS
// ─────────────────────────────────────────────────────────────

/// Reads a line from standard input (keyboard) and trims whitespace and newlines.
fn read_line_from_user() -> String {
    // Flush so prompts written with `print!` appear before the user types
    let _ = io::stdout().flush();
    let mut buffer = String::new();
    let bytes_read = io::stdin()
        .read_line(&mut buffer)
        .expect("Failed to read user input");
    // End of input (Ctrl+D or a closed pipe): exit instead of looping on empty answers
    if bytes_read == 0 {
        println!();
        std::process::exit(0);
    }
    buffer.trim().to_string()
}

/// Returns true if the given input matches a quit command.
fn is_exit_command(input: &str) -> bool {
    ["0", "q", "quit", "exit"].iter().any(|c| input.eq_ignore_ascii_case(c))
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
    fn test_duplicates_keep_oldest() {
        use std::fs::{self, File};
        use std::time::{Duration, SystemTime};

        let dir = std::env::temp_dir().join(format!("oxide_clean_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let newer = dir.join("a_newer.txt");
        let older = dir.join("b_older.txt");
        fs::write(&newer, "same content").unwrap();
        fs::write(&older, "same content").unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(3600);
        File::options().write(true).open(&older).unwrap().set_modified(old_time).unwrap();

        let files = scan_directory(&dir).unwrap();
        let groups = find_exact_duplicates(&files);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, older);
        assert_eq!(groups[0].duplicates[0].path, newer);
    }

    /// Creates an empty, unique temp directory for a test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxide_clean_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn test_hardlinks_are_not_duplicates() {
        let dir = temp_dir("hardlinks");
        std::fs::write(dir.join("a.txt"), "same content").unwrap();
        std::fs::hard_link(dir.join("a.txt"), dir.join("b.txt")).unwrap();

        let groups = find_exact_duplicates(&scan_directory(&dir).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(groups.is_empty(), "hardlinks to one file must not be reported as duplicates");
    }

    #[test]
    fn test_trash_skips_group_when_kept_copy_changed() {
        let dir = temp_dir("changed_original");
        std::fs::write(dir.join("a.txt"), "same content").unwrap();
        std::fs::write(dir.join("b.txt"), "same content").unwrap();

        let groups = find_exact_duplicates(&scan_directory(&dir).unwrap());
        assert_eq!(groups.len(), 1);
        // The kept copy is edited after the scan: its duplicate must survive
        std::fs::write(&groups[0].original.path, "edited after scan").unwrap();

        let report = trash_duplicates(&groups);
        let duplicate_still_there = groups[0].duplicates[0].path.exists();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(report.moved, 0);
        assert_eq!(report.failed.len(), 1);
        assert!(duplicate_still_there);
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
