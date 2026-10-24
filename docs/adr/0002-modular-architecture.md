# ADR-0002: Split the code into `core`, `ui` and `app` layers

**Status:** Accepted
**Date:** 2026-10-20
**Deciders:** M4DEN5K (project owner)

## Context

The project grew from a scanner into a tool with a command-line mode, interactive menus, duplicate removal and document comparison. By then the code had piled up in a few files:

- `main.rs` had 763 lines. It held argument dispatch, the start screen, the action menu, the folder browser, the paginated document picker, keyboard input, duplicate and Trash handling, and the tests for other modules.
- `formatter.rs` mixed terminal styling (colors, prompts) with result screens, and also held logic (`group_by_folder`).
- `similarity.rs` printed messages from inside the comparison logic.
- The list of document extensions was duplicated in two files.

The result was that changing how something *looks* meant editing files that also decide *what gets deleted*, and nothing kept those two concerns apart.

## Decision

Organize the source by responsibility, with one dependency rule:

```
src/
├── main.rs      entry point: parse args, pick a mode
├── cli.rs       command-line argument parsing
├── app.rs       actions shared by the CLI and the menus
├── core/        logic only — never prints, never reads input, never imports ui
│   ├── models.rs       FileInfo, FileId
│   ├── scanner.rs      directory walk + ignore rules
│   ├── duplicates.rs   size → 4 KB prefix → full hash, hardlinks, Trash
│   ├── files.rs        biggest, by type, same name, by folder, is_document
│   └── similarity.rs   text extraction + scoring
└── ui/          everything printed or typed
    ├── style.rs        colors, titles, prompts, text helpers
    ├── report.rs       result screens
    ├── input.rs        reading lines and paths
    ├── menu.rs         start screen + action menu
    ├── browser.rs      folder browser
    └── picker.rs       paginated document picker
```

**Rule:** `core` depends on nothing else in the crate. `ui` and `app` may use `core`. `main` wires everything together. A function that needs to print or ask something goes in `ui` or `app`, never in `core`.

`app` exists so that behavior shared by both modes is written once. The clearest example is the set of rules for moving files to the Trash: confirmation, refusing without a terminal, and the report afterwards.

## Options Considered

### Option A: Keep the flat layout, just split `main.rs`

**Pros:** smallest change.
**Cons:** the layers stay mixed. Logic would still print, and nothing would stop the next feature from mixing them again.

### Option B: Layers by responsibility (chosen)

**Pros:** each file has one job and stays small (the largest is about 330 lines). The rule is easy to check (`grep crate::ui src/core`). The logic is testable without a terminal.
**Cons:** more files, and a new feature touches 2–3 of them (`core` for the logic, `ui/report` to show it, `app` or `ui/menu` to wire it).

### Option C: Separate library crate + traits for output

**Pros:** the strongest separation, and it would allow other front-ends (GUI, web).
**Cons:** there is no second front-end. Traits with a single implementation and a workspace would add ceremony without solving a current problem. Revisit only if a second interface becomes real.

## Consequences

**Easier**
- Changing text or layout only touches `ui/`. The deletion logic in `core/duplicates.rs` is untouched.
- Each `core` module carries its own unit tests, next to the code.
- End-to-end tests in `tests/cli.rs` run the real binary and pin user-visible behavior. They were written *before* this refactor and passed unchanged after it.

**Harder**
- Adding a feature means knowing where each piece goes: logic in `core`, screens in `ui/report`, wiring in `app` or `ui/menu`.

**Behavior changes**
- One, on purpose: when the base document for `--similar` has no usable text, `core::similarity` now returns an error instead of printing a message itself. The CLI shows it as `error: …` and exits with code 1 (before: a message on stdout and exit 0). The menu shows it with the usual `✗` marker.

## How to add a feature

