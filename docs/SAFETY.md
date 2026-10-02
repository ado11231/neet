# Safety

* The rules neet follows before it changes, moves, or reads anything private.
* For anyone changing neet's code or writing cleanup rules.
* These rules live in the code, not in settings. A rule file can never loosen them.

## Contents

1. [The Promises](#the-promises)
2. [How A Cleanup Runs](#how-a-cleanup-runs)
3. [Allowed Folders](#allowed-folders)
4. [Protected Folders](#protected-folders)
5. [The Path Check](#the-path-check)
6. [The Check Before Moving](#the-check-before-moving)
7. [Cleanup Rules](#cleanup-rules)
8. [App Removal](#app-removal)
9. [Build Folders And Installers](#build-folders-and-installers)
10. [Tools neet Runs](#tools-neet-runs)
11. [Planned Features](#planned-features)
12. [Tests](#tests)

## The Promises

1. Nothing is deleted permanently, and neet never empties the Trash. The one exception is [simulators and Docker data](#tools-neet-runs), which only their own tool can remove, after a separate question that says so.
2. You see every path, and confirm, before anything moves.
3. Cleanup only removes items inside a short, fixed list of folders. App removal, and build folders and installers, each have their own narrow check.
4. Your own files, cloud files, passwords, and keys are never touched. The one exception: project build folders and installers, which can be in Documents, Desktop, or Downloads.
5. Anything that changed since you reviewed it is skipped.
6. Cleanup never uses admin rights.

## How A Cleanup Runs

1. Find every item the selected rules cover. Nothing changes yet.
2. Show every path.
3. Show the number of items, their total size, and that they go to the Trash.
4. Ask you to confirm.
5. Check each item again, right before it moves.
6. Ask Finder to move it to the Trash, so Put Back can restore it.

* There is no command or option that skips the review or the question.
* The first cleanup makes macOS ask whether your terminal may control Finder.
* Afterwards, neet shows how much space the items take in the Trash. That space is freed when you empty the Trash.

## Allowed Folders

* Cleanup may only move items inside these folders, never a folder itself.
* Each one holds files that apps can make again. None holds installed software or your own work.

| Folder | Holds |
| --- | --- |
| `~/Library/Caches` | App caches, including Homebrew, pip, pnpm, and Yarn. |
| `~/Library/Containers/<app>/Data/Library/Caches` | Caches of apps that run in a sandbox. Only this folder in each app's container, nothing else in it. |
| `~/Library/Logs` | App logs. |
| `~/Library/Saved Application State` | Windows that apps reopen where you left off. |
| `~/Library/Developer/Xcode/DerivedData` | Xcode build files. |
| `~/Library/Developer/Xcode/iOS DeviceSupport` | Files Xcode copies from connected devices. |
| `~/Library/Developer/CoreSimulator/Caches` | Simulator caches. Never the simulators themselves. |
| `~/.npm/_cacache` | The npm download cache. |
| `~/.cargo/registry/cache` | Downloaded Rust packages. |
| `~/.cache/pip` | The pip cache, when pip keeps it here. |

* Adding a folder needs a change to this table and to the code, together.
* A rule can never add a folder.
* App removal has its own, narrower list. See [App Removal](#app-removal).

## Protected Folders

* neet refuses these, and everything inside them:

| Path | Why |
| --- | --- |
| `/System`, `/usr`, `/Library` | macOS and shared system files. |
| Your home folder itself | Far too broad. |
| `~/Documents`, `~/Desktop`, `~/Downloads` | Your own files. Only [build folders and installers](#build-folders-and-installers) may be taken from them. |
| `~/Library/Mobile Documents`, `~/Library/CloudStorage` | iCloud and other cloud files. |
| `~/Library/Keychains`, `~/.ssh` | Passwords and keys. |
| Any `.git` folder | Project history. |
| `~/Library/Developer/Xcode/Archives` | Builds of apps you shipped. They cannot be made again. |
| The top folder of any disk | Far too broad. |

## The Path Check

* Every item must pass this check before it can be shown for review. It:
  1. Refuses empty and relative paths, and `..` parts.
  2. Follows any links in the folders above the item, to find where it really is. The item itself is judged as it is, so a link is judged as a link.
  3. Refuses anything protected.
  4. Requires the item to be inside an allowed folder, and not the folder itself.
  5. Refuses a link, unless it leads somewhere inside the same allowed folder that is not protected.
  6. Records the item's real path and its identity on disk, which stays the same even if it is renamed.
* Only this check, and the ones for [App Removal](#app-removal) and [Build Folders And Installers](#build-folders-and-installers), can approve an item. No other code can skip them.
* Links are never moved. Finder might move what a link points to instead of the link.

## The Check Before Moving

* Right before each item moves, neet checks that:
  1. The item still passes the same check that approved it.
  2. Its real path and identity on disk are unchanged.
  3. The apps its rule needs closed are still closed.
  4. Nothing inside it changed more recently than its rule allows.
* If any check fails, or neet cannot be sure, the item is skipped and listed with the reason.
* If neet cannot tell whether an app is open, it treats it as open.
* **Known gap:** Finder moves a path, not an open file. A very short moment remains between the last check and the move. Checking right before each move keeps it as short as possible.

## Cleanup Rules

* Built in rules ship inside neet. Your own rules go in `~/.config/neet/rules/`, as `.toml` files.
* A rule with a mistake is left out and listed on the Deep Clean screen. The other rules still load.
* Your rule replaces a built in rule with the same `id`.

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
| `name` | Yes | The name on the Deep Clean screen. |
| `category` | Yes | `developer`, `package`, `application`, `browser`, `logs`, or `system`. |
| `tier` | Yes | The risk level: `safe`, `caution`, or `expert`. |
| `paths` | Yes | Full paths. `~` only at the start. `*` matches any name within one folder, and only after the allowed folder part. |
| `description` | Yes | What the files are, and what happens when they are removed. |
| `regenerates` | No | Whether the app makes the files again. |
| `requires_quit` | No | Bundle IDs of apps that must be closed first. |
| `min_age_days` | No | Skip anything changed within this many days. A folder counts as changed when anything inside it changed. |

* neet refuses unknown fields, any pattern other than `*`, and any path outside the allowed folders.
* Your own rules never start selected. A `safe` level in your rule counts as `caution`.
* Before adding a built in rule:
  1. Run the tests.
  2. Read every path it finds on a real Mac.
  3. Write down the macOS and app versions you checked.

## App Removal

* App removal uses its own check, not the allowed folders. It follows the same review, question, check before moving, and Trash steps as a cleanup.

### Which Apps

* Only an app directly inside `/Applications` or `~/Applications`.
* Refused:
  1. Apple's apps, whose bundle ID starts with `com.apple.`.
  2. Links, such as `/Applications/Safari.app`.
  3. Apps in subfolders, such as `/Applications/Utilities`.
  4. Apps without a bundle ID neet can read.
  5. Apps that are open, checked when you open one and again before each move.
* An app owned by another user, such as one from the App Store, makes Finder ask for your password. neet never uses `sudo`.

### Which Files

* Only items directly inside these folders in `~/Library`, found by exact name. Capital letters do not matter, as on the Mac's disk. Nothing partial.
* `<id>` is the app's bundle ID, such as `com.hnc.Discord`. `<name>` is the app's name, such as `Discord`.

| Folder | Name | Starts Selected | Note |
| --- | --- | --- | --- |
| The app itself | | Yes | |
| `Caches` | `<id>` | Yes | |
| `Logs` | `<id>` | Yes | |
| `Saved Application State` | `<id>.savedState` | Yes | |
| `HTTPStorages` | `<id>`, `<id>.binarycookies` | Yes | |
| `WebKit` | `<id>` | Yes | |
| `Application Support` | `<id>` | No | may be your data |
| `Containers` | `<id>` | No | may be your data |
| `Preferences` | `<id>.plist` | No | settings |
| `Group Containers` | `group.<id>`, or a 10 character team ID then `.<id>` | No | shared with other apps |
| `LaunchAgents` | `<id>.plist` | No | starts on its own |
| `Application Support`, `Logs` | `<name>` | No | matched by name |

* You can select anything listed. Nothing outside this table is ever listed.
* Links are refused, whatever they point to.
* Protected folders still apply.

## Build Folders And Installers

* Quick Clean can move project build folders, installers in Downloads, and Docker's disk image to the Trash. It uses its own check, not the allowed folders, and follows the same review, question, check before moving, and Trash steps as a cleanup.
* Every item starts selected. You can clear any of them before the review.

| Item | Taken only when |
| --- | --- |
| `node_modules` | It is a folder, inside your home folder. |
| `target` | It is a folder with a `Cargo.toml` beside it. |
| Installers | The name ends in `.dmg`, `.pkg`, `.iso`, or `.xip`, in any case, anywhere inside `~/Downloads`. |
| Docker's disk image | It is a file at exactly `~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`. Only [resetting Docker](#resetting-docker) takes it, after Docker Desktop has quit. |

* Refused:
  1. Links, whatever they point to.
  2. Anything else in `~/Library`, or in a hidden folder directly in your home folder, such as `~/.vscode`. Tools keep their own copies there.
  3. A build folder inside another one, such as `node_modules` inside `node_modules`. The outer one is taken whole.
  4. Every other protected folder still applies, such as `.git`, iCloud, keychains, and `~/.ssh`.
* Build folders come back when you install or build again. An installer only comes back if you download it again.

## Tools neet Runs

* Simulator runtimes, simulators, and Docker's images and volumes cannot go to the Trash: macOS keeps runtimes in secure storage, and Docker keeps the rest inside its own disk image. neet asks their own tool to remove them, from Quick Clean.
* This is the only way neet removes anything permanently. Runtimes and images can be downloaded again. Simulators and volumes hold data that cannot.

| Tool | Command | Removes |
| --- | --- | --- |
| Simulator runtimes | `xcrun simctl runtime delete <id>`, once per runtime you select | The runtime. |
| Simulators | `xcrun simctl delete <udid>`, once per simulator | The simulators on each runtime that was removed, and simulators left without a runtime if you select them. Their apps and data go too. |
| Docker | `docker system prune --all --force` | Stopped containers, networks no container uses, every image no container uses, and the build cache. Volumes are kept. |
| Docker volumes | `docker volume rm -- <name>`, once per volume you select | A volume no container uses, with its data. Docker refuses if a container uses it. |

* Before any of these runs, neet:
  1. Lists exactly what the tool reports: each runtime with its version, build, size, when it was last used, and the simulators on it; simulators left without a runtime; or Docker's images, containers, volumes, and build cache with what can be reclaimed, and each volume no container uses.
  2. Starts with nothing selected for runtimes, simulators, and volumes.
  3. Asks in a red box that lists what goes and says the removal is permanent and does not go to the Trash. Only `y` goes ahead.
* Only IDs made of letters, digits, and hyphens are passed to `simctl`, so they can never be `all` or an option. Only volume names made of letters, digits, `_`, `.`, and `-`, starting with a letter or digit, are passed to `docker`, after `--`.
* None of these commands uses admin rights. neet never starts Docker on its own; `o` opens Docker Desktop only when you press it.

### Resetting Docker

* `x` on the Docker screen resets Docker: every image, container, and volume goes at once, by moving Docker's disk image to the Trash. It is not permanent: Put Back restores it.
* neet asks first, in a yellow box that names the size and says what goes with it. Only `y` goes ahead. Then neet:
  1. Asks Docker Desktop to quit, through `osascript`, and waits up to a minute until no part of `/Applications/Docker.app` is running. If it is still running, nothing moves.
  2. Checks the disk image with the [build folder check](#build-folders-and-installers), which takes only that one file.
  3. Moves it to the Trash through Finder, like any other cleanup.
* Docker Desktop makes a new, empty disk image the next time it opens. To undo, Put Back `Docker.raw` before opening Docker Desktop again.

## Planned Features

* Later features change files or settings in place, not through the Trash. Each has a fixed list of what it may change.

| Feature | May Change | How |
| --- | --- | --- |
| Dotfiles | Only the listed settings files. | Edit with a backup |
| Shell PATH | Only the shell files among the dotfiles. Never `/etc/paths`. | Edit with a backup |
| SSH permissions | `~/.ssh` and the files directly inside it. | Change permissions |
| SSH known hosts | `~/.ssh/known_hosts` | `ssh-keygen -R` |
| SSH agent | Nothing on disk. | `ssh-add` |
| Startup items | Whether an item runs. Never its file. Never items in `/System/Library`. | `launchctl`, with `sudo` when needed |
| Power settings | Low and High Power Mode, graphics switching, and three wake settings. | `sudo pmset` |
| Refresh rate | The mode of a connected display. It switches back after 15 seconds unless you keep it. | CoreGraphics |

* Every change:
  1. Shows what it will do first.
  2. Saves the old value, in `~/.local/state/neet/`, so it can be undone.
  3. Writes files by writing a new copy and swapping it in, so a crash never leaves half a file.
  4. Refuses links that lead out of your home folder.
* A command that needs admin rights is shown first, then run once with `sudo`.
* neet itself never runs as root.
* neet never reads the inside of a private key.
* Dotfile exports leave out private keys and known secret files, and show anything that looks like a password before writing. This is a review, not a promise that nothing secret remains.
* The AI tools view only reads settings, instruction files, and skills. It never opens sign in files, chat history, or databases.

## Tests

* Automated tests use temporary folders. They never use your real home folder, and never change real Mac settings.
* They cover:
  1. `..` parts, doubled slashes, and paths outside the allowed folders.
  2. Allowed folders themselves, the home folder, and the top of a disk.
  3. Protected folders written in other ways.
  4. Links that lead out, and links swapped in after the review.
  5. Files replaced after the review.
  6. Names neet does not support.
  7. Your own rules reaching outside the allowed folders, and a `safe` level in your rule counting as `caution`.
  8. Unsupported patterns, and `*` too early in a path.
  9. Paths in a container outside its `Caches` folder, and Xcode archives.
  10. An old folder holding a recently changed file.
  11. App removal refusing Apple's apps, links, apps in subfolders, open apps, files outside the table, and partial names.
  12. Build folders and installers: accepted in Documents, Desktop, and Downloads, and refused when nested, in `~/Library` or a hidden folder, inside `.git` or `~/.ssh`, a link, a lone `target`, or a file that is not an installer.
  13. Tool commands: only runtime IDs reach `simctl`, and what `simctl` and `docker` print is read as data.
* Before release, a harmless test item is moved to the Trash by hand and restored with Put Back. This passed on macOS 26.5.
* Removing an app owned by root was tried the same way, and Put Back restored it.
* Planned features add their own tests, for changes outside their lists, links that lead out, and undoing each change.
