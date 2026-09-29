# Roadmap

* What neet does today, what comes next, and how each step is accepted.
* For anyone tracking progress or picking the next piece of work.
* A milestone is only marked complete after its checks pass.
* Last updated September 28, 2026. The 46 library tests and 27 interface tests pass. The program opens the Home screen, and Disk browses the scan.
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
| M1 | A full scan of the home folder | Disk analysis | P1 | Complete |
| M2 | Home screen, disk browser, and keyboard keys | Disk analysis, interface | P1 | Complete |
| M3 | Rules, risk tiers, and dry run plans in Clean | Cleanup, interface | P1 | Complete |
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
* CI also checks the minimum Rust version, 1.88, caches Cargo files, and cancels runs replaced by a newer push. Added September 28, 2026.
* These checks do not replace the release builds and real Mac testing in M5.

### M1: Scanner

* Completed September 28, 2026.
* **Built:**
  1. `allocated_size` works out the space a file uses on disk.
  2. `HardLinkTracker` spots files already counted, by device and inode.
  3. `Tree` stores folders and files, links parents and children, adds up sizes and item counts, and rebuilds paths.
  4. `scan` walks a folder on several threads with `jwalk`, without following symbolic links or leaving the disk. It counts hard links once, records unreadable paths and marks the scan incomplete, lists folders on other disks, and reports progress.
  5. Lookups that macOS interrupts are retried, and interrupted folders are read again one at a time, so they are not reported as unreadable.
  6. 25 tests cover symbolic links, missing and file roots, hard links, files with empty parts, nested folders, unreadable folders, another disk from a mounted disk image, progress, and the tree.
  7. A scan of a real home folder, 1.4 million entries, takes 17 to 19 seconds with no errors, down from 55 seconds with one thread.
* **Done when** tests with temporary folders cover links, files with empty parts, nested folders, permissions, and other disks. All are covered.

### M2: Home And Disk

* Completed September 28, 2026.
* **Built:**
  1. `neet --help` and `neet --version`. Any other option is refused.
  2. The interface loop, the screen stack, and `Esc`, `q`, and `?` on every screen.
  3. The Home screen: art, menu, and info panel. Unbuilt features are dimmed and skipped.
  4. The home folder scan runs on a background thread from the start. Home shows live progress, then the total, and flags incomplete scans and skipped disks.
  5. The Disk screen: a folder column with size bars and percents, a preview column, sorting by size, name, or items, and keys to open folders and go back up. It shows scan progress until the scan finishes.
  6. Tests for the menu, the screen stack, the background scan, size formatting, the Disk browser, and the Home and Disk layouts.
  7. The disk gauge on Home, from the disk's own totals, refreshed every 5 seconds.
  8. The Skipped screen, opened with `s` on Home. It lists unreadable paths with their reasons, and other disks, and explains Full Disk Access when macOS blocked a folder.
  9. The Home art: a sleeping cat under a starry skylight, behind the neet wordmark, both in the blue `#82aaff`.
* **Done when** the screen keeps responding during a scan, and marks incomplete scans. Both are checked.
* Planning a cleanup from Disk with `d` moves to M4, since it needs the path checks.

### M3: Rules

* Completed September 29, 2026.
* **Built:**
  1. The path check, pulled forward from M4 so rules can be checked against it. `CleanupRoots` holds the cleanup roots and protected paths from SAFETY.md, and `validate_deletable` makes a `ValidatedPath`. 12 tests cover `..` parts, doubled slashes, links above and at the item, protected paths, container folders, roots themselves, names that are not `UTF-8`, and rule patterns.
  2. The TOML rule format. `rules::load` reads bundled rules and your own from `~/.config/neet/rules/`, checks every field and path, treats a `safe` tier in your own rule as `caution`, and lets your rule replace a bundled one. A rule with a problem is left out and reported. 9 tests.
  3. The first 14 bundled rules in `crates/neet-core/rules/`, with strict tiers. Only Xcode DerivedData and simulator caches are `safe`. 3 tests check that every bundled rule loads and that no download cache is `safe`.
  4. Dry run plans in `clean::plan`, with sizes, minimum age checks, and selection totals. Every item passes the path check, a file with several names is counted once, and an item found by two rules goes to the first. 8 tests, including one that plans the bundled rules and checks no file changed.
  5. The Clean screen. It makes the plan on a background thread, since a real home folder took about 7 seconds, mostly in the npm cache. It lists each rule with its tier, items, and size, selects `safe` rules from the start, and lets `Space` select or clear the others. The selection totals sit under the list, and a panel explains the selected rule and every path it found or skipped. Rules that could not be loaded are listed. 5 tests.
* **Done when** every bundled rule loads and finds only allowed targets, without changing any file. A test plans every bundled rule in a fake home folder and checks that nothing changed.
* Moved to M4: the path review opened with `Enter`, selecting `expert` rules with a typed confirmation, and blocking rules whose app is open.

## In Progress

* Nothing. M4 is next.

## Planned

### M4: Cleanup

* Checks for open apps, the path review, the question, checking each path again right before the move, and moving to the Trash. The path check itself was built in M3.
* Selecting `expert` rules with a typed confirmation.
* Showing on Home how much Clean can free.
* Planning a cleanup of the item selected in Disk, with `d`.
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
* The PATH screen: the PATH check, which program wins, and reordering or editing through the dotfile backup steps.
* The Space Breakdown screen, which explains the gap between disk used space and the scan.
* The Projects screen for build folders in code projects, after its design question is settled.
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
  8. PATH changes stay within the Shell group of the dotfile allow list, and are undone from their backups.
  9. The space breakdown's parts add up to the disk's used space, with anything left over shown as not explained, on each supported macOS version.
  10. The Projects screen offers only listed build folders that Git ignores, and refuses the rest.

## P1 Release Checks

1. Compare a known scan with the sizes macOS reports.
2. Confirm that missing Full Disk Access gives clear warnings about skipped folders.
3. Read every path the bundled rules find.
4. Move a harmless item to the Trash and restore it with Finder.
5. Check the labels that explain size on disk and APFS limits.
6. Confirm neet shows every path and asks before changing anything.
