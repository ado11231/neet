# Features

* Current features and their limits.
* Planned features are marked clearly.

## Contents

* [Status](#status)
* [Disk](#disk)
* [Cleanup](#cleanup)
* [Apps](#apps)
* [Dotfiles](#dotfiles)
* [AI Tools](#ai-tools)
* [Startup](#startup)
* [The Program](#the-program)
* [Planned Features](#planned-features)
* [Terms](#terms)

## Status

| Status | Meaning |
| --- | --- |
| **Done** | Works in the program, and is tested. |
| **Proposed** | An idea that is not built yet. The design may still change. |

## Disk

Shows where your disk space goes. Looking never changes a file.

| Feature | What It Does | Status |
| --- | --- | --- |
| Home folder scan | Scans in the background without following links or entering other disks. | Done |
| Real sizes | Counts disk space used by files, and files with several names only once. | Done |
| Disk gauge | Shows used and free space, plus why Finder may show more free space. | Done |
| Skipped folders | Lists unreadable folders and Full Disk Access help. | Done |
| Disk browser | Browse folders by size, with selected file details. View only. | Done |
| Large Files | Filter files by size and last change, then show one in Disk. | Done |
| Space breakdown | Splits used space into the home folder, other Data volume folders, macOS volumes, and what cannot be measured without admin rights. View only. | Done |
| Treemap | A view where each folder's area shows its size. | Proposed |

* Sizes are estimates. On APFS, the file system macOS uses, copied files can share space, so a cleanup may free less than it shows.
* A file's age means when it last changed, not when it was last opened.

## Cleanup

* Moves files your apps can make again, such as caches, to the Trash.
* Every cleanup shows each path and asks before anything moves.

| Feature | What It Does | Status |
| --- | --- | --- |
| Quick Clean | Overview of caches, build folders, installers, Trash, Docker, simulators, and temporary files. Open a row for its actions. | Done |
| Preview | Shows paths and sizes without changing files. | Done |
| Review and confirm | Lists every path, count, and size before asking. Required for every cleanup. | Done |
| Move to the Trash | Finder moves items so Put Back can restore them. Empty Trash to free space. | Done |
| Empty the Trash | Lists Trash items and asks before permanent deletion through Finder. | Done |
| Check before moving | Skips changed files and files whose app opened after review. | Done |
| Risk levels | Marks each rule safe, caution, or expert. | Done |
| Open app check | A rule cannot be selected while its app is open. | Done |
| Busy app advice | Names the app to quit before refreshing. Other skip reasons keep their own explanation. | Done |
| Recent file check | A rule can skip anything changed in the last few days. | Done |
| Your own rules | Loads rules from `~/.config/neet/rules/`. Fixed path limits still apply. None start selected. | Done |
| Project build folders | Review selected `target` folders beside `Cargo.toml`, and `node_modules`, before moving to Trash. | Done |
| Simulators and Docker | Review runtime and device removal, or Docker prune and selected unused volumes. Permanent, with a warning first. | Done |
| Leftover simulator downloads | Shows runtime downloads macOS keeps after the runtime is removed, and how to reuse them. View only, since macOS protects them. | Done |
| Docker space details | Labels estimates and lists running containers, images, and Compose projects. Explains when prune frees no space. | Done |
| Reset Docker | Stops Docker Desktop and moves its disk image to Trash. Restore it by hand before reopening Docker. | Done |
| Installers in Downloads | Review selected `.dmg`, `.pkg`, `.iso`, and `.xip` files before moving to Trash. | Done |

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
* User logs can include old wireless network reports. The saved network settings are elsewhere and stay untouched.

## Apps

| Feature | What It Does | Status |
| --- | --- | --- |
| Remove App | Review an app and its Library files. Data, settings, and shared files start unselected. Apple and open apps are refused. | Done |

What Remove App may find and remove is in [SAFETY.md](SAFETY.md#app-removal).

## Dotfiles

| Feature | What It Does | Status |
| --- | --- | --- |
| List | Lists settings files by program, with status and preview. Enter shows actions. | Done |
| chezmoi status | Compares files with saved copies without running chezmoi. | Done |
| Edit | Edit a copy, check it, review the diff, and save with a backup. Applies the chezmoi source when allowed. | Done |
| Save or use the saved version | Save your file in chezmoi or restore its saved copy. | Done |
| Add to your dotfiles | Review and add a listed file to chezmoi. | Done |
| Backups and restore | Keeps old versions under `~/.local/state/neet/backups/dotfiles`. Restore after review. | Done |
| Diff | Compare with the chezmoi source or latest backup. | Done |
| Configure | Review settings for Git, tmux, Starship, kitty, Ghostty, mise, and zsh. zsh edits stay in a marked block. | Done |
| Export | Review files for secrets, commit selected files, then ask before pushing. Can start a private dotfiles repository. | Done |
| Start a repository | Without chezmoi, writes the dotfiles you pick into a new repository in chezmoi's folder and layout, commits them, and offers to make a private GitHub repository with `gh`. | Done |

What may change, and how, is in [SAFETY.md](SAFETY.md#dotfiles). neet runs chezmoi only when its templates and hooks cannot run a program.

## AI Tools

| Feature | What It Does | Status |
| --- | --- | --- |
| Browse | Known Claude Code, Codex, and Gemini files, chats, and skills, including shared and current project files. | Done |
| Read | Text previews and supported chat messages. Credentials and databases stay excluded. | Done |
| Chat cleanup | Select chats by title and project, review, then move transcripts and Claude Code's related folders to Trash. History and index entries are removed with a backup. Results show failures and unknown outcomes. | Done |
| Continue a chat | Opens the chat in Claude Code or Codex from its project folder, then returns to neet. | Done |
| Archive | Archives or unarchives a Codex chat with `codex archive` and `codex unarchive`. | Done |
| Link a skill | Review a local skill folder, then link it into the chosen tool. Keep the source folder in place. | Done |
| Edit and restore | Review user skill edits and backups. Keep failed drafts for repair. Bundled skills stay view only. | Done |
| Skill sources | Mark each skill as your copy, a link, bundled, or from a plugin, with the plugin's version and on or off state. | Done |
| Remove a skill link | Review removing a linked skill. The folder it points to stays, and `u` puts the link back. | Done |
| Finder | Reveals the selected file, including hidden files. | Done |

* Settings and instructions are view only. Chat cleanup leaves cloud history and Codex's own database.
* Only known folders are searched. See [AI Tools safety](SAFETY.md#ai-tools) for paths and read limits.

## Startup

| Feature | What It Does | Status |
| --- | --- | --- |
| List | Lists user agents, shared agents and daemons, and app helpers. Shows current state and signature. | Done |
| Open at login | Open System Settings to see and change login apps. | Done |
| Turn off and back on | Review changes to your own launch agents. Saves the old state and leaves the plist alone. | Done |

* What is read, and what may change, is in [SAFETY.md](SAFETY.md#startup-items). Items installed for every user, and Apple's, stay view only.
* If a plist changes after it was listed, neet refuses the turn action. Press `r` to look again before trying it.
* If a turn action only partly finishes, the result says what changed and what failed. Saved attempts stay separate from completed changes.

## The Program

| Feature | What It Does | Status |
| --- | --- | --- |
| Home menu | Disk first. Features not built yet are marked soon. | Done |
| Help | `?` lists the keys for the current screen. | Done |
| Command line | `neet` opens the app. The only options are `--help` and `--version`. | Done |
| Refuse to run as root | Stops with an explanation if started with `sudo`. | Done |
| Install | Mac downloads, an installer, and `cargo install neet`. | Done |
| Homebrew | A Homebrew install option. | Proposed |

## Planned Features

These features are not built yet. Each needs checking on real Macs.

| Area | What It Would Do |
| --- | --- |
| Shell PATH | Show the folders your shell searches for programs, in order, flag problems, show which copy of a program runs, and reorder them with a backup. |
| Power and display | Switch power modes, graphics switching, and display refresh rate, and show what keeps the Mac awake. Every change can be undone. |
| Treemap | Show folder sizes as areas. |
| Saved preferences | Remember your choices between runs. |

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
