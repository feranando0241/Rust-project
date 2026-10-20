//! Interactive mode: the start screen and the action menu after a scan.

use crate::app;
use crate::core::files::{DEFAULT_TOP, get_top_largest, is_document};
use crate::core::models::FileInfo;
use crate::core::scanner::list_accessible_subdirectories;
use crate::core::similarity::{DEFAULT_THRESHOLD, VERY_SIMILAR};
use crate::ui::browser::run_directory_browser;
use crate::ui::input::{is_exit_command, is_one_of, read_line_from_user, resolve_user_path};
use crate::ui::picker::select_file_paginated;
use crate::ui::report::{
    print_accessible_directories, print_summary, print_top_largest, print_tree,
};
use crate::ui::style::{bold, dim, error, header, hint, prompt};
use std::path::Path;

/// The classic interactive mode: pick a folder, then pick actions from a menu.
pub fn run_interactive() {
    println!("\n{}", bold("OxideClean"));
    hint("See what's using space, find identical copies and similar documents.");

    loop {
        println!("\n{}", bold("Which folder do you want to look at?"));
        println!("  • Type its path       e.g. ~/Downloads, or . for the current folder");
        println!("  • Type b              to browse your folders");
        println!("  • Type q              to quit");
        prompt();

        let user_input = read_line_from_user();

        if is_exit_command(&user_input) {
            println!("Bye!");
            break;
        }

        if is_one_of(&user_input, &["b", "browse", "dirs", "ls"]) {
            if let Some(selected_path) = run_directory_browser() {
                process_target_path(&selected_path, true);
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
            error(&format!(
                "That's a file, not a folder: {}",
                target_path.display()
            ));
            continue;
        }

        process_target_path(&target_path, true);
    }
}

/// Scans the folder, shows the summary and opens the action menu.
pub fn process_target_path(target_path: &Path, use_cache: bool) {
    hint(&format!("\nScanning {} …", target_path.display()));

    match app::scan(target_path) {
        Ok(files) if files.is_empty() => {
            println!("No files found in {}.", target_path.display());
            hint("Hidden folders and system/build folders (like node_modules) are skipped.");
        }
        Ok(files) => {
            print_summary(target_path, &files);
            run_action_menu(target_path, &files, use_cache);
        }
        Err(err) => {
            error(&format!("Could not scan the folder: {}", err));
        }
    }
}

/// Displays and runs the action sub-menu for the currently scanned folder.
fn run_action_menu(target_path: &Path, files: &[FileInfo], use_cache: bool) {
    // Full menu the first time (or on '?'); a compact reminder afterwards
    let mut show_full_menu = true;
    loop {
        if !show_full_menu {
            hint("1 all files · 2 biggest · 3 by type · 4 folders · 5 identical copies");
            hint("6 same name · 7 similar docs · ? full menu · 0 another folder");
        } else {
            show_full_menu = false;
            println!("\n{}", bold("What would you like to do?"));
            hint("\n  Look around");
            println!("    1  See every file, folder by folder");
            println!("    2  See the {} biggest files", DEFAULT_TOP);
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
                let top = get_top_largest(files, DEFAULT_TOP);
                print_top_largest(target_path, &top);
            }
            "3" => {
                println!("\nWhich type of file? (e.g. pdf, jpg, mp4)");
                prompt();
                let ext = read_line_from_user();
                if ext.is_empty() {
                    hint("No type entered.");
                } else {
                    app::show_files_by_extension(target_path, files, &ext);
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
                if let Err(err) = app::handle_duplicates(target_path, files, true, false, use_cache)
                {
                    error(&err.to_string());
                    println!();
                }
            }
            "6" => {
                app::show_same_name_files(target_path, files);
            }
            "7" => {
                run_similarity_steps(target_path, files);
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

/// Guides the user through the similarity comparison: pick a document,
/// pick how similar the matches must be, then show the results.
fn run_similarity_steps(root: &Path, files: &[FileInfo]) {
    let documents: Vec<&FileInfo> = files.iter().filter(|f| is_document(f)).collect();

    if documents.len() < 2 {
        println!("\nThis folder needs at least 2 documents to compare.");
        hint("Supported: PDF, TXT, MD, JSON, CSV and code files (rs, py, js, c, cpp).\n");
        return;
    }

    header("Step 1 of 2 · Pick a document");
    hint("It will be compared with every other document in this folder.");

    let selected = match select_file_paginated(root, &documents) {
        Some(f) => f,
        None => return,
    };

    /// The "anything related" option: looser than the default.
    const LOOSE_THRESHOLD: f64 = 15.0;

    header("Step 2 of 2 · How similar should the matches be?");
    println!(
        "    1  Similar or closer ({:.0}% or more)   {}",
        DEFAULT_THRESHOLD,
        dim("← recommended")
    );
    println!("    2  Only very similar ({:.0}% or more)", VERY_SIMILAR);
    println!("    3  Anything related ({:.0}% or more)", LOOSE_THRESHOLD);
    println!("    4  Let me type a percentage");
    hint("  Press Enter for the recommended option.");
    prompt();

    let threshold: f64 = match read_line_from_user().as_str() {
        "2" => VERY_SIMILAR,
        "3" => LOOSE_THRESHOLD,
        "4" => {
            println!("Minimum percentage (0-100):");
            prompt();
            match read_line_from_user().trim_end_matches('%').parse::<f64>() {
                Ok(value) => value.clamp(0.0, 100.0),
                Err(_) => {
                    hint(&format!("Not a number, using {:.0}%.", DEFAULT_THRESHOLD));
                    DEFAULT_THRESHOLD
                }
            }
        }
        _ => DEFAULT_THRESHOLD,
    };

    hint(&format!(
        "\nComparing '{}' with the other documents …",
        selected.name()
    ));

    if let Err(err) = app::show_similar_documents(root, selected, files, threshold) {
        error(&err.to_string());
    }
}
