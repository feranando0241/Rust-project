//! Reading what the user types and turning it into paths and commands.

use std::io::{self, Write};
use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_exit_command() {
        assert!(is_exit_command("0"));
        assert!(is_exit_command("exit"));
        assert!(is_exit_command("EXIT"));
        assert!(is_exit_command("Exit"));
        assert!(!is_exit_command("no"));
        assert!(!is_exit_command("~/Downloads"));
    }
}
