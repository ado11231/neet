# Roadmap

* What is done, what comes next, and when each step counts as finished.
* For anyone tracking progress or picking the next piece of work.
* A milestone is only marked done after its checks pass.

## Contents

1. [Overview](#overview)
2. [Done](#done)
3. [Next](#next)
4. [Later](#later)

## Overview

| # | Milestone | Status |
| --- | --- | --- |
| 1 | Foundation | Done |
| 2 | Scanner | Done |
| 3 | Home And Disk | Done |
| 4 | Cleanup Rules | Done |
| 5 | Cleanup | Done |
| 6 | Large Files And Remove App | Done |
| 7 | First Release | Next |
| 8 | Later Features | Later |

## Done

### 1. Foundation

* A Rust workspace with the `neet` program and the `neet-core` library.
* MIT and Apache 2.0 licenses.
* CI for formatting, Clippy, tests, the minimum Rust version, and builds for Apple silicon and Intel.
* Branches, pull requests, a protected `master`, Dependabot, and release-plz.

### 2. Scanner

* Scans the home folder on several threads, without following links or entering other disks.
* Measures the real space each file takes, and counts a file with several names once.
* Records folders it could not read, and marks the scan incomplete.
* **Checked:** tests cover links, empty parts of files, nested and unreadable folders, and another disk. A real home folder of 1.4 million items scans in 17 to 19 seconds.

### 3. Home And Disk

* The Home menu, with the art, a disk gauge, and scan progress.
* The Disk browser, with sizes, share bars, a preview, and sorting.
* The Skipped screen, with Full Disk Access help.
* `?` help, `Esc`, and `q` on every screen. `--help` and `--version`.
* **Checked:** the screen keeps responding during a scan, and marks an incomplete scan.

### 4. Cleanup Rules

* The path check, the allowed folders, and the protected folders.
* The rule format, 14 built in rules, and your own rules.
* A preview of what each rule finds, with sizes and recent file checks.
* The Clean screen.
* **Checked:** every built in rule loads and finds only allowed items, without changing a file.

### 5. Cleanup

* Review, confirm, and move to the Trash through Finder.
* A check of every item right before it moves: same file, app closed, not changed recently.
* Typed confirmation for `expert` rules, and no selecting a rule while its app is open.
* `d` to clean one item from Disk.
* What Clean can free, on Home.
* **Checked:** the cleanup tests pass, and on macOS 26.5 a test item moved to the Trash and came back with Put Back. That run found two bugs in the Finder step, both fixed.

### 6. Large Files And Remove App

* Done before the first release.
* The scan records when each file last changed.
* Large Files, with size and age filters.
* The App Removal rules in [SAFETY.md](SAFETY.md#app-removal), and their own path check.
* Remove App, using the same review and move as Clean.
* **Checked:**
  1. The filters use the existing scan, and take about 4 ms on 1.4 million items.
  2. Anything that may be your data, settings, or shared starts unselected.
  3. Apple's apps, links, and open apps are refused.
* **Not yet tried:** removing an app owned by another user, which makes Finder ask for a password.

## Next

### 7. First Release

* **Ready:** release-plz keeps an open release pull request, and publishes to crates.io when it is merged. It stays open until this milestone is done.
* **To do:**
  1. Try removing an app owned by another user.
  2. The release checks below.
* **Done:**
  1. neet refuses to run as root, and explains why.
  2. dist builds ready made programs for Apple silicon and Intel on each release, with an install script.
  3. Steps for turning on Full Disk Access, in the README.
* Homebrew is left for later. It needs its own tap repository until neet is known enough for Homebrew's main list.
* **Done when** the tests and these checks pass on both kinds of Mac:
  1. A known scan matches the sizes macOS reports.
  2. Missing Full Disk Access gives clear warnings.
  3. Every path the built in rules find has been read.
  4. A test item moves to the Trash and comes back with Put Back.
  5. neet shows every path and asks before changing anything.

## Later

### 8. Later Features

* Each of these needs its [open question](FEATURES.md#open-questions) answered first.

| Feature | Done When |
| --- | --- |
| Startup items | Finding, turning off, and turning back on each kind is checked on every supported macOS version. An incomplete list is labeled. |
| SSH and dotfiles | Changes stay within their lists, and backup, restore, check, and export tests pass. |
| Shell PATH | Changes stay within the shell files, and can be undone from their backups. |
| AI tool files | Only the allowed files are read. Sign in files and chat history are never opened. |
| Power and display | Setting names are checked on real Macs. A refresh rate you do not keep switches back. Every change can be undone. |
| Space breakdown | The parts add up to the disk's used space, with anything left shown as unexplained. |
| Project build folders | Only listed build folders that Git ignores are offered. |
| Treemap and preferences | The treemap shows the scan without changing files, and preferences keep your choices. |
