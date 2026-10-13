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
| 7 | First Release | Done |
| 8 | Clarity Pass | Next |
| 9 | Later Features | Later |

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
* Quick Clean lists project build folders and installers, then moves only the ones you pick after review.
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
  4. An app owned by root, RunCat, moved to the Trash and came back with Put Back.

### 7. First Release

* Published as v0.1.0, on crates.io and as a GitHub release.
* neet refuses to run as root, and explains why.
* dist builds ready made programs for Apple silicon and Intel on each release, with an install script.
* The README explains how to install neet and turn on Full Disk Access.
* release-plz publishes to crates.io when its release pull request is merged.
* Homebrew is left for later. It needs its own tap repository until neet is known enough for Homebrew's main list.
* **Checked** on Apple silicon with macOS 26.5:
  1. The scan matches `du` to within 0.0003% over 115.65 GB, and the disk gauge matches `diskutil`.
  2. With Full Disk Access off, Home says macOS blocked 147 paths and shows the home folder as at least 105.0 GB. Skipped names the terminal and lists the steps.
  3. All 63 items and 189 skips the built in rules found were read. One question moved to the Clarity Pass: user logs offers a file of wireless networks.
  4. Put Back passed in milestone 5.
  5. Clean, Disk, and Remove App all list every path in the review and ask before anything moves. `n` and `Esc` go back, and `d` refuses items outside the cleanup folders with a reason.
  6. The install script, `cargo install neet`, and the Intel download were tried from the published release.
* **Intel:** there is no Intel Mac to test on. CI runs every test as Intel code under Rosetta, and the Intel release build starts and opens Home under Rosetta. Rosetta is not a real Intel Mac, so a report from an Intel user is welcome.

## Next

### 8. Clarity Pass

* Make it easy to see how your disk space is used, and why neet's numbers can differ from Finder's.
* Its bugs are fixed before any later feature. The other items can be done alongside them.
* **Done:**
  1. Free space and Finder. Home says how much free space Finder shows, and that it adds space macOS clears on its own, called purgeable space.
  2. A wireless networks file in user logs. It is a copy macOS wrote once for diagnostics. Your Mac connects without it, so the rule keeps offering it, and FEATURES explains it.
  3. Moving to the Trash no longer waits forever when Finder does not answer.
  4. Every built screen got a clarity pass: Quick Clean, Deep Clean, Remove App, Large Files, and Disk use small titled boxes, one label width, plain words, and lists only as tall as their rows.
  5. A Trash screen lists what is in the Trash and empties it after a red question.
  6. Home starts on Disk, then the cleanups, with SSH last.
  7. Deep Clean names the apps to quit and says to press `r` for busy app files. Other skip reasons keep their own explanation.
  8. AI chat cleanup keeps totals and actions visible while scrolling. It separates ready and skipped files, removes empty move prompts, and lists files with unconfirmed results after an interrupted cleanup.

| Item | Done When |
| --- | --- |
| Space breakdown | The parts add up to the disk's used space, with anything left shown as unexplained. Each part says what it is, in plain words. |

## Later

### 9. Later Features

* Remaining work stays here even when a feature has shipped. Proposed features must answer any [open questions](FEATURES.md#open-questions) before implementation.

| Feature | Done When |
| --- | --- |
| Startup items | List and turn actions are built and checked on macOS 26.5. Changed plists are refused from the list through confirmation. Turn results and saved history distinguish partial changes and unknown results. Checks on macOS 13, 14, and 15 remain in [#22](https://github.com/ado11231/neet/issues/22). |
| Dotfiles | **Done.** Changes stay within the list in SAFETY.md. Backup, restore, check, configure, and export tests pass. With chezmoi, edits reach the source file, and nothing is pushed without `y`. |
| SSH | Changes stay within `~/.ssh`, and known hosts and permissions can be undone. |
| Shell PATH | Changes stay within the shell files, and can be undone from their backups. |
| AI Tools | **First version built.** Browse known user and current project files for Claude Code, Codex, and Gemini CLI. Show chat titles and missing-project tags. Review and move selected transcript files to Trash. Link local skills and review edits and restores with backups. Resume, archive, index cleanup, and related files are tracked in [#111](https://github.com/ado11231/neet/issues/111); skill removal and plugin discovery in [#112](https://github.com/ado11231/neet/issues/112). |
| Power and display | Setting names are checked on real Macs. A refresh rate you do not keep switches back. Every change can be undone. |
| Treemap and preferences | The treemap shows the scan without changing files, and preferences keep your choices. |

* Dotfiles checks: action and export reviews reject changed files, backups, and links. Pushes name the reviewed commit and tracked branch. Unicode renames and export scrolling have regression tests. Ghostty, the zsh block, and starting a repository without chezmoi are built. On 2026-10-08 every action was run by hand against real personal dotfiles and worked, so Dotfiles is done.

* The opt in Dotfiles test passed with chezmoi 2.72.2 on macOS: edit and apply, save, put back, add, Configure, backup and restore, a link, and export to a local remote. It also refuses hooks added after an edit starts.

* AI Tools checks use temporary homes for discovery, chat parsing and summaries, missing-project tags, removal validation, local skill links, edit and restore, stale reviews, and escaped paths. UI tests cover section changes, shared skill filters, search, chat selection and removal review, partial cleanup results, selection after refresh, failed loads, draft recovery, edit and restore, review cancellation, and narrow terminals. Real assistant reloads still need a manual check.
