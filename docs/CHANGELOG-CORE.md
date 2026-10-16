# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/ado11231/neet/compare/neet-core-v0.1.0...neet-core-v0.2.0)

### Added


* [**breaking**] remove SSH support

* list running containers and projects

* include Gemini JSONL chats in cleanup

* read Gemini project markers for chat tags

* validate reviewed chat files before trashing

* summarize chats and flag missing projects

* add local tools and skill manager

* turn your own launch agents off and back on

* Startup lists programs that start on their own

* list startup items without admin rights

* start a dotfiles repository without chezmoi

* Configure zsh settings in a block neet owns

* Configure Ghostty settings

* Docker in small boxes, and & for and on the cleanup screens

* a Trash screen that empties the Trash after a red question

* Configure kitty and mise settings

* Configure reads and changes tmux and Starship settings

* simpler Dotfiles screen: plain statuses, Enter opens what you can do

* add a listed file to chezmoi with a, after showing it whole

* review chezmoi's changed source files for secrets, commit only the chosen ones, and push without force

* read and change Git settings one at a time with git config --file

* restore a dotfile from a backup, with the same refusals as editing

* edit dotfiles with a check and a diff, and keep or put back a file that differs from chezmoi

* edit a dotfile or its chezmoi source with checks and backups, keep the home version, or put chezmoi's back

* change a settings file with a backup and an atomic write, only if it did not change, and restore backups

* list dotfiles and read how each stands with chezmoi, without running chezmoi

* list simulator devices and unused Docker volumes, and reset Docker to the Trash

* keep row colors when selected, say permanently, and center Quick Clean's empty list

* read and remove simulator runtimes and Docker data through their own tools

* let neet move project build folders and Downloads installers to the Trash

* show each app's size in Remove App, color the list, and sort by size with s

* let a cleanup plan be copied

* measure clutter neet does not clean itself

* read the free space Finder shows, purgeable space included

### Fixed


* save state through a fresh private file

* keep existing files during direct moves

* include output reads in command timeouts

* [**breaking**] show lookup errors and unknown prune sizes

* save turn results and label attempts

* report partial command failures

* refuse plists changed since listing

* reject failed chat activity checks

* preserve drafts and clarify tool browsing

* keep changes and exports tied to their review

* give chezmoi the file's home folder path, and never run it for a link

* give Finder a minute to answer each move to the Trash, then skip the item

* reset Docker by moving its disk image into the Trash directly, once nothing has it open

* stop Docker Desktop with docker desktop stop, and know newer Docker's not running error

* skip Finder's .DS_Store when rule paths expand

### Other


* shorten module comments

* move and shorten changelogs

* track turn failure status

* opt in round trip with the real launchctl

* appease clippy in the zsh block

* check a real chezmoi round trip

* read a command's output while it runs, so a lot of it never stalls it

* share the time limited command runner

* move the 0.1.0 date to the end of each changelog entry

Released 2026-10-14
# Library Changelog

* Release notes for the neet-core library.
* App changes are in [CHANGELOG.md](CHANGELOG.md).

## Unreleased

## [0.3.0](https://github.com/ado11231/neet/compare/neet-core-v0.2.0...neet-core-v0.3.0)

### Added


* read PATH folders, their problems, and where they are added

* read and change power, Mac, and neet settings

* continue and archive chats, and clean up what they leave

* show where each skill comes from and remove skill links

* explain leftover runtime downloads

* find simulator downloads left without a runtime

* read volumes and measure folders outside the home scan

### Fixed


* flag only folders anyone can write to

* protect Mail and Messages data

* list plugin skills after your own and skip marker files

Released 2026-10-16

## [0.1.0](https://crates.io/crates/neet-core/0.1.0)

### Added

* Folder scans with disk sizes and file change times.
* Built in and user cleanup rules with fixed path limits.
* Cleanup plans with file and open app checks before moving.
* Trash moves through Finder, with Put Back support.
* App discovery and checks for removing related files.
* Size and age filters for large files.

### Fixed

* Finder file references used for Trash moves.

Released 2026-10-01
