# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/ado11231/neet/compare/neet-core-v0.1.0...neet-core-v0.2.0)

### Added

- *(core)* turn your own launch agents off and back on
- *(tui)* Startup lists programs that start on their own
- *(core)* list startup items without admin rights
- *(core)* start a dotfiles repository without chezmoi
- *(dotfiles)* Configure zsh settings in a block neet owns
- *(dotfiles)* Configure Ghostty settings
- *(tui)* Docker in small boxes, and & for and on the cleanup screens
- *(tui)* a Trash screen that empties the Trash after a red question
- *(dotfiles)* Configure kitty and mise settings
- *(core)* Configure reads and changes tmux and Starship settings
- *(tui)* simpler Dotfiles screen: plain statuses, Enter opens what you can do
- *(dotfiles)* add a listed file to chezmoi with a, after showing it whole
- *(core)* review chezmoi's changed source files for secrets, commit only the chosen ones, and push without force
- *(core)* read and change Git settings one at a time with git config --file
- *(core)* restore a dotfile from a backup, with the same refusals as editing
- *(tui)* edit dotfiles with a check and a diff, and keep or put back a file that differs from chezmoi
- *(core)* edit a dotfile or its chezmoi source with checks and backups, keep the home version, or put chezmoi's back
- *(core)* change a settings file with a backup and an atomic write, only if it did not change, and restore backups
- *(core)* list dotfiles and read how each stands with chezmoi, without running chezmoi
- *(core)* list simulator devices and unused Docker volumes, and reset Docker to the Trash
- *(tui)* keep row colors when selected, say permanently, and center Quick Clean's empty list
- *(core)* read and remove simulator runtimes and Docker data through their own tools
- *(core)* let neet move project build folders and Downloads installers to the Trash
- show each app's size in Remove App, color the list, and sort by size with s
- *(core)* let a cleanup plan be copied
- *(core)* measure clutter neet does not clean itself
- *(core)* read the free space Finder shows, purgeable space included

### Fixed

- *(dotfiles)* keep changes and exports tied to their review
- *(dotfiles)* give chezmoi the file's home folder path, and never run it for a link
- *(core)* give Finder a minute to answer each move to the Trash, then skip the item
- *(core)* reset Docker by moving its disk image into the Trash directly, once nothing has it open
- *(core)* stop Docker Desktop with docker desktop stop, and know newer Docker's not running error
- *(core)* skip Finder's .DS_Store when rule paths expand

### Other

- *(core)* opt in round trip with the real launchctl
- *(dotfiles)* appease clippy in the zsh block
- *(dotfiles)* check a real chezmoi round trip
- *(core)* read a command's output while it runs, so a lot of it never stalls it
- *(core)* share the time limited command runner
- move the 0.1.0 date to the end of each changelog entry

Released 2026-10-09

## [0.1.0](https://github.com/ado11231/neet/releases/tag/neet-core-v0.1.0)

### Added

- refuse to run as root
- *(core)* find apps and their files, and plan their removal
- *(core)* add the app removal path check
- *(core)* record change times and find large and old files
- *(tui)* plan a cleanup of the item selected in Disk
- *(core)* move planned items to the Trash through Finder
- *(tui)* add the Clean screen
- *(core)* make dry run cleanup plans
- *(core)* add the first bundled cleanup rules
- *(core)* load and check cleanup rules
- *(core)* add the cleanup path check
- *(tui)* list what the scan skipped
- *(tui)* show a disk gauge on Home
- *(tui)* add the Disk screen
- *(core)* scan a folder into a tree

### Fixed

- *(core)* give Finder a file reference it can move

### Other

- show the value in empty asserts for Clippy 1.99
- add Dependabot and release-plz, and run CI on pull requests
- *(core)* scan folders on several threads
- cache builds, cancel replaced runs, and check the minimum Rust version
- rename crates to neet

Released 2026-10-01
