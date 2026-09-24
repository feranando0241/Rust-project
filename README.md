# OxideClean

A fast, modular file scanner and organizer built in Rust. OxideClean helps you understand what is taking up space on your disk, find duplicate files, detect similar documents, and keep your folders clean — all from the terminal.

---

## Features

| # | Feature | Description |
|---|---------|-------------|
| 1 | **File tree view** | Recursive scan grouped by folder, with sizes and extensions |
| 2 | **Top 10 largest files** | Quickly spot what is eating your disk space |
| 3 | **Filter by extension** | Find all `.pdf`, `.mp4`, `.zip`, or any other type |
| 4 | **Exact duplicate detection** | SHA-256 hash comparison — finds byte-for-byte identical files and moves them to Trash |
| 5 | **Directory browser** | Interactive navigator to explore subdirectories before scanning |
| 6 | **Document similarity** | Composite content analysis (vocabulary + bigrams + length) to find similar PDFs, text files, and source code |
| 7 | **Same-name file finder** | Groups files that share a filename across different folders |

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

# Install to your local bin (make sure ~/.local/bin is in your PATH)
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

## Usage

When launched, OxideClean presents a startup prompt:

```
OxideClean — File Organizer & Scanner
=========================================

Start options:
   - Type a path to scan (e.g. ~/Downloads, . for current)
   - Type 'dirs' or 'ls' to browse your accessible folders
   - Type '0' or 'exit' to quit
Enter your option or path:
```

### Quick start examples

```
~/Downloads     — scan your Downloads folder
.               — scan the current directory
dirs            — browse folders interactively
```

After scanning, you are presented with the action menu:

```
┌────────────────────────────────────────────────────────────┐
│ What do you want to do with this folder?                   │
│ [1] View full file tree                                    │
│ [2] View Top 10 largest files                              │
│ [3] Filter by type / extension (e.g. pdf, png)             │
│ [4] Find duplicates and move to Trash                      │
│ [5] View accessible subdirectories in this path            │
│ [6] Analyze content similarity between documents           │
│ [7] Find files with the same name                          │
│ [0] Back to main menu / Scan another folder                │
└────────────────────────────────────────────────────────────┘
```

---

## How the similarity engine works

Option **[6]** compares documents using a **Composite Score**:

| Weight | Metric | What it measures |
|--------|--------|-----------------|
| 50% | Vocabulary (Jaccard) | Shared unique keywords between documents |
| 40% | Bigrams (Jaccard) | Shared 2-word phrases — indicates shared structure |
| 10% | Length ratio | Penalizes comparing a 10-word file to a 10,000-word file |

**Supported file types:** PDF, TXT, MD, JSON, CSV, RS, PY, JS, C, CPP, DOCX

**Threshold guide:**

| Range | Meaning |
|-------|---------|
| >= 70% | Very similar — same topic, structure, and phrasing |
| 40-69% | Related — shared vocabulary, different focus |
| 15-39% | Somewhat related — share some terms in the field |
| < 15% | Distant — very little content overlap |

---

## How duplicate detection works

Option **[4]** uses a two-step approach for accuracy and speed:

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
