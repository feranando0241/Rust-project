# ADR-0003: Persistent hash cache for duplicate detection

**Status:** Accepted
**Date:** 2026-10-20
**Deciders:** M4DEN5K (project owner)

## Context

Finding duplicates means reading files. After the prefix filter and parallel hashing, a 3.2 GB test folder still took about 2.5 s, and every run repeated the same work even when nothing had changed. Scanning the same folder more than once (look, decide, come back) is the common case.

A cache of hashes removes that cost. It is also the one feature that can make the tool *wrong*: a stale hash could make two different files look identical, and this tool moves files to the Trash.

## Decision

Keep a persistent cache of each file's 4 KB prefix hash and full hash.

- **Key: the file's identity on disk** (device + inode), not its path. No path canonicalization is needed, paths with odd characters are no problem, and moving a folder keeps its entries.
- **Validity: a fingerprint taken before hashing**, made of size, modification time, change time (`ctime`, nanoseconds) and inode. A hash is reused only if the fingerprint is identical. `ctime` is the key part: the OS updates it on every content or metadata change, and tools that preserve modification time (`cp -p`, `rsync -t`) cannot set it.
- **Location:** `$XDG_CACHE_HOME/oxide_clean/hashes.tsv` if set, otherwise `~/Library/Caches/oxide_clean/` on macOS and `~/.cache/oxide_clean/` elsewhere.
- **Format:** tab-separated text with a version header. Any malformed line, unknown version or invalid hash rejects the *whole* file, and the cache starts empty.
- **Writes:** to a temporary file, then renamed into place, so a crash never leaves a half-written cache.
- **Pruning:** entries unused for 90 days are dropped on save.
- **Opt-out:** `--no-cache` neither reads nor writes it.

**Safety rule: deleting never trusts the cache.** `trash_duplicates` has no access to it (the function doesn't take one) and re-reads from disk the kept copy and every copy it is about to move. If the cache were wrong, the worst case is a false duplicate *shown* to the user, never moved.

## Options Considered

| Option | Verdict |
|---|---|
| Key by path, validate size + mtime | Rejected: misses edits made by tools that preserve mtime. |
| Key by path, validate size + mtime + ctime | Works, but needs canonical paths and escaping in the file. |
| **Key by inode, validate size + mtime + ctime** | **Chosen:** same safety, simpler, survives moves. |
| SQLite or a binary format (serde) | Rejected for now: adds dependencies; a text file is enough at this size (44 KB for 312 entries). |

## Consequences

**Measured** (3.2 GB folder, Apple A18 Pro, SSD):

| Run | Time |
|---|---|
| `--no-cache` | 2.56 s |
| First run (empty cache) | 2.47 s |
| Second run | < 0.01 s |
| After editing one 250 MB file (same size, edit in the middle) | 0.52 s; only that file was re-read, and its false group disappeared |

**Easier:** repeat scans are close to instant.

**Harder:**
- Deleting now re-reads every extra copy as well as the kept file. That is slower to move many large files, but it's the price of never trusting the cache with a deletion. To limit the cost, it shows a progress spinner, and each group's files are re-read in parallel and acted on right away. Verifying everything up front would widen the window in which a file could change between its check and its move.
- The cache is Unix-only. Elsewhere every hash is computed, which is correct, just slower.

**Revisit if:** the cache file grows past a few MB (one entry per same-size candidate file, about 140 bytes each), or Windows support is needed.

## Tests

- An unchanged file is reused across runs, so its hash is computed once.
- A file rewritten with the same size and a restored mtime is re-hashed (ctime changed).
- Corrupt, foreign-version, malformed and invalid-hash cache files are ignored.
- The disabled cache never stores or saves.
- **A poisoned cache** (it claims two different files are identical) fools detection, but `trash_duplicates` still refuses to move anything.
- End to end: the second `--dupes` run reports reused hashes with identical groups, and `--no-cache` leaves no cache file.
