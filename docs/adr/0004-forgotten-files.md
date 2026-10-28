# ADR-0004: Which timestamps decide that a file is "forgotten"

**Status:** Accepted
**Date:** 2026-10-28
**Deciders:** M4DEN5K (project owner)

## Context

`--old` lists files nobody has used for a while, biggest first, so the user can decide what to clean up. A file system offers four timestamps, and none of them means "last used":

- **modified** (mtime): when the content last changed. Tools that copy or unpack files (`unzip`, `cp -p`, `rsync -a`) keep the original one.
- **accessed** (atime): when the file was last read, *if* the system records it. Measured on the owner's Mac (APFS): `README.md` was read dozens of times on 2026-10-28 and its atime still said 2026-10-25. Reads don't reliably update it, and background readers (Spotlight, backups, our own hashing) can update it without the user.
- **created** (birth time): meant to be when the file appeared on this disk. Measured on the owner's Mac: when `unzip`, `cp -p` or `ditto` set a modification time older than the birth time, macOS moves the birth time back to match. A file unpacked today from a 2015 archive reports a 2015 birth time.
- **changed** (ctime): any change to the content or metadata, including arriving on the disk. In the same measurement it showed the moment of unpacking or copying, and `touch -t` can't set it back. macOS also updates it for extended attributes the user never touches, and opening a file from Finder writes one (`com.apple.lastuseddate#PS`).

The tool's errors must go in one direction. Missing an old file is harmless. Calling a file the user still needs "forgotten" may lead them to delete it, and unlike a duplicate, it may be the only copy.

## Decision

A file is **forgotten** when, with `N` = the threshold (default **180 days**, `--old [DAYS]`):

1. it was **not modified** in the last `N` days, **and**
2. it was **not read** in the last `N` days, as far as atime records it, **and**
3. it **did not arrive or change** in the last **30 days** (ctime).

Strictly older: exactly `N` days is not forgotten.

**Why each rule:**
- mtime is the main signal.
- atime is a second condition, not a signal on its own. It can only take files *off* the list, never add one, so its unreliability can't cause a false "forgotten". Because a stale atime proves nothing, the output says "untouched", never "unread".
- The 30-day arrival grace covers files that just landed with an old mtime (an unzipped archive of 2015 photos). After a month they're judged like any other file. It uses ctime, not the birth time, because macOS moves the birth time back on unpacking (see Context). ctime also changing for reasons the user didn't cause only keeps more files off the list, which is the safe side; opening a file in Finder counting as a change is a real sign of use.

**Files that can't be judged are counted and shown, never silently dropped:**
- No modified time: "no date".
- Any timestamp in the future (wrong clock, files from another machine): "date in the future".
- Off Unix, std has no ctime: the creation time stands in for rule 3.

**Hardlinks** count once (by `FileId`), like in `--dupes`, so the total space shown is space that can really come back.

**Testable time:** core takes `now` as a parameter instead of reading the clock, and works with `Duration`s only, with no calendar or time zones. Ages are shown relative ("8 months").

`--old` is **read-only**. Moving forgotten files to the Trash needs its own decision.

## Options Considered

| Option | Verdict |
|---|---|
| mtime only | Rejected: an archive unpacked yesterday looks ten years old |
| `max(mtime, birth)`, protecting arrivals for the whole threshold | Rejected by the owner in favor of a fixed one-month grace |
| Birth time for the one-month grace (the first implementation) | Replaced by ctime: manual testing showed `unzip`, `cp -p` and `ditto` move the birth time back on macOS, so the grace didn't protect the case it was made for |
| atime as the main signal ("not read since") | Rejected: measured as unreliable on the target machine |
| macOS "Date Last Opened" (Spotlight `kMDItemLastUsedDate`) | Best signal on macOS, but needs CoreServices and isn't portable. Revisit if `--old` gets a Trash option |

## Consequences

- Files copied without preserving times (`cp`, Finder copies) look new for `N` days. This errs on the safe side; the help says so, so it isn't mistaken for a bug.
- The interactive menu reuses its initial scan, so in a long session the ages get stale. That's acceptable for a read-only listing.
- The grace (30 days) and the default (180 days) are constants next to the logic in `core/old_files.rs`.
