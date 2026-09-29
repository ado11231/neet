# Architecture

* How neet is built: its parts, how data moves between them, and what each file does.
* For people working on the code. To learn what neet does, read [FEATURES.md](FEATURES.md).
* Much of this is the planned design. Today the scanner, the Home, Disk, and Clean screens, the rules, dry run plans, and moving files to the Trash exist.
* For progress, read [ROADMAP.md](ROADMAP.md). For the planned screens, read [INTERFACE.md](INTERFACE.md).

## Contents

1. [Overview](#overview)
2. [How The Code Is Organized](#how-the-code-is-organized)
3. [How Data Moves](#how-data-moves)
4. [The Scanner](#the-scanner)
5. [Cleanup](#cleanup)
6. [SSH And Dotfiles](#ssh-and-dotfiles)
7. [AI Coding Tool Files](#ai-coding-tool-files)
8. [Settings And Startup](#settings-and-startup)
9. [Dependencies](#dependencies)
10. [Platform Support](#platform-support)
11. [Branches And Releases](#branches-and-releases)
12. [File Reference](#file-reference)

## Overview

* neet is split into two crates. A crate is one Rust package.
  1. **`neet-core`** is the library. It scans, measures sizes, reads rules, checks paths, and plans cleanups. It never draws anything on screen.
  2. **`neet`** is the program you run. It holds the screens and keyboard input. It only accepts `--version` and `--help`, read from `std::env::args` without an argument parsing crate.
* `neet` uses `neet-core`. `neet-core` does not use `neet`.

## How The Code Is Organized

```text
neet/
├── Cargo.toml             Workspace settings
├── crates/
│   ├── neet-core/         The library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scan.rs
│   │       ├── size.rs
│   │       ├── tree.rs
│   │       ├── rules.rs
│   │       ├── safety.rs
│   │       ├── clean.rs
│   │       ├── apps.rs
│   │       └── trash.rs
│   └── neet/              The program
│       └── src/
│           └── main.rs
├── docs/                  Guide, features, interface, safety, architecture, roadmap
├── release-plz.toml       Release settings
└── .github/
    ├── dependabot.yml
    └── workflows/
        ├── ci.yml
        └── release-plz.yml
```

* These are planned, but do not exist yet:

| Path | Will Hold |
| --- | --- |
| `crates/neet-core/tests/` | Tests that use the library from outside. |
| `crates/neet/src/ui/` | The terminal screens. See [The Interface Loop](#the-interface-loop). |
| `crates/neet-core/rules/` | The bundled cleanup rules, one `.toml` file per category, built into the program. |

## How Data Moves

### Starting neet

```text
neet
    -> --help or --version: print and exit
    -> refuse to run as root
    -> set up the terminal, with a panic hook that puts it back
    -> start the home folder scan on a background thread
    -> run the interface loop on the Home screen
    -> put the terminal back and exit
```

### The Interface Loop

```text
loop:
    draw the screen on top of the stack
    wait for a key, a scan message, or a timer tick
    the top screen handles it and returns an Action
        None         -> keep going
        Open(screen) -> push the screen
        Back         -> pop one screen
        Quit         -> leave the loop
```

* Screens form a stack, with Home at the bottom. Dialogs, such as the path review and the confirmation, are screens too.
* Each screen implements one trait, with a `draw` method and a `handle` method that returns an `Action`.
* The scan sends its progress over a channel. Home, Disk, and Clean all read the same scan, so it only runs once.
* Adding a feature means adding one menu row and one screen file.

| File | Will Hold |
| --- | --- |
| `ui/app.rs` | The `App`, the screen stack, the shared scan, and the `Action` type. |
| `ui/scan.rs` | Runs the scan on a background thread, and collects its progress and result for the screens. |
| `ui/home.rs` | The Home screen: art, menu, and status panel. |
| `ui/art.rs` | The ASCII art. |
| `ui/disk.rs`, `ui/clean.rs` | The P1 feature screens. |
| `ui/review.rs` | The path review, the question, and the cleanup with its result. |
| `ui/large.rs` | The Large Files screen. |

### A Scan

* The program runs `scan` on a background thread when it starts, and the Home screen shows its progress.

```text
scan request
    -> walk the folders
    -> read each file's details
    -> build the folder tree
    -> show it on screen
```

### A Cleanup

```text
selected rules
    -> find the paths each rule matches
    -> check each path
    -> show every path to the user
    -> ask to confirm
    -> check each path again
    -> move it to the Trash
```

* Only a checked path, a `ValidatedPath`, can reach the code that moves files. See [SAFETY.md](SAFETY.md).

## The Scanner

* Starts in your home folder.
* Reads folders and file details on several threads, with `jwalk`. Entries still arrive parents first, so the tree is built on one thread.
* Speed is limited by how fast macOS can read file details, not by the walker. A home folder of 1.4 million entries takes under 20 seconds, down from about 55 with one thread.
* macOS sometimes interrupts a lookup when many run at once. neet retries those, and reads an interrupted folder again on its own after the walk. Only a second failure counts as an error.
* Stays on the disk it started on, and lists any other disks it skipped.
* Lists symbolic links, but does not follow them. A symbolic link is a file that points to another path.
* Measures the space each file uses on disk as `st_blocks * 512`.
* Counts a file with several names, a hard link, only once. It spots them by `(st_dev, st_ino)`, two numbers that identify a file.
* Keeps every file and folder in one list, `Vec<Node>`. Each entry finds its parent by its place in the list.
* Sends each entry, its progress, and any errors to the screen while it runs.
* Marks the scan incomplete when a folder cannot be read.
* The sizes are estimates:
  1. Some files skip empty parts, so their length is larger than the space they use.
  2. On APFS, copied files can share space, and file details cannot show this.
  3. So the disk gauge uses the disk's own totals instead.

## Cleanup

* Most cleanup targets come from TOML rules built into the program.
* Your own rules load from `~/.config/neet/rules/`.
* Targets that need an app's own tool, such as Docker, use a `Cleaner` trait instead of a rule.
* All of them go through the same safety checks.

## SSH And Dotfiles

* Both change files where they are, so they do not use `ValidatedPath` or the Trash.
* A separate allow list in `neet-core` names every file they may change.
* Before each change, neet saves the old content or permissions.
* It writes the new content to a temporary file, then swaps it in, so a crash never leaves half a file.
* neet runs the usual tools instead of writing its own:

| Need | Tool |
| --- | --- |
| Key details and fingerprints | `ssh-keygen -l` |
| Agent keys | `ssh-add` |
| Removing known hosts | `ssh-keygen -R` |
| Checking a file for mistakes | `ssh -G`, `zsh -n`, `bash -n`, `git config --list --file` |
| Editing | Your editor, from `$VISUAL` or `$EDITOR` |

## AI Coding Tool Files

* Only reads files. It has no code that writes, moves, or deletes them.
* A fixed list in `neet-core` names the files it may read. Sign in files and chat history are never on it. See [SAFETY.md](SAFETY.md#ai-coding-tool-files).
* Reads the name and description from the front matter at the top of each `SKILL.md`.
* Finds project skill folders in the finished home folder scan, instead of walking the disk again.

## Settings And Startup

* Both change Mac settings instead of files. neet saves the old value before each change, and undo puts it back.
* The allow list in `neet-core` names every setting and startup action they may use.
* A change that needs admin rights runs one command with `sudo`. neet itself never runs as root.
* neet runs the usual tools instead of writing its own:

| Need | Tool |
| --- | --- |
| Power mode, graphics switching, wake settings | `pmset` |
| Apps keeping the Mac awake | `pmset -g assertions` |
| Why the Mac woke overnight | `pmset -g log` |
| Refresh rates | CoreGraphics display modes |
| Background items | `sfltool dumpbtm` |
| Turning launch agents and daemons off or on | `launchctl` |

## Dependencies

* Versions live in `Cargo.toml`. Try each new crate in a small test before using it.

| Need | Crate | Status |
| --- | --- | --- |
| Walking folders on several threads | `jwalk` | In use |
| Disk size and free space | `rustix`, for a safe `statvfs` | In use |
| Temporary folders in tests | `tempfile` | In use |
| Terminal screens | `ratatui` with Crossterm | In use |
| Reading TOML | `serde` and `toml` | In use |
| Reading `SKILL.md` front matter | A small YAML reader, or a hand written parser | Candidate |
| Building rules into the program | `include_str!`, no crate needed | In use |
| Checking for running apps | `lsappinfo`, which ships with macOS and never opens the app | In use |
| Moving to the Trash | `osascript` asking Finder, so Put Back works. The path is passed as an argument, never written into the script. No crate needed. | In use |
| Writing archives | `tar` and `flate2` | Candidate |
| Changing display modes | `core-graphics` | Candidate |
| Errors | `thiserror`, and `anyhow` in the `neet` crate if needed | Candidate |

## Platform Support

* macOS 13 or later, on Apple silicon and Intel.
* Building needs Rust 1.88 or later. It is set as `rust-version` in `Cargo.toml`, checked in CI, and Clippy warns about anything newer.
* Not every disk uses APFS. If a disk does something neet does not support, it must stop with a clear message.
* The workspace does not allow `unsafe` code. macOS calls must go through a crate that handles that.

## Branches And Releases

* Every change starts on its own branch, such as `feat/...`, `fix/...`, `docs/...`, or `ci/...`, and reaches `master` through a pull request.
* A ruleset on `master` blocks direct pushes, and requires a pull request with every CI check passing. Admins can bypass it in an emergency.
* Commit messages follow conventional commits, such as `feat(core): ...` or `fix(tui): ...`. release-plz writes the changelogs from them.
* Dependabot opens grouped pull requests once a week, one for Cargo updates and one for GitHub Actions updates.

| Step | What Happens |
| --- | --- |
| 1. Merge to `master` | release-plz opens or updates a release pull request with the next versions and each crate's `CHANGELOG.md`. |
| 2. Review | Check the versions and changelogs, and edit them in the pull request if needed. |
| 3. Merge the release pull request | release-plz publishes `neet-core`, then `neet`, to crates.io, tags them, and makes a GitHub release for `neet`. |
| 4. Binaries | Planned for M5: prebuilt Apple silicon and Intel programs attached to the same GitHub release, and a Homebrew formula. |

| Secret | Needed For |
| --- | --- |
| `CARGO_REGISTRY_TOKEN` | Publishing to crates.io. A crates.io API token with the publish scope. |
| `RELEASE_PLZ_TOKEN` | Letting CI run on the release pull request. A fine grained personal access token for this repository, with read and write access to contents and pull requests. Without it, the release pull request cannot pass the `master` ruleset. |

## File Reference

| File | Purpose |
| --- | --- |
| `Cargo.toml` | Lists the two crates, their shared version, license, and minimum Rust version (1.88), and the lint rules. The rules turn on Clippy's strict checks and forbid `unsafe` code. |
| `crates/neet-core/Cargo.toml` | The library's crates.io details and dependencies: `jwalk`, `rustix`, `serde`, and `toml`, and `tempfile` for tests. |
| `crates/neet-core/src/lib.rs` | The library's entry point. Makes the `apps`, `clean`, `disk`, `rules`, `safety`, `scan`, `size`, `trash`, and `tree` modules public. |
| `crates/neet-core/src/disk.rs` | `disk_space` reads the size and free space of the disk holding a path, with `statvfs`. Has tests. |
| `crates/neet-core/src/scan.rs` | `scan` walks a folder on several threads without following links or leaving the disk, builds the full `Tree`, counts hard links once, records unreadable paths, noting when macOS denied access, lists other disks, retries interrupted lookups, records when each entry last changed, and reports `Progress`. Has tests, including one that mounts a disk image. |
| `crates/neet-core/src/size.rs` | `allocated_size` measures the space a file uses on disk. `HardLinkTracker` remembers each file's device and inode, so a hard link is counted once. Has tests for hard links and files with empty parts. |
| `crates/neet-core/src/tree.rs` | `Tree`, `Node`, and `NodeId` store folders and files, add up folder sizes and item counts, and rebuild paths. Each node also keeps when it last changed. Filled by `scan`. Has tests. |
| `crates/neet-core/src/rules.rs` | Reads rule files into `Rule`s, checks every field and path against the rule format and `covers_pattern`, treats a `safe` tier in your own rule as `caution`, and lets your rule replace a bundled one with the same `id`. A rule with a problem is left out and reported in `RuleSet::errors`. Has tests. |
| `crates/neet-core/src/safety.rs` | `CleanupRoots` holds the cleanup roots and protected paths for a home folder. `validate_deletable` is the only way to make a `ValidatedPath`. `covers_pattern` checks that a rule pattern stays inside a root. Has tests. |
| `crates/neet-core/src/clean.rs` | Makes dry run plans. Expands each rule path, runs the path check on every item, measures its size and newest change without following links, skips items newer than the minimum age, leaves symbolic links in place, and gives an item found by two rules to the first. `plan_path` plans one item picked in Disk, refusing names with a `*`, and returns it selected. `run` moves the selected items, checking each one again right before: its app is closed, the path check passes, its real path, device, and inode are unchanged, and it is still older than the minimum age. Has tests. |
| `crates/neet-core/src/large.rs` | `find` lists files in the scan tree of at least a size, and optionally unchanged for a while, largest first. Files with no known change time never count as old. Has tests. |
| `crates/neet-core/src/apps.rs` | `is_running` asks `lsappinfo` whether an app is open. If it cannot tell, it answers yes. Has tests. |
| `crates/neet-core/src/trash.rs` | `move_to_trash` asks Finder, through `osascript`, to move one item to the Trash, so Put Back works. Explains how to allow it when macOS has not let neet control Finder. |
| `crates/neet/Cargo.toml` | The program's dependencies: `neet-core` and `ratatui`, and `tempfile` for tests. |
| `crates/neet/src/main.rs` | The program's entry point. Handles `--help` and `--version`, then opens the terminal interface. |
| `crates/neet/src/ui/mod.rs` | The interface loop. Draws the screen, waits for a key, and passes it on. |
| `crates/neet/src/ui/app.rs` | `App`, the `Screen` trait, `Action` (including `Home`, which closes every screen above Home and plans the Home estimate again), `Context` (including `cleanable`, what Clean can free), the screen stack, what `Esc` does on each screen, and the disk space that refreshes every 5 seconds. Handles `Esc`, `q`, and `?` for every screen. Has tests. |
| `crates/neet/src/ui/home.rs` | The Home screen: art, menu, and info panel. Has tests. |
| `crates/neet/src/ui/art.rs` | The Home art. `ART` holds the sleeping cat and the wordmark, and can be edited in place. `Night` draws grey stars across the art column, thinning out towards uneven edges, then the art on top. Has tests. |
| `crates/neet/src/ui/scan.rs` | `ScanTask` runs the home folder scan on its own thread, and `ScanStatus` holds its progress or result. Has tests. |
| `crates/neet/src/ui/format.rs` | Formats sizes the way Finder does, and counts with commas. Has tests. |
| `crates/neet/src/ui/help.rs` | The help box opened by `?`, drawn over the current screen. |
| `crates/neet/src/ui/disk.rs` | The Disk screen: a folder column with size bars, a preview column, sorting, and navigation over the scan tree. `d` plans a cleanup of the selected item and opens the review, or a notice that says why it cannot be cleaned. Has tests. |
| `crates/neet/src/ui/large.rs` | The Large Files screen. Filters the scan with `large::find`, `s` and `a` change the size and age, `Enter` opens Disk on the file, and `d` plans a cleanup as in Disk. Has tests. |
| `crates/neet/src/ui/review.rs` | `Review` lists every path of the selected rules. `Notice` shows a short message in a box. `Confirm` asks before anything moves and blocks `q`. `Cleanup` runs `clean::run` on its own thread with a progress bar, refuses `Esc` and `q` until done, then shows what moved and what was skipped, and goes back to Home. Has tests, with a stand in for Finder. |
| `crates/neet/src/ui/skipped.rs` | The Skipped screen: unreadable paths with reasons, other disks, and a Full Disk Access hint. Has tests. |
| `crates/neet/src/ui/clean.rs` | The Clean screen. Loads the rules and makes the dry run plan on its own thread, then lists each rule with its tier, items, and size. `Space` selects a rule, the title shows the selection totals, and a panel explains the selected rule and lists every path it found or skipped. An `expert` rule is selected by typing its ID, and a rule whose app is open cannot be selected. `Estimate` plans every rule in the background for the Home summary. Has tests. |
| `.github/workflows/ci.yml` | Checks formatting, runs Clippy and tests, checks the code builds with the minimum Rust version, and runs `cargo check` for Apple silicon and Intel targets. Runs on pull requests, and on `master` after each merge. Caches Cargo files, and cancels a run when a newer push replaces it. |
| `.github/workflows/release-plz.yml` | On each push to `master`, opens or updates the release pull request, and releases when a release pull request was merged. |
| `.github/dependabot.yml` | Weekly grouped update pull requests for Cargo and GitHub Actions. |
| `release-plz.toml` | Releases only from a merged release pull request, keeps a changelog per crate, checks semver, and gives only `neet` a GitHub release. |
