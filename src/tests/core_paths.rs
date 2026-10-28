//! Unit tests for `src/core/paths.rs`, loaded there as its `tests` module.

use super::*;
use crate::core::scanner::scan_directory;
use crate::core::test_support::{no_progress, temp_dir};
use std::fs;

#[test]
fn test_folder_that_does_not_exist_is_joined_to_root() {
    let root = Path::new("/no/such/scan");
    assert_eq!(
        folder_in_scan(root, Path::new("Photos")),
        Path::new("/no/such/scan/Photos")
    );
    // Matching is by path components, so a leading "./" doesn't matter
    assert!(
        Path::new("/no/such/scan/Photos/a.jpg")
            .starts_with(folder_in_scan(root, Path::new("./Photos")))
    );
    assert!(
        !Path::new("/no/such/scan/Photos2/a.jpg")
            .starts_with(folder_in_scan(root, Path::new("Photos")))
    );
}

#[cfg(unix)]
#[test]
fn test_other_spellings_of_a_folder_match_the_scanned_files() {
    // temp_dir() is usually behind a symlink on macOS (/var -> /private/var),
    // so this root is already a non-canonical spelling
    let root = temp_dir("folder_spellings");
    fs::create_dir_all(root.join("Photos/2024")).unwrap();
    fs::write(root.join("Photos/2024/a.jpg"), "image").unwrap();
    std::os::unix::fs::symlink(root.join("Photos"), root.join("Fotos")).unwrap();

    let scanned: Vec<PathBuf> = scan_directory(&root, &no_progress)
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    let spellings = [
        PathBuf::from("Photos"),
        PathBuf::from("Photos/../Photos"),
        PathBuf::from("Fotos"),
        root.join("Photos").canonicalize().unwrap(),
    ];
    let matches: Vec<bool> = spellings
        .iter()
        .map(|s| {
            let folder = folder_in_scan(&root, s);
            scanned
                .iter()
                .any(|p| p.ends_with("Photos/2024/a.jpg") && p.starts_with(&folder))
        })
        .collect();
    fs::remove_dir_all(&root).unwrap();

    for (spelling, matched) in spellings.iter().zip(matches) {
        assert!(
            matched,
            "'{}' should match the scanned photo",
            spelling.display()
        );
    }
}
