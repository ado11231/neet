# Changelog

* Release notes for neet, the app, and neet-core, the library behind it.
* Each release lists what changed in the app, then what changed in the library.
* Versions follow [Semantic Versioning](https://semver.org). Before 1.0, a minor version can change the library's API.

## Unreleased

## [0.3.1](https://github.com/ado11231/neet/compare/neet-v0.3.0...neet-v0.3.1)

### Other


* start with the screens, then the keys

Released 2026-10-19

## [0.3.0](https://github.com/ado11231/neet/compare/neet-v0.2.0...neet-v0.3.0)

Settings arrives, so every screen on Home is now built. Disk explains space outside your home folder and draws folders as a treemap, and AI Tools can continue chats and manage skills.

### Added

**Settings**

* New Settings screen, number 9 on Home, with Power, Mac settings, Preferences, and Shell PATH tabs.
* Power lists Low Power Mode, sleep timers, Power Nap, and wake for network, per battery and charger. Changes run `sudo pmset` once after review, and `u` undoes them.
* Power flags sleep turned off with `pmset disablesleep 1`, and lists the apps keeping the Mac awake.
* Mac settings offers 31 Finder, typing, Dock, screenshot, and saving settings through `defaults`, each with undo and reset to macOS's default.
* Preferences remembers what Disk, Large Files, and AI Tools open with.
* Shell PATH lists your PATH folders in search order, flags missing, repeated, relative, and world-writable ones, shows where each was added, and which copy of a program runs.

**Disk**

* Space, on `b`, splits used disk space into your home folder, apps, macOS data, macOS's own volumes, and what cannot be measured without admin rights.
* Treemap, on `t`, draws a folder as boxes sized by the space each item takes.

**Quick Clean**

* Simulator downloads macOS keeps after their runtime is removed now show, with how to reuse them. macOS protects them, so they are view only.

**AI Tools**

* Skills show where they come from: your copy, a link, bundled with the tool, or a plugin with its version and on or off state.
* `x` removes a linked skill after review, leaving its source folder alone, and `u` puts the link back.
* `c` continues a chat in Claude Code or Codex, and `z` archives or unarchives a Codex chat.
* Chat cleanup also moves Claude Code's related folders to Trash, and removes the chat's history entries with a backup.

### Changed

* Mail and Messages data is protected, so no rule or removal can reach it.
* Mail attachment cleanup and system log cleanup were checked and left out. The reasons are in [SAFETY.md](SAFETY.md#left-out).
* The README lists every screen, including Settings.

### Fixed

* Space shows the same size for other volumes in every place.

### Library

* New `space`, `settings`, `path`, and `program` modules for the new screens.
* `ai::Item` gains a `source` field, and `clutter::Kind` gains `SimulatorDownloads`. Both change the public API.
* Chat actions in `ai::actions`: session IDs, resume commands, Codex archive, related files, and history cleanup.

Released 2026-10-16

## [0.2.0](https://github.com/ado11231/neet/compare/neet-v0.1.0...neet-v0.2.0)

Quick Clean, Startup, Dotfiles, and AI Tools join Home, and every screen moves to the same plain, boxed layout.

### Added

**Cleanup**

* Quick Clean: one table of everything that can be cleared, with where each item is and what clearing it means.
* Clear project build folders and installers in Downloads from Quick Clean, after review.
* Remove simulator runtimes and devices, prune Docker, pick unused Docker volumes, and reset Docker to the Trash.
* A Trash screen that lists what is in the Trash and empties it after a separate warning.
* Docker shows running containers, the images they hold, and Compose projects.
* Remove App shows each app's size and sorts by it.

**Startup**

* Lists programs that start on their own, with whether each runs and who signed it.
* `t` turns your own launch agents off and back on, after review.

**Dotfiles**

* Lists your settings files and how each stands with chezmoi, without running chezmoi.
* Edit with a check and a diff, keep backups, restore one, and compare with `d`.
* Configure Git, tmux, Starship, kitty, Ghostty, mise, and zsh settings one at a time.
* Add a file to chezmoi, export with a check for secrets, and start a dotfiles repository when there is none.

**AI Tools**

* Browse Claude Code, Codex, and Gemini CLI files, chats, and skills.
* Search chats by title and project, select them, and move them to Trash after review.
* Link a local skill folder, and edit or restore your own skills.

**Home and layout**

* Home opens on Disk, with the rest by number.
* Every screen uses the same boxes, plain words, and colors for sizes.

### Changed

* SSH support was removed. This changed the library's API.
* Deep Clean was renamed from Clean, and Disk and Large Files became view only.

### Fixed

* Lost background results stop loading and say what happened, instead of waiting forever.
* Docker no longer promises prune savings it cannot know, and explains when a prune frees nothing.
* Finder gets a minute to answer each move to the Trash, then the item is skipped.
* Startup refuses plists changed since they were listed, and saves partial results.
* Edits and exports stay tied to the review they came from.
* Failed cache estimates and sizes show as unknown, not zero.

### Library

* New modules for Docker and simulator tools, clutter, Startup, Dotfiles, and AI Tools.
* `disk::finder_free` reads the free space Finder shows, purgeable space included.
* Commands run with a time limit, reading their output as they go.
* State files are written through a fresh private file.

Released 2026-10-14

## [0.1.0](https://github.com/ado11231/neet/releases/tag/neet-v0.1.0)

The first release.

### Added

* Home with disk space, scan progress, and the cat artwork.
* Background folder scans, a disk browser, and large file filters.
* Cache and log cleanup with risk levels and open app checks.
* App removal with a review of the app and its related files.
* A required review and confirmation before anything moves to the Trash.
* Skipped path reports and Full Disk Access help.
* Refuses to run as root.
* Mac downloads, an installer, and `cargo install neet`.

### Fixed

* Clear reasons for blocked paths and incomplete scans.
* Better review groups, item counts, and narrow screen layouts.

### Library

* Folder scans with disk sizes and change times.
* Built in and user cleanup rules with fixed path limits.
* Cleanup plans that check files and open apps before moving.
* Trash moves through Finder, with Put Back support.

Released 2026-10-01
