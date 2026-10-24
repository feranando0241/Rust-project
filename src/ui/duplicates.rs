//! The interactive choices after the duplicate groups are shown: move all of
//! them, only some groups, or first pick a folder whose copies should be kept.

use crate::core::duplicates::{DuplicateGroup, keep_copies_inside};
use crate::ui::input::{folder_in_scan, parse_selection, read_line_from_user};
use crate::ui::report::print_exact_duplicates;
use crate::ui::style::{bold, error, hint, plural, prompt};
use std::path::Path;

/// Asks what to do with `groups` until the user decides. Returns the groups
/// to move to the Trash, or `None` to leave everything as it is. Choosing a
/// folder to keep (`k <folder>`) updates `groups` and shows them again.
pub fn choose_groups_to_trash(
    root: &Path,
    groups: &mut [DuplicateGroup],
) -> Option<Vec<DuplicateGroup>> {
    loop {
        println!("\n{}", bold("What would you like to do?"));
        println!("  • Type y              to move every extra copy to the Trash");
        println!("  • Type group numbers  to move only those groups (e.g. 1,3,5-7)");
        println!("  • Type k <folder>     to keep the copies inside a folder (e.g. k Photos)");
        println!("  • Press Enter         to leave everything as it is");
        hint("  Moved files can be restored from the Trash.");
        prompt();

        let answer = read_line_from_user();
        if answer.is_empty() {
            return None;
        }
        if ["y", "yes", "all"]
            .iter()
            .any(|c| answer.eq_ignore_ascii_case(c))
        {
            return Some(groups.to_vec());
        }
        if let Some(folder) = answer.strip_prefix("k ").map(str::trim) {
            keep_copies_in(root, groups, folder);
            continue;
        }

        match parse_selection(&answer, groups.len()) {
            Ok(indices) => {
                let selected: Vec<DuplicateGroup> =
                    indices.iter().map(|&i| groups[i].clone()).collect();
                let copies: usize = selected.iter().map(|g| g.extra_copies()).sum();
                hint(&format!(
                    "Moving {} from {}.",
                    plural(copies, "extra copy", "extra copies"),
                    plural(selected.len(), "group", "groups")
                ));
                return Some(selected);
            }
            Err(message) => error(&format!(
                "{} Type y, group numbers, k <folder>, or press Enter.",
                message
            )),
        }
    }
}

/// Switches every group that has copies inside `folder` to keep all of them,
/// then shows the groups again so the user sees what changed.
fn keep_copies_in(root: &Path, groups: &mut [DuplicateGroup], folder: &str) {
    let path = folder_in_scan(root, folder);
    if !path.is_dir() {
        error(&format!("'{}' is not a folder here.", folder));
        return;
    }
    let found = keep_copies_inside(groups, &path);
    if found == 0 {
        error(&format!(
            "No group has a copy inside '{}'. Nothing changed.",
            folder
        ));
        return;
    }
    print_exact_duplicates(root, groups);
    hint(&format!(
        "Keeping every copy inside '{}' in {} of {}.",
        folder,
        found,
        plural(groups.len(), "group", "groups")
    ));
}
