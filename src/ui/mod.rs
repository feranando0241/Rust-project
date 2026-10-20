//! Terminal user interface: everything that is printed or typed.
//!
//! - `style`   colors, titles, prompts and text helpers
//! - `report`  result screens
//! - `input`   reading lines and paths
//! - `menu`    start screen and action menu
//! - `browser` folder browser
//! - `picker`  paginated document picker
//! - `progress` spinning loading indicator with a live counter

pub mod browser;
pub mod input;
pub mod menu;
pub mod picker;
pub mod progress;
pub mod report;
pub mod style;
