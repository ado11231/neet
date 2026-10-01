# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/ado11231/neet/releases/tag/neet-v0.1.0)

### Added

- *(tui)* show Remove App and an app's files as tables
- *(tui)* restructure Clean into a table with details beside it
- *(tui)* show Large Files as a colored table
- *(tui)* show a loading box while slow work runs
- refuse to run as root
- *(tui)* add the Remove App screens
- *(tui)* add the Large Files screen
- *(tui)* block rules whose app is open, and complete M4
- *(tui)* plan a cleanup of the item selected in Disk
- *(tui)* show on Home how much Clean can free
- *(tui)* select expert rules by typing their ID
- *(tui)* add the path review, question, and cleanup screens
- *(core)* move planned items to the Trash through Finder
- *(tui)* add the Clean screen
- *(tui)* list what the scan skipped
- *(tui)* show a disk gauge on Home
- *(tui)* add the sleeping cat art to Home
- *(tui)* add the Disk screen
- *(tui)* scan the home folder in the background
- *(core)* scan a folder into a tree
- *(tui)* add the Home screen and screen stack

### Fixed

- *(tui)* explain a Disk refusal in plain sentences
- *(tui)* group the review by folder and say 1 item, not 1 items
- *(tui)* list blocked folders once under their own heading in Skipped
- *(tui)* keep the Home disk gauge from wrapping in a narrow panel
- *(tui)* say why a scan is incomplete, and name the terminal

### Other

- add the install script and cargo install to the README
- build release programs with dist
- rewrite the docs and README in plain words
- add Dependabot and release-plz, and run CI on pull requests
- propose space breakdown, project build folders, and shell PATH
- cache builds, cancel replaced runs, and check the minimum Rust version
- rename crates to neet
- replace tabs with a Home menu
- rename to neet and split out the interface doc
- plan SSH, dotfiles, startup, and settings features
- scaffold Rust workspace

Released 2026-10-01
