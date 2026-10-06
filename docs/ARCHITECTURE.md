# Architecture

* How the code is organized, what each file does, and how a scan and a cleanup run.
* For anyone working on the code. To learn what neet does, read [FEATURES.md](FEATURES.md).

## Contents

1. [Overview](#overview)
2. [Files](#files)
3. [How neet Runs](#how-neet-runs)
4. [The Scan](#the-scan)
5. [A Cleanup](#a-cleanup)
6. [Dependencies](#dependencies)
7. [Building](#building)
8. [Branches And Releases](#branches-and-releases)

## Overview

* The code is split into two Rust packages, called crates:
  1. **`neet-core`**, the library. It scans, measures, reads rules, checks paths, plans cleanups, and moves items to the Trash. It draws nothing.
  2. **`neet`**, the program you run. It draws the screens and reads the keyboard.
* `neet` uses `neet-core`. `neet-core` never uses `neet`.
* Where macOS already has a tool, neet runs it instead of adding a dependency.

## Files

```text
neet/
├── Cargo.toml                 Workspace settings, shared version, lint rules
├── release-plz.toml           Version and crates.io release settings
├── dist-workspace.toml        Settings for building the ready made programs
├── crates/
│   ├── neet-core/             The library
│   │   ├── rules/             Built in cleanup rules, one file per category
│   │   └── src/
│   └── neet/                  The program
│       └── src/
│           ├── main.rs
│           └── ui/            One file per screen
├── docs/
└── .github/                   CI, releases, and Dependabot
```

### The Library

| File | Purpose |
| --- | --- |
| `lib.rs` | Lists the modules below. |
| `scan.rs` | Scans a folder into a tree on several threads. Does not follow links or enter other disks. Records unreadable folders, and reports progress. |
| `tree.rs` | The scanned tree: each file and folder, its size, item count, and when it last changed. |
| `size.rs` | The space a file really takes on disk, and counting a file with several names only once. |
| `disk.rs` | How big the disk is, how much is free, and the free space Finder shows, which adds purgeable space. |
| `large.rs` | Finds files in the tree above a size, and optionally unchanged for a while. |
| `clutter.rs` | Measures what neet does not clean itself: the Trash, installers, build folders, and the Docker image from the tree, simulator runtimes from `xcrun simctl`, and temporary files from a scan of `/private/var/folders`. |
| `rules.rs` | Reads and checks the built in rules and your own. |
| `safety.rs` | The allowed folders, the protected folders, and the two path checks: one for cleanup, one for app removal. Only these checks can approve an item. |
| `clean.rs` | Plans a cleanup from the rules, or from one picked item, then moves the selected items, checking each one again first. |
| `removal.rs` | Lists apps, finds each app's files by exact name, and plans their removal. |
| `apps.rs` | Whether an app is open, asked with `lsappinfo`. |
| `trash.rs` | Moves one item to the Trash by asking Finder with `osascript`, so Put Back works. |

### The Program

| File | Purpose |
| --- | --- |
| `main.rs` | Handles `--help` and `--version`, then opens the interface. |
| `ui/mod.rs` | The main loop: draw, wait for a key, pass it on. |
| `ui/app.rs` | The screen stack, the keys every screen shares, and the state every screen can read: the scan, the disk space, and what Clean can free. |
| `ui/home.rs` | Home: the art, the menu, and the Disk and Home folder boxes. |
| `ui/art.rs` | The Home art. |
| `ui/scan.rs` | Runs the home folder scan in the background. |
| `ui/skipped.rs` | Skipped: folders the scan could not read. |
| `ui/disk.rs` | Disk: the folder browser. |
| `ui/clean.rs` | Clean: rules, selection, and details. Also works out the Home total in the background. |
| `ui/review.rs` | Review, Confirm, Move, and Result, shared by every cleanup, plus the notice box. |
| `ui/large.rs` | Large Files. |
| `ui/quick.rs` | Quick Clean: everything that can be cleared, in one table. |
| `ui/apps.rs` | Remove App: the app list and the app's files. |
| `ui/help.rs` | The help box opened with `?`. |
| `ui/loading.rs` | The loading box a screen shows while slow work runs. |
| `ui/format.rs` | Sizes, counts, bars, and ages as text, and shortening long names and folders. |

## How neet Runs

```text
neet
    -> --help or --version: print and exit
    -> start the home folder scan in the background
    -> start working out what Clean can free, in the background
    -> show Home
    -> loop: draw the top screen, wait for a key, let the screen handle it
    -> restore the terminal and exit
```

* Screens form a stack, with Home at the bottom. Boxes, such as the question before a move, are screens too.
* Each key returns an action: do nothing, open a screen, go back one, go back to Home, or quit.
* Slow work, such as scanning, planning, and moving, runs on its own thread and reports back, so the screen keeps responding.
* Adding a feature means adding one menu row and one screen file.

## The Scan

* Starts at your home folder and reads folders on several threads.
* Stays on one disk, and lists any other disks it passes.
* Does not follow links.
* Measures the space each file takes on disk, and counts a file with several names once.
* Retries lookups that macOS interrupts, so they are not reported as errors.
* A home folder of 1.4 million items takes under 20 seconds.
* Sizes are estimates. On APFS, copied files can share space, so the disk gauge uses the disk's own totals instead.

## A Cleanup

```text
selected rules, a picked item, or an app
    -> find the items
    -> approve each item with the path check
    -> show every path, then ask
    -> check each item again, then ask Finder to move it
    -> show what moved and what was skipped
```

* Only an approved item can reach the code that moves files. See [SAFETY.md](SAFETY.md).
* Tests replace Finder with a stand in, so they never touch the real Trash.

## Dependencies

| Need | Uses |
| --- | --- |
| Walking folders on several threads | `jwalk` |
| Disk size and free space | `rustix` |
| The free space Finder shows | `osascript` asking macOS, part of macOS |
| Terminal screens | `ratatui` |
| Reading rules | `serde` and `toml` |
| Changing one TOML setting and keeping comments and layout | `toml_edit` |
| Reading simulator runtimes | `xcrun simctl`, part of Xcode, read with `serde_json` |
| Finding the temporary folder | `getconf`, part of macOS |
| Temporary folders in tests | `tempfile` |
| Whether an app is open | `lsappinfo`, part of macOS |
| Reading an app's details | `plutil`, part of macOS |
| Moving to the Trash | `osascript` asking Finder, part of macOS |
| Building release programs | `dist`, run in CI |

* Try each new crate in a small test before relying on it.

## Building

* Needs Rust 1.88 or later, and runs on macOS 13 or later, on Apple silicon and Intel.
* `unsafe` code is not allowed, and Clippy's strict checks are on.
* CI runs on every pull request, and on `master` after each merge:
  1. Formatting, Clippy, and tests.
  2. A build with Rust 1.88.
  3. The tests for Apple silicon and for Intel. The CI Macs are Apple silicon, so the Intel tests run under Rosetta, Apple's tool for running Intel programs.

## Branches And Releases

* Every change starts on its own branch, such as `feat/...`, `fix/...`, or `docs/...`, and reaches `master` through a pull request.
* `master` only accepts pull requests with every CI check passing.
* Commit messages follow the form `type(area): summary`, such as `feat(core): ...`. Release notes are written from them.
* Dependabot opens update pull requests every Monday.

| Step | What Happens |
| --- | --- |
| 1. Merge to `master` | release-plz opens or updates a release pull request with the next version and release notes. |
| 2. Review | Check the version and notes, and edit them if needed. |
| 3. Merge the release pull request | release-plz publishes both crates to crates.io, and tags the program, such as `neet-v0.1.0`. |
| 4. The tag | dist builds neet on Apple silicon and Intel Macs, and makes the GitHub release with the downloads, checksums, and an install script. |

| Secret | Used For |
| --- | --- |
| `CARGO_REGISTRY_TOKEN` | Publishing to crates.io. |
| `RELEASE_PLZ_TOKEN` | A GitHub token, so CI runs on the release pull request, and the tag starts the build. |

* crates.io only accepts a release from an account with a verified email address. If publishing fails, verify it, then run the failed job again.
