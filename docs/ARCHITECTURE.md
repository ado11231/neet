# Architecture

* Code layout and file roles.
* Build checks and release steps.

## Contents

* [Overview](#overview)
* [Files](#files)
* [How neet Runs](#how-neet-runs)
* [The Scan](#the-scan)
* [A Cleanup](#a-cleanup)
* [Dependencies](#dependencies)
* [Building](#building)
* [Branches And Releases](#branches-and-releases)

## Overview

* `neet-core` scans files, checks paths, and carries out reviewed changes.
* `neet` uses that library to draw screens and handle keys.
* Slow work runs in the background so the screen keeps responding.
* See [SAFETY.md](SAFETY.md) before adding anything that changes files.

## Files

| Path | Purpose |
| --- | --- |
| `Cargo.toml` | Packages, shared version, and code checks. |
| `crates/neet-core/rules/` | Built in cleanup rules. |
| `crates/neet-core/src/` | File scans and actions. |
| `crates/neet/src/` | Terminal app. |
| `docs/` | Project docs, including app and library changelogs. |
| `.github/` | Build checks, releases, and dependency updates. |
| `release-plz.toml` | Crate versions and release notes. |
| `dist-workspace.toml` | Mac downloads and installer. |
| `crates/neet/dist.toml` | Changelog path for Mac release notes. |

### `crates/neet-core/src/`

| File | Purpose |
| --- | --- |
| `lib.rs` | Lists the library modules. |
| `scan.rs`, `tree.rs`, `size.rs` | Read folders and record file sizes. |
| `disk.rs` | Disk totals and Finder's free space. |
| `large.rs` | Find large or old files in a scan. |
| `clutter.rs` | Find build folders, installers, and other space to review. |
| `rules.rs` | Load and check cleanup rules. |
| `safety.rs` | Check allowed and protected paths. |
| `clean.rs` | Plan a cleanup and recheck items before moving. |
| `removal.rs`, `apps.rs` | Find apps and their files, and check open apps. |
| `trash.rs` | Move items to Trash and empty it after confirmation. |
| `tools.rs` | Simulator and Docker commands. |
| `rewrite.rs` | Check files, save backups, and write changes. |
| `dotfiles.rs`, `dotfiles/` | Settings, chezmoi, edits, backups, and export. |
| `startup.rs`, `startup/` | List startup items and change user launch agents. |
| `ai.rs`, `ai/` | Assistant files, chats, and skills. |
| `run.rs` | Run commands with a time limit. |

### `crates/neet/src/`

| File | Purpose |
| --- | --- |
| `main.rs` | Command line options and startup. |
| `ui/mod.rs`, `ui/app.rs` | Main loop, shared state, and screen changes. |
| `ui/home.rs`, `ui/art.rs` | Home menu and artwork. |
| `ui/scan.rs`, `ui/skipped.rs` | Background scan and unreadable paths. |
| `ui/disk.rs`, `ui/large.rs` | Folder browser and large files. |
| `ui/clean.rs`, `ui/quick.rs` | Deep Clean and Quick Clean. |
| `ui/apps.rs`, `ui/pick.rs` | App removal, build folders, and installers. |
| `ui/review.rs` | Cleanup review, confirmation, progress, and results. |
| `ui/tools.rs`, `ui/tools/` | Docker, simulators, and Trash. |
| `ui/dotfiles.rs`, `ui/dotfiles/` | Settings files and their actions. |
| `ui/startup.rs` | Startup list and turn actions. |
| `ui/ai.rs`, `ui/ai/` | Assistant files, chat cleanup, and skill reviews. |
| `ui/editor.rs` | Open an editor on a copy, then return to neet. |
| `ui/visual.rs`, `ui/format.rs` | Shared layout, styles, and text formatting. |
| `ui/help.rs`, `ui/loading.rs` | Help and loading boxes. |

## How neet Runs

* `--help` and `--version` print a result and exit.
* Opening neet starts the scan and cleanup preview, then shows Home.
* The main loop draws the current screen and handles keys.
* Screens open on top of each other. `Esc` goes back.
* Before exit, neet restores the terminal.

## The Scan

* Reads folders on several threads without following links or entering other disks.
* Counts the space files use on disk. Files with several names count once.
* Records unreadable folders and marks the total incomplete.
* Sizes are estimates because copied files on APFS can share space.
* The disk gauge uses macOS totals, not the scan total.

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
| Reading local JSON files and tool output | `serde_json` |
| Reading simulator runtimes | `xcrun simctl`, part of Xcode, read with `serde_json` |
| Finding the temporary folder | `getconf`, part of macOS |
| Private skill drafts and temporary folders in tests | `tempfile` |
| File diffs | `similar` |
| Shortening names without splitting characters | `unicode-segmentation` |
| Whether an app is open | `lsappinfo`, part of macOS |
| Reading an app's details | `plutil`, part of macOS |
| Moving to the Trash | `osascript` asking Finder, part of macOS |
| Building release programs | `dist`, run in CI |

* Try each new crate in a small test before relying on it.

## Building

* From the repository, run `cargo run --release` to build and open the current branch.
* `cargo install --path crates/neet --force` installs that checkout.
* Before a PR, run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features`.
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
