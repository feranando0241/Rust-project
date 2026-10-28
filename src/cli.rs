//! Command-line argument parsing (see `docs/adr/0001-cli-arguments.md`).
//!
//! Hand-rolled on top of `std::env::args` to avoid a dependency while the
//! interface is small. Move to `clap` once the ADR's revisit triggers are hit.

use crate::core::files::DEFAULT_TOP;
use crate::core::old_files::{DEFAULT_OLD_DAYS, days};
use crate::core::similarity::DEFAULT_THRESHOLD;

/// Text printed by `--help`.
pub const HELP: &str = "\
OxideClean — file scanner and organizer

USAGE:
    oxide_clean                       Interactive menus
    oxide_clean <PATH>                Scan PATH, show the summary and the action menu
    oxide_clean <PATH> <ACTION>       Run a single action and exit

ACTIONS:
    --largest [N]          Top N largest files (default 10)
    --ext <EXT>            Files with the given extension (e.g. pdf)
    --dupes                List exact duplicates (read-only)
      --keep-in <FOLDER>   ...keep every copy inside FOLDER instead of the oldest
      --trash              ...and move the extra copies to the Trash (asks first)
      --yes, -y            ...without asking
    --same-name            Files that share the same name
    --old [DAYS]           Files neither modified nor read in DAYS days (default 180),
                           biggest first (read-only). Files that arrived on this
                           disk in the last 30 days are never listed
    --similar <FILE>       Documents with content similar to FILE
      --threshold <N>      Minimum similarity percentage (default 30)

OPTIONS:
    --exclude <FOLDER>     Leave FOLDER out of the scan (can be repeated)
    --no-cache             Don't reuse or save file hashes between runs
    -h, --help             Show this help

FOLDER can be relative to PATH (e.g. --keep-in Photos) or a full path.

File hashes are cached in ~/Library/Caches/oxide_clean (macOS) or
~/.cache/oxide_clean, or under $XDG_CACHE_HOME if set, so a second scan of the
same folder skips unchanged files. Moving files to the Trash never relies on it.

--old uses the dates the file system keeps. Copies made without keeping dates
(plain cp, Finder) look new, and so does a file whose details changed in the
last 30 days. It errs towards listing less. See docs/adr/0004.

Colors are disabled when the output is not a terminal or NO_COLOR is set.
";

/// A single non-interactive action requested on the command line.
#[derive(Debug, PartialEq)]
pub enum Action {
    Largest(usize),
    Ext(String),
    Dupes {
        trash: bool,
        assume_yes: bool,
        /// Keep the copy inside this folder rather than the oldest.
        keep_in: Option<String>,
    },
    SameName,
    /// Files untouched for more than this many days (validated: at least 1).
    Old(u64),
    Similar {
        file: String,
        threshold: f64,
    },
}

/// Options that apply to the whole run, whatever the action (or the menu).
/// New flags of this kind go here, so the functions that receive them don't
/// change signature every time.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOptions {
    /// False with `--no-cache`.
    pub use_cache: bool,
    /// Folders left out of the scan (`--exclude`), as typed.
    pub exclude: Vec<String>,
}

impl Default for RunOptions {
    fn default() -> RunOptions {
        RunOptions {
            use_cache: true,
            exclude: Vec::new(),
        }
    }
}

/// What the program should do, as decided by the arguments.
#[derive(Debug, PartialEq)]
pub enum Command {
    /// No arguments: the classic interactive menus.
    Interactive,
    Help,
    /// Scan `path`; run `action` and exit, or open the action menu when `None`.
    Run {
        path: String,
        action: Option<Action>,
        options: RunOptions,
    },
}

