//! Unit tests for `src/core/scanner.rs`, loaded there as its `tests` module.

use super::*;

#[test]
fn test_should_ignore_directory() {
    assert!(should_ignore_directory("node_modules"));
    assert!(should_ignore_directory("target"));
    assert!(should_ignore_directory(".git"));
    assert!(should_ignore_directory(".cache"));
    assert!(!should_ignore_directory("photos"));
    assert!(!should_ignore_directory("documents"));
}
