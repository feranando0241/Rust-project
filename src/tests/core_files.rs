//! Unit tests for `src/core/files.rs`, loaded there as its `tests` module.

use super::*;

fn file(path: &str, size_bytes: u64) -> FileInfo {
    FileInfo::from_path(PathBuf::from(path), size_bytes)
}

#[test]
fn test_queries() {
    let files = vec![
        file("a/report.PDF", 10),
        file("b/report.pdf", 30),
        file("b/photo.jpg", 20),
    ];

    let top: Vec<u64> = get_top_largest(&files, 2)
        .iter()
        .map(|f| f.size_bytes)
        .collect();
    assert_eq!(top, vec![30, 20]);

    assert_eq!(filter_by_extension(&files, ".pdf").len(), 2);
    assert!(is_document(&files[0]) && !is_document(&files[2]));

    let same_name = find_same_name_files(&files);
    assert_eq!(same_name.len(), 1);
    assert_eq!(same_name[0].shared_name, "report.pdf");

    let folders = group_by_folder(&files);
    assert_eq!(folders[&PathBuf::from("b")].len(), 2);
}

#[test]
fn test_usage_by_folder_and_type() {
    let root = Path::new("/scan");
    let files = vec![
        file("/scan/notes.txt", 5),
        file("/scan/photos/2024/a.jpg", 100),
        file("/scan/photos/b.jpg", 50),
        file("/scan/docs/c.txt", 20),
        file("/scan/README", 1),
    ];

    let by_folder = usage_by_top_folder(root, &files);
    assert_eq!(
        by_folder,
        vec![
            (
                Some("photos".to_string()),
                Usage {
                    bytes: 150,
                    files: 2
                }
            ),
            (
                Some("docs".to_string()),
                Usage {
                    bytes: 20,
                    files: 1
                }
            ),
            (None, Usage { bytes: 6, files: 2 }),
        ]
    );

    let by_type = usage_by_extension(&files);
    assert_eq!(
        by_type,
        vec![
            (
                Some("jpg".to_string()),
                Usage {
                    bytes: 150,
                    files: 2
                }
            ),
            (
                Some("txt".to_string()),
                Usage {
                    bytes: 25,
                    files: 2
                }
            ),
            (None, Usage { bytes: 1, files: 1 }),
        ]
    );
}
