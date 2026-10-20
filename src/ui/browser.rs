//! Interactive folder browser used to pick a folder before scanning.

use std::path::PathBuf;
use crate::core::scanner::list_accessible_subdirectories;
use crate::ui::input::{is_one_of, read_line_from_user};
use crate::ui::report::print_accessible_directories;
use crate::ui::style::{error, hint, prompt};

/// Lets the user walk their folders starting from home.
/// Returns the folder to scan, or `None` if they go back.
pub fn run_directory_browser() -> Option<PathBuf> {
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

        if is_one_of(&choice, &["0", "cancel", "back", "exit", "q"]) {
            return None;
        }

        if is_one_of(&choice, &["s", "scan", "analyze"]) {
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
