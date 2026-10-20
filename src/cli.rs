//! Command-line argument parsing (see `docs/adr/0001-cli-arguments.md`).
//!
//! Hand-rolled on top of `std::env::args` to avoid a dependency while the
//! interface is small. Move to `clap` once the ADR's revisit triggers are hit.

use crate::core::files::DEFAULT_TOP;
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
      --trash              ...and move the copies to the Trash (asks first)
      --yes, -y            ...without asking
    --same-name            Files that share the same name
    --similar <FILE>       Documents with content similar to FILE
      --threshold <N>      Minimum similarity percentage (default 30)

OPTIONS:
    --no-cache             Don't reuse or save file hashes between runs
    -h, --help             Show this help

File hashes are cached in ~/Library/Caches/oxide_clean (macOS) or
~/.cache/oxide_clean, or under $XDG_CACHE_HOME if set, so a second scan of the
same folder skips unchanged files. Moving files to the Trash never relies on it.

Colors are disabled when the output is not a terminal or NO_COLOR is set.
";

/// A single non-interactive action requested on the command line.
#[derive(Debug, PartialEq)]
pub enum Action {
    Largest(usize),
    Ext(String),
    Dupes { trash: bool, assume_yes: bool },
    SameName,
    Similar { file: String, threshold: f64 },
}

/// What the program should do, as decided by the arguments.
#[derive(Debug, PartialEq)]
pub enum Command {
    /// No arguments: the classic interactive menus.
    Interactive,
    Help,
    /// Scan `path`; run `action` and exit, or open the action menu when `None`.
    /// `use_cache` is false with `--no-cache`.
    Run {
        path: String,
        action: Option<Action>,
        use_cache: bool,
    },
}

/// Parses the arguments (without the program name). `Err` holds a usage error message.
pub fn parse_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::Interactive);
    }

    let mut path: Option<String> = None;
    let mut action: Option<Action> = None;
    let mut trash = false;
    let mut assume_yes = false;
    let mut no_cache = false;
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
            }),
            "--same-name" => Some(Action::SameName),
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
            "--trash" => {
                trash = true;
                None
            }
            "--yes" | "-y" => {
                assume_yes = true;
                None
            }
            "--no-cache" => {
                no_cache = true;
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
    let action = match action {
        Some(Action::Dupes { .. }) => Some(Action::Dupes { trash, assume_yes }),
        Some(Action::Similar { file, .. }) => Some(Action::Similar {
            file,
            threshold: threshold.unwrap_or(DEFAULT_THRESHOLD),
        }),
        other => other,
    };
    if trash && !matches!(action, Some(Action::Dupes { .. })) {
        return Err("--trash can only be used with --dupes".to_string());
    }
    if assume_yes && !trash {
        return Err("--yes can only be used with --dupes --trash".to_string());
    }
    if threshold.is_some() && !matches!(action, Some(Action::Similar { .. })) {
        return Err("--threshold can only be used with --similar".to_string());
    }

    let path = path.ok_or("missing <PATH> to scan")?;
    Ok(Command::Run {
        path,
        action,
        use_cache: !no_cache,
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
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Command, String> {
        let args: Vec<String> = line.split_whitespace().map(String::from).collect();
        parse_args(&args)
    }

    fn run(path: &str, action: Option<Action>) -> Result<Command, String> {
        Ok(Command::Run {
            path: path.to_string(),
            action,
            use_cache: true,
        })
    }

    #[test]
    fn test_valid_commands() {
        assert_eq!(parse(""), Ok(Command::Interactive));
        assert_eq!(parse("~/Downloads --help"), Ok(Command::Help));
        assert_eq!(parse("."), run(".", None));
        assert_eq!(parse(". --largest"), run(".", Some(Action::Largest(10))));
        assert_eq!(parse(". --largest 3"), run(".", Some(Action::Largest(3))));
        assert_eq!(parse("--largest 3 ."), run(".", Some(Action::Largest(3))));
        assert_eq!(
            parse(". --ext pdf"),
            run(".", Some(Action::Ext("pdf".into())))
        );
        assert_eq!(parse(". --same-name"), run(".", Some(Action::SameName)));
        assert_eq!(
            parse(". --dupes"),
            run(
                ".",
                Some(Action::Dupes {
                    trash: false,
                    assume_yes: false
                })
            )
        );
        assert_eq!(
            parse(". --trash -y --dupes"),
            run(
                ".",
                Some(Action::Dupes {
                    trash: true,
                    assume_yes: true
                })
            )
        );
        assert_eq!(
            parse(". --similar a.txt --threshold 55"),
            run(
                ".",
                Some(Action::Similar {
                    file: "a.txt".into(),
                    threshold: 55.0
                })
            )
        );
        // --no-cache works with an action, and with a bare path (menu mode)
        assert_eq!(
            parse(". --dupes --no-cache"),
            Ok(Command::Run {
                path: ".".into(),
                action: Some(Action::Dupes {
                    trash: false,
                    assume_yes: false
                }),
                use_cache: false,
            })
        );
        assert_eq!(
            parse("--no-cache ."),
            Ok(Command::Run {
                path: ".".into(),
                action: None,
                use_cache: false
            })
        );
    }

    #[test]
    fn test_help_shows_the_real_defaults() {
        // HELP is a plain string, so it can't read the constants itself
        assert!(HELP.contains(&format!("(default {})", DEFAULT_TOP)));
        assert!(HELP.contains(&format!("(default {})", DEFAULT_THRESHOLD)));
    }

    #[test]
    fn test_usage_errors() {
        for bad in [
            ". --bogus",
            ". --ext",
            ". --ext --dupes",
            ". --dupes --same-name",
            ". --trash",
            ". --dupes --yes",
            ". --largest --threshold 10",
            ". --similar a.txt --threshold 150",
            ". --similar a.txt --threshold abc",
            ". other",
            "--dupes",
        ] {
            assert!(parse(bad).is_err(), "expected a usage error for '{}'", bad);
        }
    }
}
