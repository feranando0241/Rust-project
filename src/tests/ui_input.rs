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
fn test_folder_in_scan() {
    let root = Path::new("/scan");
    assert_eq!(folder_in_scan(root, "Photos"), Path::new("/scan/Photos"));
    assert_eq!(
        folder_in_scan(root, "./Photos/2024"),
        Path::new("/scan/./Photos/2024")
    );
    // Matching is by path components, so "./" doesn't matter
    assert!(
        Path::new("/scan/Photos/2024/a.jpg").starts_with(folder_in_scan(root, "./Photos/2024"))
    );
    assert!(!Path::new("/scan/Photos2/a.jpg").starts_with(folder_in_scan(root, "Photos")));
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
