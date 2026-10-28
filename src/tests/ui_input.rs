//! Unit tests for `src/ui/input.rs`, loaded there as its `tests` module.

use super::*;

#[test]
fn test_parse_selection() {
    assert_eq!(parse_selection("1", 5), Ok(vec![0]));
    assert_eq!(parse_selection("3,1", 5), Ok(vec![0, 2]));
    assert_eq!(parse_selection(" 1 , 3-5 ", 5), Ok(vec![0, 2, 3, 4]));
    assert_eq!(parse_selection("2-2,2", 5), Ok(vec![1]), "no repeats");
    for bad in ["", "0", "6", "a", "1-", "4-2", "1,,x", "-3"] {
        assert!(
            parse_selection(bad, 5).is_err(),
            "expected an error for '{}'",
            bad
        );
    }
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
