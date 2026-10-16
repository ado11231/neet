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
| `space.rs` | Volumes and folders outside the home scan. |
| `settings.rs`, `settings/` | Power, Mac settings, and neet preferences. |
| `program.rs` | Programs neet hands the terminal to. |
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
| `ui/space.rs` | Where used space outside the home folder goes. |
| `ui/settings.rs`, `ui/settings/` | Power, Mac settings, and preferences. |
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

* Find items and check their paths.
* Show every path and ask for confirmation.
* Recheck each item before asking Finder to move it to Trash.
* Show moved and skipped items, with reasons.
* Tests replace Finder with a fake mover and leave the real Trash alone.
* Docker, simulator, and Trash actions have separate rules in [SAFETY.md](SAFETY.md).

## Dependencies

| Need | Tool |
| --- | --- |
| Folder scans | `jwalk` |
| File and disk details | `rustix` |
| Terminal screens | `ratatui` |
| JSON and TOML | `serde`, `serde_json`, `toml`, `toml_edit` |
| Private drafts and test folders | `tempfile` |
| File diffs | `similar` |
| Shorten text without splitting characters | `unicode-segmentation` |
| Finder actions and free space | `osascript` |
| App details and open apps | `plutil`, `lsappinfo` |
| Simulators and temporary folders | `xcrun simctl`, `getconf` |
| Release downloads | `dist` |

## Building

* Requires Rust 1.88 or later and macOS 13 or later.
* Run: `cargo run --release`.
* Install: `cargo install --path crates/neet --locked --force`.
* Check before a PR:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo +1.88 check --workspace --all-targets --all-features --locked
```

## Branches And Releases

* Use a branch and PR into `master`. All required checks must pass.
* Use `type(scope): summary` for commits and PR titles.
* Keep commits at 200 changed lines or fewer, counting additions and deletions. Merge by rebase.
* Commit as `ado11231 <adoalagic0@gmail.com>`. No co-author.
* Get approval for each doc before committing. Stop and recap after each PR.
* release-plz prepares the release PR. Merging it publishes the crates and starts the Mac download builds.
