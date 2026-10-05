# Features

* Everything neet does today, everything planned, and what it will never do.
* For anyone deciding what to build next, or checking what neet can do.
* For screens and keys, read [INTERFACE.md](INTERFACE.md). For the safety rules, read [SAFETY.md](SAFETY.md). For progress, read [ROADMAP.md](ROADMAP.md).

## Contents

1. [Status](#status)
2. [Disk](#disk)
3. [Cleanup](#cleanup)
4. [Apps](#apps)
5. [Dotfiles](#dotfiles)
6. [The Program](#the-program)
7. [Planned Features](#planned-features)
8. [Open Questions](#open-questions)
9. [Never Planned](#never-planned)
10. [Terms](#terms)

## Status

| Status | Meaning |
| --- | --- |
| **Done** | Works in the program, and is tested. |
| **Proposed** | An idea for after the first release. The design may still change. |

## Disk

* Shows where your disk space goes. Looking never changes a file.

| Feature | What It Does | Status |
| --- | --- | --- |
| Home folder scan | Scans your home folder in the background as soon as neet opens. It does not follow links, and does not enter other disks. | Done |
| Real sizes | Measures the space each file really takes on disk, and counts a file with several names only once. | Done |
| Disk gauge | Shows how full the disk is, from the disk's own totals, updated every 5 seconds. Also says how much more free space Finder shows, and why. | Done |
| Skipped folders | Lists the folders the scan could not read, with the reason, and explains how to turn on Full Disk Access. | Done |
| Disk browser | Browses folders largest first, with a bar for each item's share of its folder. | Done |
| Large Files | Lists files above a size you choose, optionally only ones unchanged for a while, and shows them in Disk. | Done |
| Space breakdown | Explains why the disk's used space is larger than the scan: apps, macOS itself, Time Machine snapshots, and space macOS can free on its own. View only. | Proposed |
| Treemap | A view where each folder's area shows its size. | Proposed |

* Sizes are estimates. On APFS, the file system macOS uses, copied files can share space, so a cleanup may free less than it shows.
* A file's age means when it last changed, not when it was last opened.

## Cleanup

* Moves files your apps can make again, such as caches, to the Trash.
* Every cleanup shows each path and asks before anything moves.

| Feature | What It Does | Status |
| --- | --- | --- |
| Quick Clean | One table of everything that can be cleared: what the rules found, project build folders, and installers in Downloads, which neet clears, and the Trash, the Docker disk image, simulator runtimes, and temporary files, with how to remove each yourself. Looking never changes a file. | Done |
| Preview | Finds everything the cleanup rules cover, with sizes, and changes nothing. | Done |
| Review and confirm | Lists every path, then asks with the item count and total size. It cannot be skipped. | Done |
| Move to the Trash | Moves items through Finder, so Put Back restores them. Space is freed when you empty the Trash. | Done |
| Check before moving | Checks each item again right before it moves. Anything that changed, or whose app opened, is skipped. | Done |
| Risk levels | Marks each rule `safe`, `caution`, or `expert`, which decides whether it starts selected. | Done |
| Open app check | A rule cannot be selected while its app is open. | Done |
| Recent file check | A rule can skip anything changed in the last few days. | Done |
| Your own rules | Loads extra rules from `~/.config/neet/rules/`. They can never reach outside the allowed folders, and never start selected. | Done |
| Project build folders | Moves build folders in your code projects, `target/` beside a `Cargo.toml` and `node_modules/`, to the Trash from Quick Clean. You pick which, then review. | Done |
| Simulators and Docker | From Quick Clean, runs `xcrun simctl` to remove the runtimes you pick with their simulators, and simulators left without a runtime, or `docker system prune --all` with the unused volumes you pick, after a red question. Permanent, not the Trash. | Done |
| Reset Docker | Stops Docker Desktop and moves its whole disk image to the Trash, after a question. Moving it back from the Trash restores it. | Done |
| Installers in Downloads | Moves `.dmg`, `.pkg`, `.iso`, and `.xip` files in Downloads to the Trash from Quick Clean. You pick which, then review. | Done |

### Risk Levels

| Level | What It Means | Starts Selected |
| --- | --- | --- |
| `safe` | Your Mac makes the files again by itself. The only cost is time. | Yes |
| `caution` | You may need to download or sign in again. | No |
| `expert` | The files may not exist anywhere else. You type the rule's ID to select it. | No |

### Built In Rules

* Each rule empties a folder and keeps the folder itself.
* Checked on macOS 26.5 with Xcode 26.6.

| Rule | Folder | Level | Close First | Keeps Anything Newer Than |
| --- | --- | --- | --- | --- |
| Xcode DerivedData | `~/Library/Developer/Xcode/DerivedData` | `safe` | Xcode | |
| Simulator caches | `~/Library/Developer/CoreSimulator/Caches` | `safe` | Simulator | |
| Xcode device support | `~/Library/Developer/Xcode/iOS DeviceSupport` | `caution` | Xcode | 30 days |
| Playwright browsers | `~/Library/Caches/ms-playwright` | `caution` | | |
| npm cache | `~/.npm/_cacache` | `caution` | | |
| Homebrew downloads | `~/Library/Caches/Homebrew` | `caution` | | |
| pnpm cache | `~/Library/Caches/pnpm` | `caution` | | |
| Cargo downloads | `~/.cargo/registry/cache` | `caution` | | |
| node-gyp headers | `~/Library/Caches/node-gyp` | `caution` | | |
| pip cache | `~/Library/Caches/pip`, `~/.cache/pip` | `caution` | | |
| Yarn cache | `~/Library/Caches/Yarn` | `caution` | | |
| Chrome cache | `~/Library/Caches/Google/Chrome` | `caution` | Chrome | |
| User logs | `~/Library/Logs` | `caution` | | 7 days |
| Saved window state | `~/Library/Saved Application State` | `caution` | | 30 days |

* Playwright browsers do not come back on their own. Run `npx playwright install` to get them back.
* User logs can include old copies of your wireless networks, which macOS once wrote to `~/Library/Logs` for diagnostics. Your saved networks are kept elsewhere, in a folder neet never touches, so moving these copies does not change how your Mac connects.

## Apps

| Feature | What It Does | Status |
| --- | --- | --- |
| Remove App | Lists your apps, then shows an app with the files it keeps in your Library folder. Caches and logs start selected. Anything that may be your data, settings, or shared with other apps starts unselected. Refuses Apple's apps, and any app that is open. | Done |

* What Remove App may find and remove is in [SAFETY.md](SAFETY.md#app-removal).

## Dotfiles

| Feature | What It Does | Status |
| --- | --- | --- |
| List | Shows your settings files, such as `~/.zshrc` and `~/.gitconfig`, grouped by program, with size, last change, and a preview. | Done |
| chezmoi status | Reads chezmoi's folder without running it, and shows whether each file matches its source file. | Done |
| Edit | Opens a copy in your editor, then checks it, shows the diff, and writes it after a backup. With chezmoi, writes the source file, then applies it. | Done |
| Keep or put back | For a file that differs from its source file: keeps this version in chezmoi, or puts chezmoi's back. | Done |
| Backups and restore | Keeps every old version in `~/.local/state/neet/backups/dotfiles`, and restores one after showing what it changes. | Done |
| Diff | Shows how a file differs from its source file in chezmoi, or what changed since its last backup. | Done |
| Configure | Changes a program's settings one at a time, then reviews them like an edit. Git first; more programs to come. | Done |
| Export | Reviews chezmoi's changed source files for secrets, commits the ones you pick, and pushes when you press `y`. | Done |
| Start a repository | Without chezmoi, writes your dotfiles into a new repository in chezmoi's layout, ready for GitHub. | Proposed |

* What may change, and how, is in [SAFETY.md](SAFETY.md#dotfiles). neet runs chezmoi only when its templates and hooks cannot run a program.

## The Program

| Feature | What It Does | Status |
| --- | --- | --- |
| Home menu | Opens on a menu of every feature. Features not built yet are shown dimmed. | Done |
| Help | `?` lists the keys for the current screen. | Done |
| Command line | `neet` opens the app. The only options are `--help` and `--version`. | Done |
| Refuse to run as root | Stops with an explanation if started with `sudo`. | Done |
| Install | Ready made programs for Apple silicon and Intel, an install script, and `cargo install neet`. | Done |
| Homebrew | `brew install neet`. | Proposed |

## Planned Features

* All of these are proposed for after the first release. Each needs checking on real Macs before it is built.

| Area | What It Would Do |
| --- | --- |
| Startup items | List programs that start on their own, including ones with no icon, and turn them off in a way you can undo. Items that belong to macOS are view only. |
| SSH | Show hosts, key details, and agent keys, fix file permissions that are too open, and remove old known hosts. Never shows a private key. |
| Shell PATH | Show the folders your shell searches for programs, in order, flag problems, show which copy of a program runs, and reorder them with a backup. |
| AI tool files | Show the settings, instruction files, and skills that Claude Code and Codex keep on your Mac. View only. |
| Power and display | Switch power modes, graphics switching, and display refresh rate, and show what keeps the Mac awake. Every change can be undone. |
| Space breakdown | Explain the gap between the disk's used space and the scan. |
| Project build folders | Find old build folders in code projects, only when Git ignores them. |
| Treemap | Show folder sizes as areas. |
| Saved preferences | Remember your choices between runs. |

## Open Questions

* These must be answered before the feature is built. None of them may weaken a safety rule.

| Feature | Question |
| --- | --- |
| Mail downloads | Mail keeps attachments in its own folder. Can that folder be cleaned without losing your only copy of an attachment? |
| System logs | Can they be cleaned without admin rights? |
| Startup items | How is each kind found, turned off, and turned back on, on every supported macOS version? |
| Power and display | Which settings exist on each Mac, and does switching back a refresh rate always work? |
| Space breakdown | Which numbers does macOS report reliably for volumes, snapshots, and purgeable space? |
| Project build folders | Many projects live in Documents or Desktop, which neet never touches. Can a narrow check allow only ignored build folders there? |
| Shell PATH | How can neet find where each folder was added, and should edits go in a block neet owns? |
| AI tool files | Where does each tool keep project skills, across versions? |
| Treemap and preferences | How should they work, and what should be saved? |

## Never Planned

| Feature | Why |
| --- | --- |
| Deleting files permanently, or emptying the Trash | Everything goes to the Trash, so it can be put back. |
| Cleaning without a review | You always see every path first. |
| Removing Apple's apps | macOS needs them. |
| Deleting duplicate files automatically | Only you know which copy matters. |
| Freeing memory | macOS already manages memory. |
| Turning off System Integrity Protection | It protects macOS itself. |
| Changing `nvram` | A mistake can stop the Mac from starting. |
| Showing, creating, or deleting SSH keys | Use `ssh-keygen` for that. |
| Editing AI tool files | Use the tool itself. |
| Syncing dotfiles on its own, or pulling and merging | neet commits and pushes only when you press `y`. Use chezmoi or Git to pull changes from another Mac. |
| Mac settings unrelated to speed, battery, or sleep | Out of scope. |

## Terms

| Term | Meaning |
| --- | --- |
| **Rule** | A short entry that says which folder neet may empty, and how risky that is. |
| **Risk level** | `safe`, `caution`, or `expert`. |
| **Put Back** | The Finder command that returns a Trash item to where it was. |
| **Full Disk Access** | A macOS permission that lets an app read protected folders. |
| **Purgeable space** | Space macOS clears on its own when the disk runs low, such as iCloud files kept offline and old caches. Finder counts it as free. |
| **APFS** | The file system macOS uses. Copied files can share space on it. |
| **Dotfile** | A settings file in your home folder, such as `~/.zshrc`. |
| **chezmoi** | A dotfile manager. It keeps a source copy of each dotfile in a Git repository, and writes them into your home folder. |
| **`PATH`** | The ordered list of folders your shell searches when you type a program name. |
| **`sudo`** | Runs one command with admin rights, after asking for your password. |
