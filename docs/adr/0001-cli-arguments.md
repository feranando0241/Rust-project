# ADR-0001: Add command-line arguments alongside the interactive menus

**Status:** Accepted
**Date:** 2026-10-20
**Deciders:** M4DEN5K (project owner)

## Context

OxideClean is used only through interactive menus today. You pick a path, the app scans it, and then you choose actions from a numbered menu. That works well for exploring a folder you don't know. It is slow for repetitive tasks such as "show me the duplicates in ~/Downloads", which takes four inputs every time.

It also means the tool can't be scripted, piped or run from another program.

Forces at play:

- **Exploring vs. repeating.** Menus are good for discovery. Arguments are good for repeated tasks and automation.
- **Deletion is the only destructive action.** Any non-interactive path must never move files to the Trash without explicit consent.
- **Small codebase, three dependencies.** Each new crate adds compile time and maintenance. The project so far prefers the standard library.
- **Output is ready for pipes.** `formatter.rs` already turns colors off when stdout is not a terminal or when `NO_COLOR` is set.
- **The logic is already decoupled from the UI.** `scanner`, `actions` and `similarity` return data and `formatter` prints it. Only the `handle_*` functions in `main.rs` mix logic with stdin prompts.

## Decision

Add a small argument parser built on `std::env::args`. Running with no arguments keeps today's interactive behavior exactly as it is.

```
oxide_clean                                  # interactive menus (unchanged)
oxide_clean <PATH>                           # scan + summary, then the action menu
oxide_clean <PATH> --largest [N]             # top N largest files (default 10)
oxide_clean <PATH> --ext <EXT>               # files with that extension
oxide_clean <PATH> --dupes                   # list exact duplicates (read-only)
oxide_clean <PATH> --dupes --trash           # move duplicates to Trash, asks first
oxide_clean <PATH> --dupes --trash --yes     # same, without asking
oxide_clean <PATH> --same-name               # files sharing a name
oxide_clean <PATH> --similar <FILE> [--threshold N]
oxide_clean --help
```

When any action flag is given, the app runs that one action, prints the result and exits. It does not enter the menus.

Reassess and move to `clap` when any of the triggers listed under Consequences is reached.

## Options Considered

### Option A: Keep interactive menus only

| Dimension | Assessment |
|-----------|------------|
| Complexity | None, no change |
| Cost | None |
| Scalability | Poor: every new feature adds menu depth |
| Familiarity | Already in place |

**Pros:** no work, no new edge cases, no risk of deleting files without a prompt.
**Cons:** repeated tasks stay slow, the tool can't be scripted, and the menu grows with each feature.

### Option B: Hand-rolled parser with `std::env::args` + keep menus (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low: one `match` over the args, about 80 lines plus a help text |
| Cost | No new dependency |
| Scalability | Fine up to roughly 8 flags, then it gets brittle |
| Familiarity | Plain Rust, same style as the rest of the code |

**Pros:** no dependency, the interactive mode stays intact, and reuses the existing `actions` and `formatter` functions as they are.
**Cons:** `--help`, error messages and validation are written by hand. There are no shell completions. Parsing edge cases have to be tested by us (`--threshold` without a value, unknown flags, flag order).

### Option C: `clap` (derive API) + keep menus

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to write, but brings a large dependency tree |
| Cost | Noticeably longer clean builds and a larger binary |
| Scalability | Excellent: subcommands, validation, completions |
| Familiarity | De-facto standard in Rust CLIs |

**Pros:** generated `--help`, typed arguments, good errors and completions for free. It is the obvious choice once the CLI grows.
**Cons:** it becomes the largest dependency in the project for about 8 flags today, which is more than the current need.

### Option D: CLI only, remove the menus

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium: rewrite of `main.rs` |
| Cost | Loses the existing UX |
| Scalability | Good for scripting |
| Familiarity | Breaks how the tool is used today |

**Pros:** one code path.
**Cons:** loses the guided exploration that is the tool's main strength, and the paginated file picker and directory browser would no longer make sense.

## Trade-off Analysis

The real choice is between B and C. A doesn't solve the problem and D throws away working UX.

The interface needed today is small: one positional path, about six action flags, and two modifiers. A `match` over `std::env::args` handles that clearly and keeps the dependency list at three crates. `clap` pays off when there are many options per action, nested subcommands, or users who expect completions. None of that applies yet.

The deciding constraint is **safety, not parsing**. With either parser, the design must ensure that:

1. `--dupes` alone is always read-only.
2. `--trash` asks for confirmation unless `--yes` is also passed.
3. If stdin is not a terminal and `--yes` is missing, the app refuses to delete and exits with an error. It never waits on a prompt nobody will answer.

These rules are the same under B or C, so moving to `clap` later doesn't change the safety model.

## Consequences

**Easier**
- Repeated tasks become one command (`oxide_clean ~/Downloads --dupes`).
- Output can be piped (`| less`, `> report.txt`) and stays free of color codes.
- Actions can be tested end-to-end without simulating menu input.

**Harder**
- There are two entry points to keep in sync. Every new action needs a menu entry *and* a flag.
- Errors for bad flags, missing values or invalid numbers must be written and tested by hand.

**To revisit (move to `clap` when any of these is true)**
- More than about 8 flags, or an action that needs its own sub-options.
- A need for shell completions or a generated man page.
- Parsing bugs or a hand-written `--help` that keeps falling out of date.

## Action Items

1. [x] Split the `handle_*` functions in `main.rs` so the logic doesn't read stdin directly. The menu path and the CLI path then call the same functions, and only the menu path prompts.
2. [x] Add `parse_args()` returning a small `enum Command`, plus `--help` text and exit codes (0 ok, 1 runtime error, 2 usage error).
3. [x] Put the deletion rules above in one place: `--trash` needs a confirmation or `--yes`, and the app refuses when stdin is not a TTY.
4. [x] Add tests for `parse_args` (valid combinations, unknown flag, missing value). The refusal to delete without a TTY was verified by hand; there is no automated test because it would need a fake TTY.
5. [x] Update the README with a "Command-line usage" section.
