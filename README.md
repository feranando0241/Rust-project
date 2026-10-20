# OxideClean

A fast, modular file scanner and organizer built in Rust. OxideClean helps you understand what is taking up space on your disk, find duplicate files, detect similar documents, and keep your folders clean — all from the terminal.

---

## Features

| # | Feature | Description |
|---|---------|-------------|
| 1 | **File tree view** | Recursive scan grouped by folder, with sizes and extensions |
| 2 | **Top 10 largest files** | Quickly spot what is eating your disk space |
| 3 | **Filter by extension** | Find all `.pdf`, `.mp4`, `.zip`, or any other type |
| 4 | **Folder listing** | List the folders inside the scanned one (a folder browser is also available before scanning) |
| 5 | **Identical copies** | SHA-256 hash comparison — finds byte-for-byte identical files and moves the extra copies to Trash |
| 6 | **Same-name file finder** | Groups files that share a filename across different folders |
| 7 | **Document similarity** | Composite content analysis (vocabulary + bigrams + length) to find similar PDFs, text files, and source code |

---

## Installation

### Requirements

- **Rust** (1.70 or later) — installed via [Homebrew](https://brew.sh) or [rustup](https://rustup.rs)
- **macOS** (tested on Apple Silicon and Intel)

### Build and install

```bash
# Clone or open the project folder
cd ~/path/to/Rust\ project

# Build the optimized binary
cargo build --release

# Install to your local bin (make sure ~/.local/bin is in your PATH).
# Remove the old copy first: on macOS, overwriting a binary in place makes
# the system kill it on launch (exit code 137).
rm -f ~/.local/bin/oxide_clean
cp target/release/oxide_clean ~/.local/bin/oxide_clean
```

Add to your shell if not already there:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### Run

```bash
oxide_clean
```

---

## Command-line usage

Run without arguments for the interactive menus, or pass a path and one action to run it directly and exit:

```bash
oxide_clean ~/Downloads                      # scan, show summary, open the action menu
oxide_clean ~/Downloads --largest 20         # top 20 largest files (default 10)
oxide_clean ~/Downloads --ext pdf            # all PDF files
oxide_clean ~/Downloads --dupes              # list exact duplicates (read-only)
oxide_clean ~/Downloads --dupes --trash      # move duplicate copies to Trash (asks first)
oxide_clean ~/Downloads --dupes --trash -y   # same, without asking
oxide_clean ~/Downloads --same-name          # files that share a name
oxide_clean ~/Documents --similar thesis.pdf --threshold 50
oxide_clean --help
```

- `--dupes` on its own never deletes anything.
- `--trash` always asks for confirmation unless `--yes`/`-y` is given. Without a terminal to ask on (e.g. in a script), it refuses and exits with an error.
- Output can be piped or redirected (`> report.txt`); colors turn off automatically.
- Exit codes: `0` success, `1` runtime error, `2` invalid arguments.

See [ADR-0001](docs/adr/0001-cli-arguments.md) for the design rationale.

---

## Usage (interactive)

When launched, OxideClean presents a startup prompt:

```
OxideClean
See what's using space, find identical copies and similar documents.

Which folder do you want to look at?
  • Type its path       e.g. ~/Downloads, or . for the current folder
  • Type b              to browse your folders
  • Type q              to quit
›
```

After scanning, you get a short summary of where the space goes, then the action menu:

```
What would you like to do?

  Look around
    1  See every file, folder by folder
    2  See the 10 biggest files
    3  See only one type of file (pdf, jpg, mp4…)
    4  List the folders inside this one

  Clean up
    5  Find identical copies of files (and free up space)
    6  Find files with the same name in different places

  Compare documents
    7  Find documents with similar content

    0  Choose another folder        ?  Show this menu again
```

After the first action, the menu shrinks to a two-line reminder; type `?` to see the full one again.

---

## How the similarity engine works

Option **[7]** compares documents using a **Composite Score**:

| Weight | Metric | What it measures |
|--------|--------|-----------------|
| 50% | Vocabulary (Jaccard) | Shared unique keywords between documents |
| 40% | Bigrams (Jaccard) | Shared 2-word phrases — indicates shared structure |
| 10% | Length ratio | Penalizes comparing a 10-word file to a 10,000-word file |

**Supported file types:** PDF, TXT, MD, JSON, CSV, RS, PY, JS, C, CPP

**Threshold guide:**

| Range | Meaning |
|-------|---------|
| >= 70% | Very similar — same topic, structure, and phrasing |
| 40-69% | Related — shared vocabulary, different focus |
| 15-39% | Somewhat related — share some terms in the field |
| < 15% | Distant — very little content overlap |

---

## How duplicate detection works

Option **[5]** uses a two-step approach for accuracy and speed:

1. **Group by file size** — files with different sizes cannot be identical (instant filter, zero I/O cost).
2. **SHA-256 hash comparison** — only files with the same size are hashed; identical hashes mean byte-for-byte duplicates.

Duplicates are listed with the space that would be recovered, and you can choose to move them to the macOS Trash (recoverable).

---

## Ignored directories and files

OxideClean automatically skips folders and files that generate noise:

**Ignored directories:** `.git`, `target`, `node_modules`, `.cache`, `.vscode`, `.idea`, `.Trash`, `venv`, `.venv`, `env`, `__pycache__`, `Library`, `dist`, `build`, `.next`, `.nuxt`, `out`, `site-packages`, `.npm`, `.yarn`, `.pnpm-store`, `.mypy_cache`, `.pytest_cache`, `.ruff_cache`, `.tox` — plus any folder ending in `_venv`, `-venv`, `-env`, `.app`, `.framework`, or `.bundle`.

**Ignored files:** `.DS_Store`, `.localized`, `Thumbs.db`, `desktop.ini`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `Pipfile.lock`, `poetry.lock` — plus files with extensions `.pyc`, `.pyo`, `.class`, `.o`.

---

## Project structure

```
src/
├── main.rs        — Entry point, menus, and user interaction handlers
├── models.rs      — FileInfo struct (path, size, extension)
├── scanner.rs     — Recursive directory scanning and ignore rules
├── actions.rs     — Hashing, duplicates, filtering, same-name detection
├── formatter.rs   — All terminal output and display functions
└── similarity.rs  — Document similarity engine (Jaccard + bigrams)
```

---

## Running the tests

```bash
cargo test
```

The test suite covers:

- `test_format_sizes` — byte formatting (B, KB, MB, GB)
- `test_is_exit_command` — exit command recognition
- `test_should_ignore_directory` — ignore-rules logic
- `test_similarity_algorithm` — Jaccard similarity calculation
- `test_duplicates_keep_oldest` — the oldest copy is the one kept
- `cli::tests` — command-line parsing: valid combinations and usage errors

---

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `sha2` | 0.10 | SHA-256 hashing for duplicate detection |
| `trash` | 5.2 | Cross-platform safe move-to-Trash |
| `pdf-extract` | 0.8 | Text extraction from PDF files |

---

## License

This project is open source and available under the MIT License.
