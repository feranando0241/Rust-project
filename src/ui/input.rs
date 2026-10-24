//! Reading what the user types and turning it into paths and commands.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Reads a line from standard input (keyboard) and trims whitespace and newlines.
pub fn read_line_from_user() -> String {
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
pub fn is_exit_command(input: &str) -> bool {
    ["0", "q", "quit", "exit"]
        .iter()
        .any(|c| input.eq_ignore_ascii_case(c))
}

/// True if `input` is one of `options`, ignoring ASCII case.
pub fn is_one_of(input: &str, options: &[&str]) -> bool {
    options.iter().any(|c| input.eq_ignore_ascii_case(c))
}

/// Converts user input into a valid `PathBuf`, expanding `~` to the home directory.
pub fn resolve_user_path(input: &str) -> PathBuf {
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

/// Expresses `path` the way the scanner builds paths for `root` (root +
/// relative part), so it can be compared with scanned files. A path outside
/// `root`, or one that can't be resolved, is returned unchanged.
pub fn in_scan_form(root: &Path, path: PathBuf) -> PathBuf {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(abs), Ok(root_abs)) => match abs.strip_prefix(&root_abs) {
            Ok(rel) => root.join(rel),
            Err(_) => abs,
        },
        _ => path,
    }
}

/// A folder typed by the user (`--keep-in`, `--exclude`, or in the menu), in
/// the same form as the scanned paths. Relative folders are taken relative to
/// the scanned `root`; `~` is expanded.
pub fn folder_in_scan(root: &Path, input: &str) -> PathBuf {
    let typed = resolve_user_path(input);
    if typed.is_absolute() {
        in_scan_form(root, typed)
    } else {
        root.join(typed)
    }
}

/// Parses a list of group numbers like `1,3,5-7` (spaces allowed) into
/// zero-based indices, sorted and without repeats. Numbers go from 1 to `max`.
pub fn parse_selection(input: &str, max: usize) -> Result<Vec<usize>, String> {
    let number = |s: &str| -> Result<usize, String> {
        match s.trim().parse::<usize>() {
            Ok(n) if (1..=max).contains(&n) => Ok(n),
            _ => Err(format!(
                "'{}' isn't a group number from 1 to {}.",
                s.trim(),
                max
            )),
        }
    };

    let mut picked = BTreeSet::new();
    for part in input.split(',').filter(|p| !p.trim().is_empty()) {
        match part.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (number(from)?, number(to)?);
                if from > to {
                    return Err(format!("'{}' goes backwards.", part.trim()));
                }
                picked.extend(from..=to);
            }
            None => {
                picked.insert(number(part)?);
            }
        }
    }
    if picked.is_empty() {
        return Err("No group numbers given.".to_string());
    }
    Ok(picked.into_iter().map(|n| n - 1).collect())
}

#[cfg(test)]
#[path = "../tests/ui_input.rs"]
mod tests;
