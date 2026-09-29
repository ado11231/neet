# Roadmap

* What neet does today, what comes next, and how each step is accepted.
* For anyone tracking progress or picking the next piece of work.
* A milestone is only marked complete after its checks pass.
* Last updated September 28, 2026. The 24 library tests and 21 interface tests pass. The program opens the Home screen, and Disk browses the scan.
* Features and their status live in [FEATURES.md](FEATURES.md). Screens live in [INTERFACE.md](INTERFACE.md).

## Contents

1. [Status](#status)
2. [How The Work Is Organized](#how-the-work-is-organized)
3. [Complete](#complete)
4. [In Progress](#in-progress)
5. [Planned](#planned)
6. [P1 Release Checks](#p1-release-checks)

## Status

| Milestone | Goal | Areas | Phase | Status |
| --- | --- | --- | --- | --- |
| M0 | Workspace, licenses, formatting, linting, and CI | Foundation | P1 | Complete |
| M1 | A full scan of the home folder | Disk analysis | P1 | In Progress |
| M2 | Home screen, disk browser, and keyboard keys | Disk analysis, interface | P1 | In Progress |
| M3 | Rules, risk tiers, and dry run plans in Clean | Cleanup, interface | P1 | Planned |
| M4 | Review, path checks, open app checks, and moving to the Trash | Cleanup, safety | P1 | Planned |
| M5 | Packages, testing on real Macs, and the P1 release checks | Distribution | P1 | Planned |
| M6 | Large and old file filters, and the app removal review | Disk analysis, apps | P1.5 | Planned. App removal scope is open. |
| M7 | Check and build the P2 proposals | Startup, SSH, dotfiles, AI tools, settings, preferences, treemap | P2 | Proposed |

## How The Work Is Organized

* Work is split into milestones, M0 to M7.
* Each milestone leaves neet building and its tests passing.
* M0 to M5 make up P1. M6 is P1.5. M7 is P2.
* M7 holds draft proposals, not settled plans. Settle the [design questions](FEATURES.md#design-questions) before building a feature they affect.

## Complete

### M0: Foundation

* A Cargo workspace with `neet` and `neet-core`.
* MIT and Apache 2.0 licenses.
* CI checks formatting, Clippy, and tests, and runs `cargo check` for Apple silicon and Intel.
* These checks do not replace the release builds and real Mac testing in M5.

## In Progress

### M1: Scanner

* **Built:**
  1. `walk_directory` walks a folder without following symbolic links or leaving the disk.
  2. `allocated_size` works out the space a file uses on disk.
  3. `HardLinkTracker` spots files already counted, by device and inode.
  4. `Tree` stores folders and files, links parents and children, adds up sizes, and rebuilds paths.
  5. `scan` joins them into one scan. It counts hard links once, records unreadable paths and marks the scan incomplete, lists folders on other disks, and reports progress.
  6. 24 tests cover walking, symbolic links, missing and file roots, hard links, files with empty parts, nested folders, unreadable folders, progress, and the tree.
  7. A scan of a real home folder, 1.4 million entries, matched `du` and took about a minute.
* **Remaining:**
  1. Test the other disk listing, for example with a mounted disk image.
  2. Make the scan faster, for example with `ignore` and `rayon`.
* **Done when** tests with temporary folders cover links, files with empty parts, nested folders, permissions, and other disks.

### M2: Home And Disk

* **Built:**
  1. `neet --help` and `neet --version`. Any other option is refused.
  2. The interface loop, the screen stack, and `Esc`, `q`, and `?` on every screen.
  3. The Home screen: art, menu, and info panel. Unbuilt features are dimmed and skipped.
  4. The home folder scan runs on a background thread from the start. Home shows live progress, then the total, and flags incomplete scans and skipped disks.
  5. The Disk screen: a folder column with size bars and percents, a preview column, sorting by size, name, or items, and keys to open folders and go back up. It shows scan progress until the scan finishes.
  6. Tests for the menu, the screen stack, the background scan, size formatting, the Disk browser, and the Home and Disk layouts.
  7. The Home art: a sleeping cat under a starry skylight, behind the neet wordmark, both in the blue `#82aaff`.
* **Remaining:**
  1. The disk gauge in the Home panel, and a list of the paths the scan skipped.
* **Done when** the screen keeps responding during a scan, and marks incomplete scans.

## Planned

### M3: Rules

* The TOML rule format, bundled and user rules, risk tiers, age filters, selection totals, and dry run plans shown in Clean.
* Docker, simulator, and system log cleaners wait on the [design questions](FEATURES.md#design-questions). Listing a target group does not approve it.
* **Done when** every bundled rule loads and finds only allowed targets, without changing any file.

### M4: Cleanup

* The path check, checks for open apps, the path review, the question, and moving to the Trash.
* **Done when** the safety tests in [SAFETY.md](SAFETY.md#tests) pass, and Finder restores a test item.

### M5: P1 Release

* Programs for Apple silicon and Intel, Homebrew and Cargo packages, and steps for turning on Full Disk Access.
* **Done when** automated tests and the release checks below pass on both kinds of Mac.

### M6: P1.5

* Filters for large and old files, and the app removal review.
* **Needed first:** apps are outside the cleanup roots. Decide what app removal may remove, and its screens, before building it. This milestone does not widen any cleanup root.
* **Done when:**
  1. The filters use the existing scan.
  2. Every removal follows the updated safety rules.
  3. Anything that may be your data stays unselected, and shared folders are flagged.
  4. An open app cannot be removed.

### M7: P2

* The Startup screen, with background items.
* The SSH screen and the Dotfiles screen.
* A view only screen for Claude Code and Codex settings, instruction files, and skills.
* The Settings screen: power mode, graphics switching, refresh rate, Game Mode guidance, what keeps the Mac awake, and wake settings.
* Saved neet preferences and the optional treemap, after their scope is defined.
* **Done when:**
  1. Every change saves its old state, and tests restore it.
  2. A refresh rate change that is not kept switches back.
  3. The `pmset` setting names are checked on real Macs, for each supported macOS version.
  4. Finding, turning off, and turning back on each kind of startup item is checked, and an incomplete list is labeled.
  5. SSH and dotfile changes stay within their allow lists, and the backup, restore, check, and export tests pass.
  6. The AI tools view reads only its allowed files, never opens sign in files or chat history, and finds project skills from the scan.
  7. Preferences keep the agreed choices between runs, and the treemap shows the scan without changing files.

## P1 Release Checks

1. Compare a known scan with the sizes macOS reports.
2. Confirm that missing Full Disk Access gives clear warnings about skipped folders.
3. Read every path the bundled rules find.
4. Move a harmless item to the Trash and restore it with Finder.
5. Check the labels that explain size on disk and APFS limits.
6. Confirm neet shows every path and asks before changing anything.
