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

* Fix failed Docker lookups and unknown prune sizes in [#121](https://github.com/ado11231/neet/issues/121).
* Finish chat actions, related file cleanup, restore, and reload checks in [#111](https://github.com/ado11231/neet/issues/111).
* Add skill source and removal support in [#112](https://github.com/ado11231/neet/issues/112).
* Explain disk space outside the home scan in [#20](https://github.com/ado11231/neet/issues/20).
* Explain leftover simulator downloads in [#61](https://github.com/ado11231/neet/issues/61).
* Test Startup on macOS 13, 14, and 15 in [#22](https://github.com/ado11231/neet/issues/22). macOS 26.5 is checked.
* Review the next release, then verify the published crate and both Mac downloads.

## Later

| Work | Issue |
| --- | --- |
| SSH hosts, key details, permissions, and known hosts | [#23](https://github.com/ado11231/neet/issues/23) |
| Shell PATH inspection and reviewed edits | [#25](https://github.com/ado11231/neet/issues/25) |
| Power and display settings with undo | [#27](https://github.com/ado11231/neet/issues/27) |
| Folder sizes shown as areas | [#29](https://github.com/ado11231/neet/issues/29) |
| Saved preferences | [#30](https://github.com/ado11231/neet/issues/30) |
| Decide whether Mail attachment cleanup is safe | [#32](https://github.com/ado11231/neet/issues/32) |
| Decide whether system log cleanup is safe | [#33](https://github.com/ado11231/neet/issues/33) |

* These features must follow [SAFETY.md](SAFETY.md), with tests for refused changes and undo.
* Homebrew remains a later option. Current installs use crates.io or GitHub Releases.
