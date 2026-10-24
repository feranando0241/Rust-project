//! Unit tests for `src/cli.rs`, loaded there as its `tests` module.

use super::*;

fn parse(line: &str) -> Result<Command, String> {
    let args: Vec<String> = line.split_whitespace().map(String::from).collect();
    parse_args(&args)
}

fn run(path: &str, action: Option<Action>) -> Result<Command, String> {
    Ok(Command::Run {
        path: path.to_string(),
        action,
        options: RunOptions::default(),
    })
}

fn dupes(trash: bool, assume_yes: bool, keep_in: Option<&str>) -> Option<Action> {
    Some(Action::Dupes {
        trash,
        assume_yes,
        keep_in: keep_in.map(String::from),
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
    assert_eq!(parse(". --dupes"), run(".", dupes(false, false, None)));
    assert_eq!(
        parse(". --trash -y --dupes"),
        run(".", dupes(true, true, None))
    );
    assert_eq!(
        parse(". --dupes --keep-in Photos --trash"),
        run(".", dupes(true, false, Some("Photos")))
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
}

#[test]
fn test_run_options_work_with_any_action_and_the_menu() {
    assert_eq!(
        parse(". --dupes --no-cache --exclude Backups --exclude old/stuff"),
        Ok(Command::Run {
            path: ".".into(),
            action: dupes(false, false, None),
            options: RunOptions {
                use_cache: false,
                exclude: vec!["Backups".into(), "old/stuff".into()],
            },
        })
    );
    assert_eq!(
        parse("--exclude Backups ."),
        Ok(Command::Run {
            path: ".".into(),
            action: None,
            options: RunOptions {
                use_cache: true,
                exclude: vec!["Backups".into()],
            },
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
        ". --largest --keep-in Photos",
        ". --dupes --keep-in",
        ". --exclude",
        ". other",
        "--dupes",
    ] {
        assert!(parse(bad).is_err(), "expected a usage error for '{}'", bad);
    }
}
