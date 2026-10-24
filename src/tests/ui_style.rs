//! Unit tests for `src/ui/style.rs`, loaded there as its `tests` module.

use super::*;

#[test]
fn test_format_sizes() {
    assert_eq!(format_size(500), "500 B");
    assert_eq!(format_size(2048), "2.00 KB");
    assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
    assert_eq!(format_size(10 * 1024 * 1024 * 1024), "10.00 GB");
}
