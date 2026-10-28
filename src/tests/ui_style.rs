//! Unit tests for `src/ui/style.rs`, loaded there as its `tests` module.

use super::*;

#[test]
fn test_format_sizes() {
    assert_eq!(format_size(500), "500 B");
    assert_eq!(format_size(2048), "2.00 KB");
    assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
    assert_eq!(format_size(10 * 1024 * 1024 * 1024), "10.00 GB");
}

#[test]
fn test_format_age() {
    let days = |n: u64| Duration::from_secs(n * 86_400);
    assert_eq!(format_age(days(1)), "1 day");
    assert_eq!(format_age(days(59)), "59 days");
    assert_eq!(format_age(days(60)), "2 months");
    assert_eq!(format_age(days(200)), "6 months");
    assert_eq!(format_age(days(729)), "24 months");
    assert_eq!(format_age(days(730)), "2 years");
}