1. Put the logic in `core/` (a new module, or an existing one if it fits). Return data or a typed error, and add a unit test. Use `core/test_support.rs` for temp folders and the fake Trash.
2. Put the screen that shows the result in `ui/report.rs`. Wording for the user lives in `ui`: core returns enums such as `SkipReason`, not sentences.
3. Wire it up in `app.rs` if both the CLI and the menu use it. Otherwise wire it in `ui/menu.rs`.
4. If it is a CLI flag, add it to `cli.rs` and to `tests/cli.rs`.
5. Defaults (`DEFAULT_TOP`, `DEFAULT_THRESHOLD`, …) live next to the logic they configure in `core` and are imported by `cli` and `ui`. Never repeat the number.
6. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` before pushing. CI runs the same three.

## Amendment: second split and automatic enforcement (2026-10-20)

After the parallel hashing, hash cache and property-testing work, the layout had drifted:

- `core/duplicates.rs` reached 636 lines and did three jobs: hashing, detection and moving to the Trash.
- File identity (device + inode) was written three times, and `FileInfo` construction twice.
- `ui/report.rs` computed the "where the space goes" totals, which is logic living in the UI.
- Defaults (30 %, top 10) were repeated in `cli.rs` and `ui/menu.rs`.
- Test helpers were duplicated, and the property tests reached them through a `pub(super) mod tests` workaround.
- Formatting didn't match `rustfmt` in 124 places.

The rules were sound; nothing *checked* them. Changes:

- `duplicates.rs` was split into `hashing.rs` (how a hash is computed), `duplicates.rs` (detection) and `trash.rs` (verified removal).
- `models.rs` now owns `FileId` and `FileInfo::from_path`; `files.rs` owns `Usage` and the space totals; defaults live in `core`; test helpers live in `core/test_support.rs`.
- `cargo fmt` was applied once. The stopword list is the only `#[rustfmt::skip]`, because rustfmt can't place the per-language comments stably.
- **Enforcement:** `tests/architecture.rs` fails `cargo test` when `core/` imports `ui`/`app`/`cli`, when `core/` prints or reads input, or when a source file passes 500 lines. CI also runs `cargo fmt --check`. The rules now hold without anyone having to remember them.

## Amendment: unit tests in their own folder (2026-10-24)

Unit tests sat at the bottom of each production file, so files like `duplicates.rs` were 40 % tests, and reading the logic meant scrolling past them. They now live in `src/tests/<layer>_<module>.rs` (e.g. `core_duplicates.rs`), and each module loads its file with `#[cfg(test)] #[path = "…"] mod tests;`.

The top-level `tests/` folder was rejected for these: it only sees a crate's public API, so it would need a `lib.rs` and would make private items (such as `DuplicateGroup`'s `kept` field, which guarantees a group always keeps a copy) public or untestable. With `#[path]` the tests stay child modules and keep their access, and production code is unchanged.

`tests/architecture.rs` now also fails when a `#[test]` or `mod tests {` appears outside `src/tests/`, and when a file in `src/tests/` isn't loaded by any `#[path]` (its tests would never compile or run, and `cargo test` would still pass). Step 1 of "How to add a feature" means: add the unit test to `src/tests/`; the helpers are in `src/tests/core_test_support.rs`.

## Amendment: duplicates grouped in a folder (2026-10-24)

`hashing`, `hash_cache`, `duplicates` and `trash` (about 770 lines) form one feature: `hashing` was only used by detection and `trash`, and `trash` only acts on duplicate groups. They now live in `core/duplicates/` (detection in its `mod.rs`).

The point is encapsulation, not looks: `hashing` is private to the folder, so the compiler stops anything outside it from depending on how hashes are made. `hash_cache` and `trash` stay public because `app.rs` and `ui/` use them (`crate::core::duplicates::trash::…`). There are no re-exports, so each item has one path.

The rest of `core` stays flat: its modules are independent, and folders of one file would only add clicks. Group more when a feature has three or more files used mostly together, or when `core` passes about 12 files. Unit test files follow the module path: `src/tests/core_duplicates_trash.rs` for `core/duplicates/trash.rs`.
