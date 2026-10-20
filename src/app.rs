//! Actions shared by the command line and the interactive menus: each one
//! calls the core logic and shows the result. Anything that must behave the
//! same in both modes (especially the rules for moving files to the Trash)
//! lives here, in one place.

use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use crate::cli::Action;
use crate::core::duplicates::{find_exact_duplicates, trash_duplicates};
use crate::core::files::{filter_by_extension, find_same_name_files, get_top_largest};
use crate::core::hash_cache::HashCache;
use crate::core::models::FileInfo;
use crate::core::scanner::scan_directory;
use crate::core::similarity::find_similar_documents;
use crate::ui::input::{read_line_from_user, resolve_user_path};
use crate::ui::report::{
    print_exact_duplicates, print_filtered_files, print_same_name_results,
    print_similarity_results, print_top_largest, print_trash_report,
};
use crate::ui::progress::Spinner;
use crate::ui::style::{hint, plural, prompt};

/// Resolves a folder given on the command line, expanding `~`.
pub fn resolve_folder(path: &str) -> Result<PathBuf, String> {
    let root = resolve_user_path(path);
    if root.is_dir() {
        Ok(root)
    } else {
        Err(format!("'{}' is not a directory", root.display()))
    }
}

/// Scans `root` recursively, showing a spinner with the files found so far.
pub fn scan(root: &Path) -> io::Result<Vec<FileInfo>> {
    let spinner = Spinner::start("Scanning", "files found");
    scan_directory(root, &|done, total| spinner.update(done, total))
}

/// Where the hash cache is stored: under `$XDG_CACHE_HOME` if set, otherwise
/// `~/Library/Caches` on macOS (Apple's place for app caches) and `~/.cache`
/// elsewhere. `None` if there is no home folder to put it in.
fn cache_file() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = PathBuf::from(std::env::var_os("HOME")?);
            if cfg!(target_os = "macos") {
                home.join("Library/Caches")
            } else {
                home.join(".cache")
            }
        }
    };
    Some(base.join("oxide_clean").join("hashes.tsv"))
}

/// Scans `root` and runs one command-line action on it.
pub fn run_action(root: &Path, action: Action, use_cache: bool) -> Result<(), String> {
    let files = scan(root).map_err(|e| format!("scan failed: {}", e))?;

    match action {
        Action::Largest(n) => print_top_largest(root, &get_top_largest(&files, n)),
        Action::Ext(ext) => show_files_by_extension(root, &files, &ext),
        Action::Dupes { trash, assume_yes } => {
            handle_duplicates(root, &files, trash, assume_yes, use_cache)?
        }
        Action::SameName => show_same_name_files(root, &files),
        Action::Similar { file, threshold } => {
            let target = locate_base_file(root, &file)?;
            show_similar_documents(root, &target, &files, threshold)?;
        }
    }
    Ok(())
}

/// Builds the `FileInfo` for the `--similar` base file. When the file lives
/// inside `root`, its path is expressed the same way the scanner builds paths
/// (root + relative part) so the comparison recognizes and skips it.
fn locate_base_file(root: &Path, file: &str) -> Result<FileInfo, String> {
    let path = resolve_user_path(file);
    let metadata = fs::metadata(&path).map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a file", path.display()));
    }

    let path = match (path.canonicalize(), root.canonicalize()) {
        (Ok(abs), Ok(root_abs)) => match abs.strip_prefix(&root_abs) {
            Ok(rel) => root.join(rel),
            Err(_) => abs,
        },
        _ => path,
    };
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase());

    Ok(FileInfo { path, size_bytes: metadata.len(), extension })
}

/// Prints the files matching an extension, or a notice when there are none.
pub fn show_files_by_extension(root: &Path, files: &[FileInfo], ext: &str) {
    // Accept both "pdf" and ".pdf"
    let ext = ext.trim().trim_start_matches('.');
    let filtered = filter_by_extension(files, ext);
    if filtered.is_empty() {
        println!("No .{} files in this folder.", ext);
    } else {
        print_filtered_files(root, ext, &filtered);
    }
}

/// Finds and displays files that share the same name within the scanned folder.
///
/// Unlike identical copies, content is not analyzed here: two files with the
/// same name but different content (e.g. different versions of a document)
/// will also appear in the list.
pub fn show_same_name_files(root: &Path, files: &[FileInfo]) {
    let groups = find_same_name_files(files);
    print_same_name_results(root, &groups);
}

/// Compares `target` with the other documents and prints the matches.
pub fn show_similar_documents(
    root: &Path,
    target: &FileInfo,
    files: &[FileInfo],
    threshold: f64,
) -> Result<(), String> {
    let matches = {
        let spinner = Spinner::start("Reading documents", "documents");
        find_similar_documents(target, files, threshold, &|done, total| spinner.update(done, total))?
    };
    print_similarity_results(root, target.name(), &matches, threshold);
    Ok(())
}

/// Runs the SHA-256 duplicate analysis and prints the groups.
///
/// With `offer_trash`, the redundant copies are then moved to the Trash after
/// a y/N confirmation, or straight away with `assume_yes`. Without a terminal
/// to ask on and without `assume_yes`, nothing is deleted and an error is returned.
///
/// With `use_cache`, hashes of unchanged files are reused from earlier runs and
/// the cache is updated afterwards. A cache that can't be saved is not an error.
pub fn handle_duplicates(
    root: &Path,
    files: &[FileInfo],
    offer_trash: bool,
    assume_yes: bool,
    use_cache: bool,
) -> Result<(), String> {
    hint("\nComparing file contents …");
    let cache_path = if use_cache { cache_file() } else { None };
    let cache = match &cache_path {
        Some(path) => HashCache::load(path),
        None => HashCache::disabled(),
    };

    let duplicate_groups = {
        let spinner = Spinner::start("Comparing file contents", "files");
        find_exact_duplicates(files, &cache, &|done, total| spinner.update(done, total))
    };

    if let Some(path) = &cache_path
        && let Err(err) = cache.save(path)
    {
        hint(&format!("(Couldn't save the hash cache at {}: {})", path.display(), err));
    }
    if cache.hits() > 0 {
        hint(&format!(
            "Reused {} saved {} for files that haven't changed.",
            cache.hits(),
            if cache.hits() == 1 { "hash" } else { "hashes" }
        ));
    }

    if duplicate_groups.is_empty() {
        println!("No identical copies found. Nothing to clean up here.\n");
        return Ok(());
    }

    print_exact_duplicates(root, &duplicate_groups);

    if !offer_trash {
        return Ok(());
    }

    let copies: usize = duplicate_groups.iter().map(|g| g.duplicates.len()).sum();

    if !assume_yes {
        if !io::stdin().is_terminal() {
            return Err(
                "Nothing was moved: there's no terminal to ask for confirmation \
                 (pass --yes to skip it)"
                    .to_string(),
            );
        }

        println!(
            "Move the {} to the Trash? You can restore {} from there later.",
            plural(copies, "extra copy", "extra copies"),
            if copies == 1 { "it" } else { "them" }
        );
        hint("  Type y to move them, or press Enter to leave everything as it is.");
        prompt();
        let confirmation = read_line_from_user();
        if !(confirmation.eq_ignore_ascii_case("y") || confirmation.eq_ignore_ascii_case("yes")) {
            println!("Nothing was moved.\n");
            return Ok(());
        }
    }

    let report = trash_duplicates(&duplicate_groups);
    print_trash_report(root, &report);

    if report.failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} file(s) could not be moved to the Trash", report.failed.len()))
    }
}
