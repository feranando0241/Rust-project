//! # OxideClean
//!
//! A file scanning, organization, and optimization tool built in Rust.
//!
//! Layout (see `docs/adr/0002-modular-architecture.md`):
//! - `core` — logic only: scanning, duplicates, queries, similarity
//! - `ui`   — terminal screens, menus and input
//! - `app`  — actions shared by the command line and the menus
//! - `cli`  — command-line argument parsing

mod app;
mod cli;
mod core;
mod ui;

use std::process::ExitCode;

use cli::Command;

/// Application entry point.
///
/// Exit codes: 0 = ok, 1 = runtime error, 2 = usage error.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match cli::parse_args(&args) {
        Ok(Command::Interactive) => {
            ui::menu::run_interactive();
            ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Ok(Command::Run {
            path,
            action,
            options,
        }) => {
            let result = app::resolve_folder(&path).and_then(|root| match action {
                // A path with no action opens the interactive menu on it
                None => {
                    ui::menu::process_target_path(&root, &options);
                    Ok(())
                }
                Some(action) => app::run_action(&root, action, &options),
            });
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("error: {}", err);
                    ExitCode::from(1)
                }
            }
        }
        Err(err) => {
            eprintln!("error: {}\nRun 'oxide_clean --help' for usage.", err);
            ExitCode::from(2)
        }
    }
}