/// Parses the arguments (without the program name). `Err` holds a usage error message.
pub fn parse_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::Interactive);
    }

    let mut path: Option<String> = None;
    let mut action: Option<Action> = None;
    let mut options = RunOptions::default();
    let mut trash = false;
    let mut assume_yes = false;
    let mut keep_in: Option<String> = None;
    let mut threshold: Option<f64> = None;

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        let new_action = match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--largest" => {
                // N is optional: only consume the next argument if it is a number
                let n = iter.peek().and_then(|v| v.parse::<usize>().ok());
                if n.is_some() {
                    iter.next();
                }
                Some(Action::Largest(n.unwrap_or(DEFAULT_TOP)))
            }
            "--ext" => Some(Action::Ext(take_value(&mut iter, "--ext")?)),
            "--dupes" => Some(Action::Dupes {
                trash: false,
                assume_yes: false,
                keep_in: None,
            }),
            "--same-name" => Some(Action::SameName),
            "--old" => {
                // DAYS is optional, like N in --largest
                let n = iter.peek().and_then(|v| v.parse::<u64>().ok());
                if n.is_some() {
                    iter.next();
                }
                let n = n.unwrap_or(DEFAULT_OLD_DAYS);
                if n == 0 {
                    return Err("--old needs at least 1 day".to_string());
                }
                if days(n).is_none() {
                    return Err(format!("--old {} is too many days", n));
                }
                Some(Action::Old(n))
            }
            "--similar" => Some(Action::Similar {
                file: take_value(&mut iter, "--similar")?,
                threshold: DEFAULT_THRESHOLD,
            }),
            "--threshold" => {
                let raw = take_value(&mut iter, "--threshold")?;
                let value = raw
                    .parse::<f64>()
                    .ok()
                    .filter(|v| (0.0..=100.0).contains(v))
                    .ok_or_else(|| {
                        format!("--threshold must be a number from 0 to 100, got '{}'", raw)
                    })?;
                threshold = Some(value);
                None
            }
            "--keep-in" => {
                keep_in = Some(take_value(&mut iter, "--keep-in")?);
                None
            }
            "--trash" => {
                trash = true;
                None
            }
            "--yes" | "-y" => {
                assume_yes = true;
                None
            }
            "--exclude" => {
                options.exclude.push(take_value(&mut iter, "--exclude")?);
                None
            }
            "--no-cache" => {
                options.use_cache = false;
                None
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option '{}'", flag)),
            positional => {
                if path.is_some() {
                    return Err(format!("unexpected extra argument '{}'", positional));
                }
                path = Some(positional.to_string());
                None
            }
        };

        if let Some(new_action) = new_action {
            if action.is_some() {
                return Err("only one action can be used at a time".to_string());
            }
            action = Some(new_action);
        }
    }

    // Attach modifiers to their action, rejecting them anywhere else
    let is_dupes = matches!(action, Some(Action::Dupes { .. }));
    if trash && !is_dupes {
        return Err("--trash can only be used with --dupes".to_string());
    }
    if keep_in.is_some() && !is_dupes {
        return Err("--keep-in can only be used with --dupes".to_string());
    }
    if assume_yes && !trash {
        return Err("--yes can only be used with --dupes --trash".to_string());
    }
    if threshold.is_some() && !matches!(action, Some(Action::Similar { .. })) {
        return Err("--threshold can only be used with --similar".to_string());
    }
    let action = match action {
        Some(Action::Dupes { .. }) => Some(Action::Dupes {
            trash,
            assume_yes,
            keep_in,
        }),
        Some(Action::Similar { file, .. }) => Some(Action::Similar {
            file,
            threshold: threshold.unwrap_or(DEFAULT_THRESHOLD),
        }),
        other => other,
    };

    let path = path.ok_or("missing <PATH> to scan")?;
    Ok(Command::Run {
        path,
        action,
        options,
    })
}

/// Takes the value that must follow `flag`, e.g. `--ext pdf`.
fn take_value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, String> {
    match iter.next() {
        Some(value) if !value.starts_with('-') => Ok(value.clone()),
        _ => Err(format!("{} needs a value", flag)),
    }
}

#[cfg(test)]
#[path = "tests/cli.rs"]
mod tests;
