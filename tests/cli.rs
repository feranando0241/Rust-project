//! End-to-end tests: run the real binary against a small fixture folder and
//! check what the user sees (output and exit codes). They pin down the
//! current behavior so internal refactors can't silently change it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Builds a fixture folder:
/// - one pair of identical files in different folders (a real duplicate)
/// - two similar text documents and one unrelated one
/// - files of different sizes for the "biggest files" ranking
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oxide_clean_it_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for sub in ["Videos", "Photos/2024", "Photos/backup", "Docs"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    fs::write(dir.join("Videos/clip.mp4"), vec![7u8; 30_000]).unwrap();
    fs::write(dir.join("Photos/2024/img.jpg"), vec![1u8; 10_000]).unwrap();
    fs::write(dir.join("Photos/backup/img.jpg"), vec![1u8; 10_000]).unwrap();
    fs::write(
        dir.join("Docs/report.txt"),
        "report about applied artificial intelligence in modern medicine",
    )
    .unwrap();
    fs::write(
        dir.join("Docs/draft.txt"),
        "report about applied artificial intelligence in medicine and health",
    )
    .unwrap();
    fs::write(dir.join("notes.md"), "groceries: apples bananas").unwrap();
    dir
}

/// Runs the binary with `args`, feeding `stdin` (not a terminal).
fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oxide_clean"))
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start oxide_clean");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn help_exits_ok() {
    let out = run(&["--help"], "");
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("USAGE:"));
}

#[test]
fn usage_errors_exit_2() {
    let out = run(&[".", "--bogus"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("unknown option '--bogus'"));

    let out = run(&[".", "--trash"], "");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn missing_folder_exits_1() {
    let out = run(&["/definitely/not/here", "--dupes"], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("is not a directory"));
}

#[test]
fn largest_lists_biggest_first() {
    let dir = fixture("largest");
    let out = run(&[path_str(&dir), "--largest", "2"], "");
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("The 2 biggest files"));
    let clip = text.find("clip.mp4").expect("clip.mp4 listed");
    let img = text.find("img.jpg").expect("img.jpg listed");
    assert!(clip < img, "biggest file must come first");
    assert!(!text.contains("notes.md"), "only 2 files requested");
}

#[test]
fn ext_filter_with_and_without_matches() {
    let dir = fixture("ext");
    let found = run(&[path_str(&dir), "--ext", ".txt"], "");
    let none = run(&[path_str(&dir), "--ext", "png"], "");
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(found.status.code(), Some(0));
    let text = stdout(&found);
    assert!(text.contains(".txt files: 2 found"));
    assert!(text.contains("report.txt") && text.contains("draft.txt"));

    assert_eq!(none.status.code(), Some(0));
    assert!(stdout(&none).contains("No .png files in this folder."));
}

#[test]
fn dupes_lists_groups_without_deleting() {
    let dir = fixture("dupes");
    let out = run(&[path_str(&dir), "--dupes"], "");
    let both_exist = dir.join("Photos/2024/img.jpg").exists() && dir.join("Photos/backup/img.jpg").exists();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("Identical files"));
    assert!(text.contains("keep ") && text.contains("extra"));
    assert!(text.contains("1 extra copy in 1 group"));
    assert!(both_exist, "--dupes alone must never delete");
}

#[test]
fn trash_without_terminal_refuses_and_keeps_files() {
    let dir = fixture("trash_refuse");
    let out = run(&[path_str(&dir), "--dupes", "--trash"], "y\n");
    let both_exist = dir.join("Photos/2024/img.jpg").exists() && dir.join("Photos/backup/img.jpg").exists();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Nothing was moved"));
    assert!(both_exist, "without a terminal and without --yes nothing may be trashed");
}

#[test]
fn similar_finds_related_document_and_skips_itself() {
    let dir = fixture("similar");
    let base = dir.join("Docs/report.txt");
    let out = run(&[path_str(&dir), "--similar", path_str(&base)], "");
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("Documents similar to 'report.txt'"));
    assert!(text.contains("draft.txt"));
    let results = text.split("loosely related").nth(1).unwrap_or("");
    assert!(!results.contains("report.txt"), "the base document must not match itself");
    assert!(!results.contains("notes.md"), "unrelated document must not match");
}

#[test]
fn interactive_quit() {
    let out = run(&[], "q\n");
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("Which folder do you want to look at?"));
    assert!(text.contains("Bye!"));
}

#[test]
fn interactive_scan_menu_and_back() {
    let dir = fixture("interactive");
    let input = format!("{}\n2\n6\nx\n0\nq\n", path_str(&dir));
    let out = run(&[], &input);
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("Scan complete"));
    assert!(text.contains("What would you like to do?"));
    assert!(text.contains("biggest files"));
    assert!(text.contains("Files with the same name"));
    assert!(stderr(&out).contains("That's not an option"));
    assert!(text.contains("Bye!"));
}

#[test]
fn interactive_end_of_input_exits() {
    // Closed stdin in the middle of the menu must exit, not loop forever
    let dir = fixture("eof");
    let out = run(&[path_str(&dir)], "");
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(out.status.code(), Some(0));
}
