# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/ado11231/neet/releases/tag/neet-core-v0.1.0) - 2026-10-01

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
