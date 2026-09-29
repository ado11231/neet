# Safety

* The rules neet follows whenever it changes a file or a setting, or reads a private one.
* For anyone writing code or rules that change things on the Mac.
* They cover every screen: Disk, Clean, SSH, Dotfiles, Startup, AI Tools, and Settings. They also cover bundled rules, your own rules, and custom cleaners.
* These are requirements. None of these safeguards are built yet. For progress, read [ROADMAP.md](ROADMAP.md). For open questions, read [FEATURES.md](FEATURES.md#design-questions).

## Contents

1. [Key Terms](#key-terms)
2. [How A Cleanup Runs](#how-a-cleanup-runs)
3. [The Path Check](#the-path-check)
4. [Protected Paths](#protected-paths)
5. [What The Path Check Does](#what-the-path-check-does)
6. [Cleanup Rules](#cleanup-rules)
7. [Risk Tiers And Running Apps](#risk-tiers-and-running-apps)
8. [SSH And Dotfile Changes](#ssh-and-dotfile-changes)
9. [AI Coding Tool Files](#ai-coding-tool-files)
10. [Admin Rights](#admin-rights)
11. [Settings And Startup Changes](#settings-and-startup-changes)
12. [Tests](#tests)

## Key Terms

| Term | Meaning |
| --- | --- |
| **Cleanup root** | A folder that cleanup may remove items from. The list is fixed in the Rust code. |
| **Protected path** | A folder neet never touches, including everything inside it. |
| **Symbolic link** | A file that points to another path. |
| **Real path** | A path after following every symbolic link and `..`. |
| **Device and inode** | Two numbers that identify a file, even if it is renamed. |
| **Allow list** | A fixed list, in the Rust code, of the only files or settings a feature may change. |
| **`sudo`** | Runs one command with admin rights, after asking for your password. |

## How A Cleanup Runs

1. Make a plan without changing anything. This is a dry run.
2. Show every path the plan found.
3. Show how many items there are, how much space they use, and that they go to the Trash.
4. Ask you to confirm.
5. Check each path again.
6. Move the items to the Trash.

* P1 never deletes files for good and never empties the Trash.
* Items go to the Trash through Finder, so Finder's Put Back can restore each one to where it was. The first cleanup makes macOS ask whether neet may control Finder. The faster way that skips Finder is not used, because it loses Put Back.
* Cleanup only happens in the terminal interface. There is no command or option that skips the path list or the question.
* After a cleanup, neet shows how much space the cleaned items now take up in the Trash, and that it is only freed once you empty the Trash.

## The Path Check

* Safety lives in the Rust code. TOML rules cannot change it.
* Every path must pass one function before it can be moved:

  ```rust
  fn validate_deletable(path: &Path) -> Result<ValidatedPath, SafetyError>
  ```

* Only this function can make a `ValidatedPath`, so no other code can skip the check.
* A path must be inside a cleanup root, and never the root itself.
* P1 has exactly these cleanup roots. Each one is a folder whose contents apps can make again. A root never includes installed software or your own data.

| Cleanup Root | Holds |
| --- | --- |
| `~/Library/Caches` | App caches, including Homebrew, pip, pnpm, and Yarn caches. Each rule still names the folder it cleans. |
| `~/Library/Containers/*/Data/Library/Caches` | Caches of sandboxed apps. Only the `Caches` folder inside each app's container, never anything else in the container. |
| `~/Library/Logs` | App logs. |
| `~/Library/Saved Application State` | Window state apps save to reopen where you left off. |
| `~/Library/Developer/Xcode/DerivedData` | Xcode build output. |
| `~/Library/Developer/Xcode/iOS DeviceSupport` | Debug files Xcode copies from connected devices. |
| `~/Library/Developer/CoreSimulator/Caches` | Simulator caches. Never the simulators themselves. |
| `~/.npm/_cacache` | The npm download cache. |
| `~/.cargo/registry/cache` | Downloaded Rust crate archives. |
| `~/.cache/pip` | The pip cache, when pip is set to keep it here instead of `~/Library/Caches`. |

* In the Containers root, `*` stands for exactly one folder, an app's container. The path check handles it. It is not a rule pattern.
* Tool folders such as `~/.rustup`, `~/.pyenv`, `~/.m2`, and the rest of `~/.cargo` hold installed software or downloads you may need offline, so they are not roots.
* Mail download caches live in `~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads`, outside the Containers root. They are not a root until that exact folder is reviewed. See [Design Questions](FEATURES.md#design-questions).
* System logs have no root. Their locations, and a way to clean them without admin rights, are not chosen yet.
* Adding a root needs a change to this table and to the Rust code, in the same change.
* A rule cannot add a cleanup root.
* The P1.5 app removal review does not add apps as a cleanup root. What it may remove must be decided first.
* `~/.claude` and `~/.codex` are not cleanup roots, so cleanup cannot touch them.

## Protected Paths

* Cleanup always refuses these, and everything inside them:

| Path | Why |
| --- | --- |
| `/System`, `/usr` | macOS system files. |
| `/Library` itself | Shared by apps and services. |
| Your home folder itself | Far too broad. |
| `~/Documents`, `~/Desktop`, `~/Downloads` | Your own files. |
| `~/Library/Mobile Documents`, `~/Library/CloudStorage` | iCloud and other cloud files. |
| `~/Library/Keychains`, `~/.ssh` | Passwords and keys. |
| Any `.git` folder | Repository history. |
| `~/Library/Developer/Xcode/Archives` | Archived builds with the debug symbols of apps you shipped. They cannot be made again. |
| The top folder of any disk | Far too broad. |

* Folders inside `/Library` are not P1 targets. Adding one later needs a safety review and a change to the Rust code.
* SSH and Dotfiles have separate, narrow allow lists for changing files where they are. They never give cleanup access to protected paths.

## What The Path Check Does

1. Refuses empty paths, relative paths, paths it does not support, and the top folder of any disk.
2. Finds your home folder.
3. Follows each part of the path on disk. It does not just remove `..` from the text.
4. Refuses symbolic links that lead out of a cleanup root or into a protected path.
5. Confirms the path is inside its cleanup root.
6. Saves the real path, device, inode, and other details it needs.

* Right before moving an item, neet checks its real path, device, and inode again. If anything changed, it skips that item.
* If neet cannot be sure a file is the same one you reviewed, it skips it.
* **Known gap:** moving to the Trash through Finder takes a path, not an open file. Between the last check and the move, a very short window remains in which a path could be swapped for a link. neet checks right before each move to keep that window as small as possible. Closing it fully would need a way of moving files that macOS does not offer for the Trash.
* Any other case macOS cannot protect against must be written down here before M4 is done.

## Cleanup Rules

* Bundled rules live in `rules/`. Your own rules live in `~/.config/neet/rules/`.
* Your rule can replace a bundled rule by using the same `id`. It still goes through the same Rust checks.

```toml
[[rule]]
id = "xcode-derived-data"
name = "Xcode DerivedData"
category = "developer"
tier = "safe"
paths = ["~/Library/Developer/Xcode/DerivedData/*"]
description = "Build files. Xcode makes them again during the next build."
regenerates = true
requires_quit = ["com.apple.dt.Xcode"]
min_age_days = 0
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | Yes | Lowercase letters, numbers, and hyphens. |
| `name` | Yes | The name shown in the Clean screen. |
| `category` | Yes | `developer`, `package`, `application`, `browser`, `logs`, or `system`. |
| `tier` | Yes | `safe`, `caution`, or `expert`. |
| `paths` | Yes | Full paths. `~` may only be used at the start. |
| `description` | Yes | What the files are, and what happens when they are removed. |
| `regenerates` | No | Whether the app makes the files again. |
| `requires_quit` | No | The IDs of apps that must be closed first. |
| `min_age_days` | No | Skip files changed within this many days. |

* neet refuses unknown fields, patterns it does not support, and fixed paths that are not safe.

### Path Patterns

* The only pattern is `*`. It matches any name within one folder level, such as `DerivedData/*` or `Caches/com.example.*`.
* Not supported: `**`, `?`, `[...]`, and `{a,b}`.
* A `*` may only appear after the cleanup root part of the path. `~/Library/Caches/*/data` is allowed, but `~/Library/*/Caches` is not.
* Every path a pattern matches must pass the path check, like any other path.

### Your Own Rules

* Your rules may target anything inside the cleanup roots, and nothing outside them.
* They are never selected from the start. A `safe` tier in your rule is treated as `caution`.
* A rule with the same `id` as a bundled rule replaces it, under these same limits.

### Minimum Age

* `min_age_days` compares against the newest change anywhere inside an item. A folder counts as changed when any file inside it changed, not only when entries were added or removed.
* If neet cannot read the times of everything inside, it skips the item.
* Every path a pattern matches is checked again. Matching nothing is fine.
* Never call files junk or useless without saying why they are safe to remove.
* Before adding a rule:
  1. Run the rule and safety tests.
  2. Read every path its dry run finds.
  3. Write down the macOS and app versions you tested with.

## Risk Tiers And Running Apps

| Tier | How It Is Selected | Why |
| --- | --- | --- |
| `safe` | Already selected. | The files come back, and the only cost is time. |
| `caution` | You select it. | You may need to download, index, or sign in again. |
| `expert` | You select it and type a confirmation. | The files may not exist anywhere else. |

* A tier never skips a check or the question.
* neet checks the apps in `requires_quit` when it makes the plan, and again before moving files. If one of them opened in between, its items are skipped.
* Custom cleaners, such as Docker or simulator cleanup, may use the app's own tool. They still show a plan, and go through the same checks and question.
* A cleaner whose tool cannot be undone the way the Trash can is not allowed. Its targets, and how to recover from it, must be settled first.

## SSH And Dotfile Changes

* These change files where they are, so they do not use `ValidatedPath` or the Trash. They have their own allow list instead.

| Feature | May Change |
| --- | --- |
| Dotfile editing | Only the files listed in the Dotfiles screen. |
| SSH permissions | `~/.ssh` and the files directly inside it. |
| Known hosts | `~/.ssh/known_hosts`, only through `ssh-keygen -R`. |
| Agent keys | Nothing on disk. `ssh-add` only changes the running agent. |

* Every change must:
  1. Save the old content or permissions first, so it can be undone.
  2. Refuse symbolic links that lead out of your home folder.
  3. Write to a temporary file in the same folder, then swap it in, so a crash never leaves half a file.
* Cleanup can still never touch `~/.ssh`.
* neet never reads what is inside a private key. Key details come from `ssh-keygen`.
* Exports:
  1. Never include private keys, `~/.netrc`, `~/.aws/credentials`, `~/.npmrc`, or `~/.pypirc`.
  2. Show you anything that looks like a token or password before writing the archive.
* Detection can miss secrets. Exports must be called reviewed exports, never promised to be free of secrets.

## AI Coding Tool Files

* This view only reads files. It never edits, moves, or deletes anything.
* It may read only:
  1. The settings files `~/.claude/settings.json` and `~/.codex/config.toml`.
  2. The instruction files `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`.
  3. `SKILL.md` files, and the names of the other files beside them, in each tool's `skills` folder, its plugins, and project skill folders.
* It never opens sign in files, such as `~/.codex/auth.json`.
* It never opens chat history, session logs, or databases, such as `history.jsonl`, `sessions`, and `.sqlite` files.
* It refuses symbolic links that lead out of the folder it is reading.

## Admin Rights

* neet never runs as root. If you start it with `sudo`, it stops and explains why: it would find root's home folder instead of yours.
* A change that needs admin rights runs one command with `sudo`, after showing you that command.
* The terminal screen pauses while `sudo` asks for your password, then comes back.
* Cleanup never uses `sudo`.

## Settings And Startup Changes

| Feature | May Change | How |
| --- | --- | --- |
| Power mode | Low Power Mode and High Power Mode, for battery and charger | `sudo pmset` |
| Graphics switching | `gpuswitch` | `sudo pmset` |
| Wake settings | `womp`, `powernap`, `tcpkeepalive` | `sudo pmset` |
| Refresh rate | Which mode a connected display uses | CoreGraphics |
| Startup items | Whether a login item, launch agent, or launch daemon runs | `launchctl` or the login item list, with `sudo` for launch daemons |

* neet refuses any `pmset` setting not in this table.
* Before each change, neet saves the old value in `~/.local/state/neet/`. Undo puts it back.
* A new refresh rate switches back after 15 seconds unless you keep it.
* Items in `/System/Library` are never changed.
* Launch agent and launch daemon files are never edited or deleted. neet only changes whether they run.

## Tests

* Use temporary folders to test:
  1. `..` tricks, doubled slashes, and paths outside cleanup roots.
  2. A path that is a cleanup root, the home folder, or the top folder of a disk.
  3. Protected paths written in other ways.
  4. Symbolic links that lead out, and links swapped in after the review.
  5. A device or inode that changed after the review.
  6. Names that are not valid `UTF-8`, or that neet does not support.
  7. Your own rules that try to reach outside the cleanup roots, and a `safe` tier in your own rule being treated as `caution`.
  8. Rule patterns with `**`, `?`, `[...]`, `{a,b}`, or a `*` before the end of the cleanup root.
  9. Paths inside a container but outside its `Caches` folder, and paths inside `~/Library/Developer/Xcode/Archives`.
  10. A folder whose own time is old but that holds a recently changed file, skipped by `min_age_days`.
  11. Dotfile edits outside the allow list, or through a symbolic link that leads out.
  12. Permission fixes and dotfile edits undone from their backups.
  13. Exports that contain a private key or something that looks like a token.
  14. Starting neet as root.
  15. `pmset` settings outside the allow list.
  16. Settings and startup items undone from their saved values.
  17. A refresh rate that is not kept switching back.
  18. The AI tools view refusing sign in files, chat history, and links that lead out.
* Automated tests never use a real home folder, and never change real Mac settings.
* The M4 manual test moves a harmless temporary file to the Trash, then restores it with Finder's Put Back.
