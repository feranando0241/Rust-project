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
│   ├── models.rs       FileInfo
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

1. Put the logic in `core/` (a new module, or an existing one if it fits). Return data or `Result`, and add a unit test.
2. Put the screen that shows the result in `ui/report.rs`.
3. Wire it up in `app.rs` if both the CLI and the menu use it. Otherwise wire it in `ui/menu.rs`.
4. If it is a CLI flag, add it to `cli.rs` and to `tests/cli.rs`.
