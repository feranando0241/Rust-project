//! Paginated, searchable list for picking one document.

use crate::core::models::FileInfo;
use crate::ui::input::read_line_from_user;
use crate::ui::style::{bold, dim, error, folder_of, hint, plural, prompt, truncate};
use std::path::Path;

/// Displays a paginated file list (20 per page) and lets the user
/// navigate pages or search by name before selecting.
///
/// Returns the chosen file, or `None` if the user cancels.
pub fn select_file_paginated<'a>(root: &Path, files: &[&'a FileInfo]) -> Option<&'a FileInfo> {
    const PAGE_SIZE: usize = 20;
    let total = files.len();
    // total_pages is fixed: used in the header when no search is active.
    // page_total is recalculated each iteration to reflect the active list (may be filtered).
    let total_pages = total.div_ceil(PAGE_SIZE);
    let mut current_page: usize = 0;
    // Active list: may be the full listing or a search result.
    let mut active_list: Vec<&FileInfo> = files.to_vec();
    let mut search_active = false;

    loop {
        let page_total = active_list.len().div_ceil(PAGE_SIZE).max(1);
        let page = current_page.min(page_total - 1);
        let start = page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(active_list.len());
        let page_slice = &active_list[start..end];

        // Page header
        let shown = if search_active {
            format!("{} matching your search", active_list.len())
        } else {
            plural(total, "document", "documents")
        };
        let page_info = if page_total > 1 {
            format!(
                " · page {} of {}",
                page + 1,
                if search_active {
                    page_total
                } else {
                    total_pages
                }
            )
        } else {
            String::new()
        };
        hint(&format!("\n{}{}", shown, page_info));

        for (i, f) in page_slice.iter().enumerate() {
            println!(
                "  {:>3}  {:<34}  {}",
                start + i + 1,
                truncate(f.name(), 34),
                dim(&truncate(&folder_of(f, root), 30)),
            );
        }
        println!();

        println!("  • Type a number   to pick that document");
        // Only show pagination options when there is more than one page
        if page + 1 < page_total {
            println!("  • Type n          for the next page");
        }
        if page > 0 {
            println!("  • Type p          for the previous page");
        }
        if search_active {
            println!("  • Type r          to show all documents again");
        } else {
            println!("  • Type s          to search by name");
        }
        println!("  • Type 0          to cancel");
        prompt();

        let input = read_line_from_user();

        match input.to_lowercase().as_str() {
            "0" | "cancel" => return None,

            "n" if page + 1 < page_total => {
                current_page = page + 1;
            }
            "p" if page > 0 => {
                current_page = page - 1;
            }

            "s" | "b" => {
                println!("Type part of the name:");
                prompt();
                let query = read_line_from_user().to_lowercase();
                if query.is_empty() {
                    hint("Empty search, showing all documents.");
                    active_list = files.to_vec();
                    search_active = false;
                } else {
                    active_list = files
                        .iter()
                        .filter(|f| f.name().to_lowercase().contains(&query))
                        .copied()
                        .collect();
                    search_active = true;
                    current_page = 0;

                    if active_list.is_empty() {
                        error(&format!(
                            "No document name contains '{}'. Showing all again.",
                            query
                        ));
                        active_list = files.to_vec();
                        search_active = false;
                    }
                }
            }

            "r" => {
                active_list = files.to_vec();
                search_active = false;
                current_page = 0;
            }

            raw => {
                // Try to interpret as a selection number
                if let Ok(num) = raw.parse::<usize>() {
                    if num >= 1 && num <= active_list.len() {
                        let chosen = active_list[num - 1];
                        println!("Picked: {}", bold(chosen.name()));
                        return Some(chosen);
                    } else {
                        error(&format!("Pick a number from 1 to {}.", active_list.len()));
                    }
                } else {
                    error("That's not an option. Type a number, or one of the letters shown.");
                }
            }
        }
    }
}
