# Docs Guide

* Which doc answers which question, and where to make each kind of change.
* For anyone reading or updating the neet docs.
* Start with [FEATURES.md](FEATURES.md) to see what neet will do and what exists today.

## Contents

1. [The Docs](#the-docs)
2. [Feature Categories](#feature-categories)
3. [Where Changes Go](#where-changes-go)
4. [How To Write](#how-to-write)

## The Docs

| Doc | What It Covers |
| --- | --- |
| [Project README](../README.md) | What neet is, how far along it is, and how you will install and open it. |
| [FEATURES.md](FEATURES.md) | Every feature, grouped by purpose, with its phase, status, open questions, and what neet will never do. |
| [INTERFACE.md](INTERFACE.md) | The Home menu, what each screen shows, keyboard keys, and review steps. |
| [SAFETY.md](SAFETY.md) | Cleanup roots, protected paths, allow lists, backups, admin rights, and the safety tests. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | The two crates, how data moves, each file's purpose, and dependencies. |
| [ROADMAP.md](ROADMAP.md) | Milestones, what is built, what remains, and how each milestone is accepted. |

## Feature Categories

| Category | Purpose |
| --- | --- |
| [Disk analysis](FEATURES.md#disk-analysis-and-file-discovery) | Find what fills the disk, and browse or filter the scan. |
| [Cleanup and rules](FEATURES.md#cleanup-and-rules) | Review allowed targets, and move confirmed items to the Trash. |
| [Application management](FEATURES.md#application-management) | Review an app and its files before removing it. |
| [Startup and background services](FEATURES.md#startup-and-background-services) | See programs that start on their own, and turn them off in a way you can undo. |
| [SSH management](FEATURES.md#ssh-management) | See hosts and key details, fix permissions, and manage agent keys and known hosts. |
| [Dotfile management](FEATURES.md#dotfile-management) | List, edit, check, back up, and export settings files. |
| [AI coding tool files](FEATURES.md#ai-coding-tool-files) | View Claude Code and Codex settings, instruction files, and skills. |
| [Performance, power, and displays](FEATURES.md#performance-power-and-displays) | See what keeps the Mac awake, and change power and display settings. |
| [Interface and distribution](FEATURES.md#interface-preferences-and-distribution) | Move around the program, get help, install it, and later save preferences. |
| [Safety and recovery](FEATURES.md#safety-and-recovery) | Path limits, identity checks, backups, and undo. |

## Where Changes Go

| If You | Update |
| --- | --- |
| Add, remove, or change the scope of a feature | [FEATURES.md](FEATURES.md) |
| Change a screen, key, or review step | [INTERFACE.md](INTERFACE.md) |
| Allow a new kind of change, path, or command | [SAFETY.md](SAFETY.md), before writing the code |
| Add or move a file, or change the design | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Build something, or pass a milestone check | [ROADMAP.md](ROADMAP.md), with the date, and the status in [FEATURES.md](FEATURES.md) |

* A phase is a target. It never means a feature is done.
* **Built** means the library has the code. It does not mean the program can use it yet.

## How To Write

1. Start with an H1, then bullets saying what the doc covers and who it is for, then a numbered Contents list.
2. Use bullets, numbered steps, and tables. No paragraphs.
3. Capitalize every word in headings.
4. Never use em dashes, or hyphens in ordinary text. Hyphens are fine in code.
5. Use plain words and short sentences. Explain a term the first time it appears, or in a Key Terms table.
6. Never describe planned work as finished.
