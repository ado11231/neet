# Roadmap

* Finished work and passed checks.
* Remaining features, bugs, and tests.

## Contents

* [Overview](#overview)
* [Done](#done)
* [Next](#next)
* [Later](#later)

## Overview

* The app is published as v0.1.0 on crates.io and GitHub Releases.
* Newer work on `master` will ship in the next release.
* [Issues](https://github.com/ado11231/neet/issues) track only remaining work and bugs.
* A feature is done only when its checks pass.

## Done

### Foundation

* Rust app and library, with MIT and Apache 2.0 licenses.
* CI checks formatting, Clippy, tests, Rust 1.88, and both Mac builds.
* Releases provide crates, Mac downloads, checksums, and an installer.

### Scanner

* Background home scan, real disk sizes, and skipped path reports.
* Tests cover links, unreadable folders, shared file names, and other disks.
* A 1.4 million item home scan took 17 to 19 seconds.

### Home And Disk

* Home menu, disk totals, folder browser, and Full Disk Access help.
* Finder's free space is explained separately from the scan.

### Cleanup Rules

* Built in and user rules with fixed path limits and risk levels.
* Rules preview paths and explain skipped files without changing anything.

### Cleanup

* Review, confirmation, checks before moving, Trash, and results.
* Build folders and installers can be selected in Quick Clean.
* Docker and simulator tools ask before permanent removal.
* Trash can be emptied after a separate warning.

### Large Files And Remove App

* Size and age filters, cached app sizes, and reviewed app removal.
* App data and settings start unselected. Apple apps, links, and open apps are refused.

### First Release

* The installer, `cargo install neet`, and Intel download were checked.
* On Apple silicon with macOS 26.5, scan size matched `du` within 0.0003% over 115.65 GB.
* Full Disk Access refusals, cleanup reviews, and cancellation were checked by hand.
* Trash and Put Back passed for a test file and an app owned by root.
* Intel builds and tests passed under Rosetta. A real Intel Mac check remains in [#122](https://github.com/ado11231/neet/issues/122).

### Dotfiles

* Edit, save, apply, add, configure, backup, restore, and export are built.
* Tests reject changed review files, links, hooks, and push destinations.
* Every action passed by hand on real dotfiles and in temporary folders with chezmoi 2.72.2.

### Startup And AI Tools

* Startup lists items and turns user launch agents off and back on.
* Changed plists are refused. Partial changes and failed checks stay visible.
* AI Tools browses files and chats, reviews transcript cleanup, and manages local skill links and edits.
* Tests cover file checks, backups, failed cleanup, draft recovery, and narrow screens.

### Clarity Pass

* Screens use short labels, shared styles, and visible cleanup warnings.
* Busy app advice names the app to quit before refreshing.
* Chat cleanup keeps totals and actions visible while scrolling.
* Docker labels estimates and shows containers keeping images in use.

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
  9. Docker labels reported reclaimable space as an estimate and drops the free space forecast. A scrollable list names running containers, images, and Compose projects. Zero space results use a fresh container check and keep volume results separate.

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
