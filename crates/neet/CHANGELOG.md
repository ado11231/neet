# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/ado11231/neet/compare/neet-v0.1.0...neet-v0.2.0)

### Added

- *(tui)* pick unused Docker volumes, reset Docker with x, and remove simulators with their runtimes
- *(core)* list simulator devices and unused Docker volumes, and reset Docker to the Trash
- *(tui)* give Large Files a filter box, size bars, and a side with the selected file and where files are
- *(tui)* keep row colors when selected, say permanently, and center Quick Clean's empty list
- *(tui)* make Disk and Large Files view only, without d
- *(tui)* remove simulator runtimes and prune Docker from Quick Clean
- *(tui)* split Deep Clean's details into boxes that fit, with a chart of every rule
- *(tui)* clear build folders and installers from Quick Clean, and tidy its layout
- show each app's size in Remove App, color the list, and sort by size with s
- *(tui)* order Home by quickest first, rename Clean to Deep Clean, and drop most gray text
- *(tui)* put Quick Clean first on Home, marked start here
- *(tui)* plan Clean once and reuse it, with r to look again
- *(tui)* show the move and its result in a box in the middle
- *(tui)* add Quick Clean, one table of everything that can be cleared
- *(tui)* add a summary beside the review, and leave out rules that found nothing
- *(tui)* explain the selected item in the Disk preview, with colors
- *(tui)* say on Home why Finder shows more free space

### Fixed

- *(tui)* match the blue accents to the Home art
- *(tui)* line up columns, brighten labels, and shorten screen text
- *(core)* reset Docker by moving its disk image into the Trash directly, once nothing has it open
- *(tui)* size message boxes by word wrapping, so their last line shows
- *(tui)* say remove permanently for simulators, offer prune only when Docker can, and say 1 item in Disk
- *(tui)* line up Deep Clean's item counts
- *(tui)* fit the Finder line on one row, and space out Home's details

### Other

- *(tui)* hold the plan back so Deep Clean's loading test cannot race
- *(tui)* share the colored size bar through format
- describe the new Home order, Deep Clean, and white selection
- describe Quick Clean
- *(tui)* share the size colors between screens
- move the 0.1.0 date to the end of each changelog entry
- link the wiki and crates.io from the README

Released 2026-10-02

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
