# Architecture

* How neet is built: its parts, how data moves between them, and what each file does.
* For people working on the code. To learn what neet does, read [FEATURES.md](FEATURES.md).
* Much of this is the planned design. Today only the walker, size helpers, and folder tree exist. The cleanup files are empty, and the program has an empty `main`.
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
11. [File Reference](#file-reference)

## Overview

* neet is split into two crates. A crate is one Rust package.
  1. **`neet-core`** is the library. It scans, measures sizes, reads rules, checks paths, and plans cleanups. It never draws anything on screen.
  2. **`neet`** is the program you run. It holds the screens and keyboard input. It only accepts `--version` and `--help`, read from `std::env::args` without an argument parsing crate.
* `neet` uses `neet-core`. `neet-core` does not use `neet`.
* The project was renamed from tidymac. The crate folders and `Cargo.toml` names in the code still say `tidymac` until that rename is made.

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
│   │       └── clean.rs
│   └── neet/              The program
│       └── src/
│           └── main.rs
├── docs/                  Guide, features, interface, safety, architecture, roadmap
└── .github/workflows/ci.yml
```

* These are planned, but do not exist yet:

| Path | Will Hold |
| --- | --- |
| `crates/neet-core/tests/` | Tests that use the library from outside. |
| `crates/neet/src/ui/` | The terminal screens. See [The Interface Loop](#the-interface-loop). |
| `rules/` | The bundled cleanup rules. |

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
| `ui/event.rs` | The event type and the loop that waits for keys, scan messages, and ticks. |
| `ui/home.rs` | The Home screen: art, menu, and status panel. |
| `ui/art.rs` | The ASCII art. |
| `ui/disk.rs`, `ui/clean.rs` | The P1 feature screens. |

### A Scan

* This is the planned flow. The walker, size helpers, hard link tracker, and tree exist, but are not joined yet.

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
| Walking folders | `walkdir` | In use |
| Temporary folders in tests | `tempfile` | In use |
| Terminal screens | `ratatui` with Crossterm | Candidate |
| Faster walking | `ignore` and `rayon` | Candidate |
| Reading TOML | `serde` and `toml` | Candidate |
| Reading `SKILL.md` front matter | A small YAML reader, or a hand written parser | Candidate |
| Building rules into the program | `include_dir` | Candidate |
| Checking for running apps | `sysinfo` or a macOS API | Candidate |
| Moving to the Trash | A crate or macOS API that works with Finder's Put Back | Candidate |
| Writing archives | `tar` and `flate2` | Candidate |
| Changing display modes | `core-graphics` | Candidate |
| Errors | `thiserror`, and `anyhow` in the `neet` crate if needed | Candidate |

## Platform Support

* macOS 13 or later, on Apple silicon and Intel.
* Not every disk uses APFS. If a disk does something neet does not support, it must stop with a clear message.
* The workspace does not allow `unsafe` code. macOS calls must go through a crate that handles that.

## File Reference

| File | Purpose |
| --- | --- |
| `Cargo.toml` | Lists the two crates, their shared version and license, and the lint rules. The rules turn on Clippy's strict checks and forbid `unsafe` code. |
| `crates/neet-core/Cargo.toml` | The library's dependencies: `walkdir`, and `tempfile` for tests. |
| `crates/neet-core/src/lib.rs` | The library's entry point. Makes the `scan`, `size`, and `tree` modules public. |
| `crates/neet-core/src/scan.rs` | `walk_directory` walks a folder without following links or leaving the disk. Has tests. |
| `crates/neet-core/src/size.rs` | `allocated_size` measures the space a file uses on disk. `HardLinkTracker` remembers each file's device and inode, so a hard link is counted once. Has tests for hard links and files with empty parts. |
| `crates/neet-core/src/tree.rs` | `Tree`, `Node`, and `NodeId` store folders and files, add up folder sizes, and rebuild paths. Has tests. Not yet filled by the walker. |
| `crates/neet-core/src/rules.rs` | Empty. Will read and check rules. |
| `crates/neet-core/src/safety.rs` | Empty. Will hold `validate_deletable` and `ValidatedPath`. |
| `crates/neet-core/src/clean.rs` | Empty. Will plan cleanups and move items to the Trash. |
| `crates/neet/Cargo.toml` | The program's dependencies: `neet-core`. |
| `crates/neet/src/main.rs` | The program's entry point. An empty `main` for now. Will open the terminal interface. |
| `.github/workflows/ci.yml` | Checks formatting, runs Clippy and tests, and runs `cargo check` for Apple silicon and Intel targets. |
