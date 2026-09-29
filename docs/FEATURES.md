# Features

* Every feature planned for neet, grouped by what it helps you do.
* Each feature lists its release phase and whether it exists yet.
* For anyone deciding what to build next, or checking what neet will and will not do.
* For screens and keys, read [INTERFACE.md](INTERFACE.md). For safeguards, read [SAFETY.md](SAFETY.md). For milestones, read [ROADMAP.md](ROADMAP.md).
* Status was checked against the code on September 29, 2026.

## Contents

1. [Where Things Stand](#where-things-stand)
2. [Status And Phases](#status-and-phases)
3. [Features By Phase](#features-by-phase)
4. [Disk Analysis And File Discovery](#disk-analysis-and-file-discovery)
5. [Cleanup And Rules](#cleanup-and-rules)
6. [Application Management](#application-management)
7. [Startup And Background Services](#startup-and-background-services)
8. [SSH Management](#ssh-management)
9. [Dotfile Management](#dotfile-management)
10. [Shell PATH](#shell-path)
11. [AI Coding Tool Files](#ai-coding-tool-files)
12. [Performance, Power, And Displays](#performance-power-and-displays)
13. [Interface, Preferences, And Distribution](#interface-preferences-and-distribution)
14. [Safety And Recovery](#safety-and-recovery)
15. [Design Questions](#design-questions)
16. [Excluded Features](#excluded-features)
17. [Key Terms](#key-terms)

## Where Things Stand

* Parts of the scanner exist in the `neet-core` library.
* The `neet` program opens a Home menu. It has no working feature screens, no cleanup, and no Mac settings changes yet.
* Nothing below is available to use until it is connected to the program.

## Status And Phases

* Status says how much is built. Phase says which release it is meant for.
* A phase is a target. It never means a feature is done.

| Status | Meaning |
| --- | --- |
| **Built** | Written and tested in the library, but not yet used by the program. |
| **In Progress** | Some of the code exists. The feature is not finished. |
| **Planned** | Part of P1 or P1.5, but not built. Some details are still open. |
| **Proposed** | A P2 idea with a draft design. Scope and Mac behavior still need checking. |
| **Excluded** | Outside what neet will do. |

| Phase | What It Covers | Milestones |
| --- | --- | --- |
| **P1** | The first release: disk analysis, and reviewed cleanup to the Trash. | M0 to M5 |
| **P1.5** | Large and old file filters, and an app removal review. | M6 |
| **P2** | Startup items, SSH, dotfiles, shell PATH, AI coding tool files, speed and battery settings, saved preferences, a treemap, a space breakdown, and project build cleanup. | M7 |

## Features By Phase

| Area | P1 | P1.5 | P2 |
| --- | --- | --- | --- |
| [Disk analysis](#disk-analysis-and-file-discovery) | Scan, sizes, browser, Home status, progress | Large and old file filters | Treemap, space breakdown |
| [Cleanup](#cleanup-and-rules) | Dry run, review, Trash, risk tiers, rules | | Project build folders |
| [Apps](#application-management) | | App removal review | |
| [Startup](#startup-and-background-services) | | | Inventory, details, disable and restore |
| [SSH](#ssh-management) | | | Hosts, keys, agent, permissions, known hosts |
| [Dotfiles](#dotfile-management) | | | Inventory, safe editing, export |
| [Shell PATH](#shell-path) | | | PATH check, which program wins, reorder and edit |
| [AI tools](#ai-coding-tool-files) | | | Claude Code and Codex settings, instructions, and skills |
| [Settings](#performance-power-and-displays) | | | Power, graphics, refresh rate, sleep and wake |
| [Interface](#interface-preferences-and-distribution) | Home menu, help, install | | Saved preferences |
| [Safety](#safety-and-recovery) | Path limits, identity check, no `sudo` cleanup | | Backups, undo, scoped admin commands |

## Disk Analysis And File Discovery

* Shows where your disk space goes.
* Scanning and filtering never change files.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Home folder scan | Scans your home folder into a tree of sizes. It reads folders on several threads, does not follow symbolic links, and does not cross onto another disk. | P1 | Built |
| Size on disk | Measures the space a file really uses, so files with empty parts are measured correctly. | P1 | Built |
| Hard link tracking | Spots files with more than one name, so their space is counted once. | P1 | Built |
| Folder tree and totals | Stores each folder's parent and children, adds up folder totals, and rebuilds paths. | P1 | Built |
| Disk browser | Browses folders in columns, sorted by size, name, or item count, with a bar for each folder's share and a preview of the selected row. Planning a cleanup from here is not built. | P1 | In Progress |
| Home status and disk gauge | On the Home screen, shows free and used space, scan progress, whether the scan was complete, and how much Clean can free. The gauge and scan status are built. What Clean can free waits on Clean. | P1 | In Progress |
| Scan progress and warnings | Keeps the screen responsive. Lists folders it could not read and disks it skipped. Marks a scan as incomplete, including when Full Disk Access is missing. The scan runs in the background, Home shows progress and counts, and the Skipped screen lists each path with its reason and a Full Disk Access hint. | P1 | Built |
| Large and old file filters | Finds files above a size you choose, or not changed since a date you choose, and opens them in Disk. Finding a file does not make it a cleanup target. | P1.5 | Planned |
| Treemap | A view where each folder's area shows its size. How you interact with it is not designed yet. | P2 | Proposed |
| Space breakdown | Explains why the disk's used space and the home folder scan disagree. Splits the difference into parts, such as apps, macOS and its other volumes, local Time Machine snapshots, purgeable space, and folders the scan skipped. Whatever neet cannot explain is shown as unexplained, never hidden. View only. | P2 | Proposed |

* Folder sizes are estimates. On APFS, copies can share space, so neet cannot say exactly how much a cleanup will free.
* File age means when a file was last changed, not when it was last opened.
* The space breakdown only explains. It never deletes snapshots or frees purgeable space. macOS manages both.
* Which volume totals, snapshot sizes, and purgeable figures macOS reports reliably still needs checking. See [Design Questions](#design-questions).

## Cleanup And Rules

* Finds reviewed cleanup targets and explains what removing them costs.
* Moves only the items you approve to the Trash.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Dry run plan | Lists matching paths and changes nothing. Every cleanup starts here. | P1 | Planned |
| Path review and confirmation | Shows every target, the item count, and the estimated size, then asks before going on. You cannot skip it. | P1 | Planned |
| Move to the Trash | Moves confirmed items to the Trash through Finder, so Put Back restores them. Space is freed only when you empty the Trash yourself, and neet shows how much that will be. | P1 | Planned |
| Risk tiers | Labels each rule `safe`, `caution`, or `expert`. The tier explains the cost and decides how the rule is selected. | P1 | Planned |
| Open app checks | Blocks a rule while its app is open, and checks again right before moving anything. | P1 | Planned |
| Recent file protection | Lets a rule skip items changed within its minimum age. A folder counts as changed when anything inside it changed. | P1 | Planned |
| Selection totals | Shows how many items and how much estimated space your selected rules cover. | P1 | Planned |
| Bundled and user rules | Loads the reviewed TOML rules, plus your own from `~/.config/neet/rules/`. Your rule can replace a bundled rule with the same ID, but can never widen what cleanup may touch, and is never selected from the start. Loading and checking rules is built, with 14 bundled rules. See [Bundled Rules](#bundled-rules). | P1 | In Progress |
| Cleanup from Disk | Plans a cleanup for the item selected in Disk, with the same checks, review, and question as Clean. Items outside the allowed folders are refused. | P1 | Planned |
| Project build folders | Finds build output inside your code projects, such as `target/` and `node_modules/`, grouped by project and sorted by size or by how long since you last worked on the project. Uses the home folder scan, so it does not walk the disk again. Offers a folder only when Git ignores it. Waits on its [design question](#design-questions), because project folders are outside the cleanup roots. | P2 | Proposed |

### Cleanup Target Groups

* The first 14 bundled rules are written. See [Bundled Rules](#bundled-rules). The other groups are planned and not reviewed yet.
* Each rule's paths, risk tier, and supported app versions must be reviewed one by one.
* The category labels match the rule format in [SAFETY.md](SAFETY.md#cleanup-rules).

| Category | Target Group | What Cleanup Would Do |
| --- | --- | --- |
| `developer` | Xcode build files | Removes build output, such as DerivedData. Xcode rebuilds it next time. |
| `developer` | Simulators | Removes simulator caches in `~/Library/Developer/CoreSimulator/Caches`. Removing simulators themselves, and whether to use simulator tools, are not decided. |
| `developer` | Docker data | A possible cleaner that uses Docker's own tool. Targets, and a way to undo it that fits the Trash policy, are not decided. |
| `package` | Package manager caches | Removes the npm, Cargo, pip, pnpm, Yarn, and Homebrew download caches listed as cleanup roots. Later installs may download them again. |
| `application` | App caches | Removes named app caches inside the allowed folders. Each rule says what the app must rebuild. |
| `application` | Mail download caches | Not possible yet. They sit outside the cleanup roots. See [Design Questions](#design-questions). |
| `browser` | Browser caches | Removes reviewed browser cache files. Targets and costs must be written down before a rule is added. |
| `logs` | User logs | Removes chosen logs from allowed folders. They will no longer be there for troubleshooting. |
| `system` | Saved app state | Removes reviewed saved state files. The rule must explain how this affects reopening an app where you left off. |
| `system` | System logs | A possible cleaner. Where these logs live, and how to clean them without `sudo`, are not decided. |
| `project` | Project build folders | Removes build output in your code projects. See [Project Build Folders](#project-build-folders). Not a TOML rule, because the folders are found, not fixed paths. |

### Risk Tiers

| Tier | What It Means | How It Is Selected |
| --- | --- | --- |
| `safe` | The app can rebuild the files. The only cost is time. | Selected at the start. You still review the paths and confirm. |
| `caution` | You may need to download, index, or sign in again. | You select it yourself. |
| `expert` | Some files may not exist anywhere else. The rule must say so. | You select it and type a confirmation. |

* No tier skips the path checks or the review. The full rule fields and checks are in [SAFETY.md](SAFETY.md#cleanup-rules).

### Bundled Rules

* The rules live in `crates/neet-core/rules/`, one file per category.
* Tiers follow the table above as written. Anything that must be downloaded again is `caution`, so only files your Mac rebuilds by itself are selected at the start.
* Each rule removes what is inside its folder and keeps the folder itself.
* Reviewed on macOS 26.5 with Xcode 26.6.

| Rule | Paths | Category | Tier | Must Quit | Minimum Age |
| --- | --- | --- | --- | --- | --- |
| Xcode DerivedData | `~/Library/Developer/Xcode/DerivedData/*` | `developer` | `safe` | Xcode | None |
| Simulator caches | `~/Library/Developer/CoreSimulator/Caches/*` | `developer` | `safe` | Simulator | None |
| Xcode device support | `~/Library/Developer/Xcode/iOS DeviceSupport/*` | `developer` | `caution` | Xcode | 30 days |
| Playwright browsers | `~/Library/Caches/ms-playwright/*` | `developer` | `caution` | | None |
| npm cache | `~/.npm/_cacache/*` | `package` | `caution` | | None |
| Homebrew downloads | `~/Library/Caches/Homebrew/*` | `package` | `caution` | | None |
| pnpm cache | `~/Library/Caches/pnpm/*` | `package` | `caution` | | None |
| Cargo downloads | `~/.cargo/registry/cache/*` | `package` | `caution` | | None |
| node-gyp headers | `~/Library/Caches/node-gyp/*` | `package` | `caution` | | None |
| pip cache | `~/Library/Caches/pip/*`, `~/.cache/pip/*` | `package` | `caution` | | None |
| Yarn cache | `~/Library/Caches/Yarn/*` | `package` | `caution` | | None |
| Chrome cache | `~/Library/Caches/Google/Chrome/*` | `browser` | `caution` | Chrome | None |
| User logs | `~/Library/Logs/*` | `logs` | `caution` | | 7 days |
| Saved window state | `~/Library/Saved Application State/*` | `system` | `caution` | | 30 days |

* Playwright browsers do not come back on their own. You run `npx playwright install` again.
* App caches such as Deno, and the pnpm store in `~/Library/pnpm`, are left out. The pnpm store sits outside the cleanup roots.

### Project Build Folders

* A folder is offered only when all of these are true:
  1. Its name is on the list below.
  2. The project file that makes it sits in the folder above it.
  3. The project is a Git repository, and `git check-ignore` says Git ignores the folder.
  4. Nothing inside it changed within the minimum age, 14 days by default.
* Folders in a project with no Git repository, or that Git does not ignore, are shown dimmed and cannot be selected.

| Folder | Needs Beside It | Made By | Cost Of Removing | Tier |
| --- | --- | --- | --- | --- |
| `target/` | `Cargo.toml` | Cargo | The next build starts from scratch. | `safe` |
| `node_modules/` | `package.json` and a lock file | npm, pnpm, Yarn, Bun | The next install downloads packages again. | `caution` |
| `.next/` | `next.config.*` | Next.js | The next build starts from scratch. | `safe` |
| `.venv/`, `venv/` | `pyproject.toml` or `requirements.txt`, and a lock file | Python | The environment must be made again. Packages installed by hand, without the lock file, are lost. | `caution` |
| `.gradle/`, `build/` | `build.gradle` or `build.gradle.kts` | Gradle | The next build starts from scratch. | `safe` |
| `__pycache__/` | Any `.py` file | Python | Python writes them again when it runs. | `safe` |

* `build/` and `dist/` are common names for other things, so only `build/` beside a Gradle file is on the list. `dist/` is not.
* Each row shows the project, the folder, its size, and when the project last changed. Last changed means the newest Git commit, or the newest change to a tracked file, whichever is later.
* The list and each tier must be reviewed, like a bundled rule, before this is built.
* Moving tens of thousands of small files to the Trash through Finder can be slow. The screen shows progress.

## Application Management

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| App removal review | Shows an app and its related files before removal. Leaves anything that may be your data unselected, warns about folders shared with other apps, and refuses while the app is open. | P1.5 | Planned |

* This is separate from cleaning app caches.
* The allowed cleanup folders do not include apps yet. See [Design Questions](#design-questions).

## Startup And Background Services

* Shows programs that start on their own, and lets you turn them off in a way you can undo.
* Items owned by macOS can be viewed, never changed.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Startup inventory | Lists login items, background items, launch agents, and launch daemons, including ones with no Dock or menu bar icon. | P2 | Proposed |
| Item details | Shows the program, its publisher, whether it is running, and its signature. Items under `/System/Library` are view only. | P2 | Proposed |
| Disable and restore | Shows what will happen, saves the current state, and turns the item off. Undo restores that saved state. Its launch file is never edited or deleted. | P2 | Proposed |
| Incomplete list warning | Says when macOS permissions or missing data mean the list may not be complete. | P2 | Proposed |

* Changes that affect the whole Mac may need one reviewed command run with `sudo`.
* How to read background items, and which macOS versions work, still need checking.
* See the [Startup screen](INTERFACE.md#the-startup-screen).

## SSH Management

* Shows your SSH setup and makes small, specific changes.
* Cleanup can never remove anything in `~/.ssh`.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Host list | Lists hosts from `~/.ssh/config`, with host name, user, port, and key. Editing the file belongs to Dotfiles. | P2 | Proposed |
| Key list | Shows each key's type, size, fingerprint, comment, whether it has a passphrase, and whether its public key file exists. Never shows a private key. | P2 | Proposed |
| Agent list | Lists keys loaded in the SSH agent, using `ssh-add -l`. | P2 | Proposed |
| Permission check and fix | Finds files with permissions that are too open, and offers exact fixes. Saves the old permissions for undo. | P2 | Proposed |
| Agent keys | Adds a key to the agent or removes it, using `ssh-add`. The key file stays on disk. | P2 | Proposed |
| Known hosts | Lists known hosts and removes the ones you select, using `ssh-keygen -R`, after a backup. | P2 | Proposed |

* Every change shows a preview and asks first.
* The views and permission modes are in the [SSH screen](INTERFACE.md#the-ssh-screen).

## Dotfile Management

* Works with a fixed list of settings files: shell, Git, SSH config, editor, and terminal.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Dotfile list | Lists the supported files that exist on your Mac, grouped by purpose. | P2 | Proposed |
| Edit with backup and check | Saves a dated backup, opens your editor, shows what changed, and runs a syntax check when one exists. Then you keep it, edit again, or restore. | P2 | Proposed |
| Reviewed export | Lets you pick files, flags possible tokens or passwords for you to review, leaves out known secret files, and writes a `.tar.gz` with a list of what was included and left out. | P2 | Proposed |

* Secret detection helps your review. It cannot promise a file has no secrets.
* Private keys and the listed credential files are always left out.
* Importing and syncing are not included.
* The supported files, checks, and exclusions are in the [Dotfiles screen](INTERFACE.md#the-dotfiles-screen).

## Shell PATH

* Shows your `PATH`, the list of folders your shell searches for programs, as an ordered list you can read at a glance.
* Finds problems in it, and shows which copy of a program runs when several exist.
* Changes go through the same backup, change list, and syntax check as dotfile editing.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| PATH view | Lists each `PATH` folder in search order, with how many programs it holds, and where it was added when neet can tell, such as `~/.zshrc` line 12, `/etc/paths.d/`, or a tool like Homebrew. | P2 | Proposed |
| PATH check | Flags folders listed twice, folders that do not exist, folders other users can change, and relative folders such as `.`. | P2 | Proposed |
| Which program wins | Type a program name, such as `python3`, to see every copy on your `PATH` in order, which one runs, and each one's version when it can be asked safely. Also lists every program that is hidden by an earlier copy. | P2 | Proposed |
| Reorder and edit | Move a folder up or down, add one, or remove one. neet shows the old and new `PATH` side by side, then writes the change to a shell file on the dotfile allow list, with a backup, a change list, and a syntax check. | P2 | Proposed |
| New shell preview | After a change, starts a fresh login shell and shows the `PATH` it really gets, so you can see whether the change worked. Your open terminals keep their old `PATH` until you open a new one. | P2 | Proposed |

* neet never changes `/etc/paths` or `/etc/paths.d`. They are shown, but need admin rights, and belong to macOS and installers.
* A folder added by a command, such as `eval "$(brew shellenv)"`, is shown with that command. neet does not edit inside it, but it can move the whole line.
* How to find where each folder was added, and how to write edits without breaking a hand written file, are not decided. See [Design Questions](#design-questions).
* The screen is in [INTERFACE.md](INTERFACE.md#the-path-screen).

## AI Coding Tool Files

* Shows the settings, instruction files, and skills that Claude Code and Codex keep on your Mac.
* Covers `~/.claude` and `~/.codex`, plus skills saved inside your project folders.
* View only. neet never edits, moves, or deletes these files.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Tool overview | Shows which tools are set up, where their folders are, and how much space each folder uses. | P2 | Proposed |
| Settings view | Shows settings files, such as `~/.claude/settings.json` and `~/.codex/config.toml`. | P2 | Proposed |
| Instruction files | Shows global instruction files, such as `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`, when they exist. | P2 | Proposed |
| Skill list | Lists every skill with a `SKILL.md`, from each tool's `skills` folder and its installed plugins. Shows the name, description, tool, source (yours, built in, or plugin), and path. | P2 | Proposed |
| Project skills | Lists skills saved inside project folders, such as `<project>/.claude/skills`, next to your global skills. Shows which project each one belongs to. Uses the home folder scan to find them, so it does not walk the disk again. | P2 | Proposed |
| Skill viewer | Opens a skill's `SKILL.md` and lists the other files in its folder. | P2 | Proposed |

* The name and description come from the front matter at the top of each `SKILL.md`.
* Never shows sign in files, such as `~/.codex/auth.json`.
* These folders are not cleanup roots, so cleanup cannot touch them. They are not added to the protected paths either.
* Never opens chat history, session logs, or databases, such as `history.jsonl`, `sessions`, and `.sqlite` files. They can hold private conversations.

## Performance, Power, And Displays

* Shows or changes settings that affect speed, battery life, and sleep. Nothing else.
* Controls your Mac does not support are hidden.
* Every change saves the old value, so you can undo it.

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Power modes | Offers Battery Life, Balanced, and, where supported, Maximum Performance, set apart for battery and charger. The `pmset` settings need checking on real Macs. | P2 | Proposed |
| Graphics switching | Turns Automatic Graphics Switching on or off, on Intel Macs with two graphics chips. | P2 | Proposed |
| Refresh rate | Shows each display and its rates, including Adaptive Sync where supported. A new rate switches back after 15 seconds unless you keep it. | P2 | Proposed |
| Game Mode guidance | Explains whether Game Mode is available and how a full screen game turns it on. Changes nothing. | P2 | Proposed |
| What keeps the Mac awake | Shows apps keeping the Mac awake and the reason each one gives, with links to the app or setting. Never quits an app. | P2 | Proposed |
| Overnight wake history | Shows when the Mac woke up and why, from the power log. Changes nothing. | P2 | Proposed |
| Wake settings | Reviews changes to wake for network access, Power Nap, and keeping network connections alive during sleep. Which Macs and macOS versions support each one needs checking. | P2 | Proposed |

* A change that runs a command shows the command first, and uses `sudo` only when needed.
* Display changes use CoreGraphics.
* The draft flows and setting names are in the [Settings screen](INTERFACE.md#the-settings-screen).

## Interface, Preferences, And Distribution

| Feature | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Home menu | Opens on a Home screen with ASCII art and a menu of features, moved with the arrow keys. `Enter` opens a screen and `Esc` goes back. Works with Vim keys outside text fields. Features not built yet show dimmed. The status panel shows the disk gauge and the scan. | P1 | In Progress |
| Help | Press `?` for help on the current screen and the selected rule. Screen help works. Rule help waits on Clean. | P1 | In Progress |
| Command line | Run `neet` to open it. The only options are `--help` and `--version`. Cleanup happens only inside the interface. | P1 | Built |
| Install with Homebrew or Cargo | Packages for both. Neither is published yet. | P1 | Planned |
| macOS support | macOS 13 or later, on Apple silicon and Intel. CI checks that the code builds for both, but release builds still need testing on real Macs. | P1 | Planned |
| One program | The disk and cleanup features ship as one program. Later features may call macOS tools or your editor. | P1 | Planned |
| Saved preferences | Remembers your neet choices between runs. Which choices, and how they are stored, are not designed yet. | P2 | Proposed |

## Safety And Recovery

* These rules apply to every feature above.
* The path check is built in the library. The others are not built yet, because no feature that changes anything exists yet.

| Safeguard | What It Does | Phase | Status |
| --- | --- | --- | --- |
| Allowed folders and protected paths | Limits cleanup in Rust code. Refuses personal folders, credentials, Git history, system files, and very broad folders. Rules cannot override it. Built in the library as `CleanupRoots`, not yet used by a cleanup. | P1 | Built |
| Identity check | Checks each reviewed path again, and that it is still the same file, right before moving it. Anything changed or unclear is skipped. | P1 | Planned |
| No admin cleanup | Refuses to run as root, and never uses `sudo` for cleanup. | P1 | Planned |
| Backups and undo | Saves the old content, permissions, or setting before any SSH, dotfile, startup, or settings change, so it can be undone. | P2 | Proposed |
| Scoped admin commands | When a settings or startup change needs admin rights, runs one allowed command, then returns to the interface. | P2 | Proposed |

* The full path rules, allow lists, backup rules, and required tests are in [SAFETY.md](SAFETY.md).

## Design Questions

* These gaps must be settled before the feature is built.
* None of them allow wider cleanup or weaken a safeguard.

| Area | What Needs Deciding |
| --- | --- |
| App removal | P1.5 removes apps, but the allowed folders do not include apps. Decide what may be removed, and update the safety rules before building it. |
| Docker and simulators | Their tools may not be undoable the way the Trash is. Decide the targets, the preview, and a way to undo. Postpone anything that cannot meet the Trash policy. |
| Mail downloads | Mail keeps downloaded attachments in `~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads`, outside the Containers cleanup root. Decide whether to add that one folder as a root, after checking it never holds your only copy of an attachment. |
| System logs | No targets are chosen, and cleanup cannot use `sudo` or touch protected paths. Find targets that fit both limits. |
| Startup control | Check how each item type is found, turned off, and turned back on, on every supported macOS version, including when the list is incomplete. |
| Power and display | Check which settings exist, what they really do, and that refresh rate rollback works, on real Macs. Draft command names are not proof. |
| Treemap and preferences | Design the interaction, the saved choices, and how they are stored. Then add their M7 checks. |
| AI coding tool files | Confirm where each tool saves project skills. Folder layouts change between tool versions, so check them on each release. |
| Space breakdown | Check which numbers macOS reports reliably: the used space of each APFS volume, local snapshot sizes, and purgeable space. Decide which disk totals the Home gauge and the breakdown share, so they always add up. |
| Project build folders | Project folders are outside the cleanup roots, and many live in `~/Documents` or `~/Desktop`, which are protected. Decide whether a separate, narrow check can allow only the listed build folders inside a Git repository, and only when Git ignores them, without weakening the protected paths for anything else. Update [SAFETY.md](SAFETY.md) first. |
| Shell PATH | Decide how to find where each `PATH` folder was added. Tracing a login shell as it starts runs your shell files, so decide whether that is acceptable. Decide whether edits go in a block neet owns, marked with comments, or change your own lines in place. |

## Excluded Features

* neet will never do these.
* Deleting files for good, and emptying the Trash, are excluded from P1. No later phase allows them yet.

| Feature | Why |
| --- | --- |
| Turning off System Integrity Protection | It protects macOS itself. |
| Removing Apple's own apps | macOS needs them. |
| Changing `nvram` | A mistake can stop the Mac from starting. |
| Deleting duplicate files automatically | Only you know which copy matters. |
| Freeing memory | macOS already manages memory. |
| Cleaning without review | Every cleanup shows its paths and asks first. |
| Deleting files for good, or emptying the Trash, in P1 | Everything goes to the Trash, so it can be put back. |
| Showing private SSH keys | Only key details are shown. |
| Creating or deleting SSH keys | Use `ssh-keygen` for that. |
| Mac settings unrelated to speed, battery life, or sleep | The Settings screen covers only those. |
| Editing or deleting AI coding tool files | The AI tools section is view only. Use the tool itself to change them. |
| Importing or syncing dotfiles | Unpack the export into a Git repository, or use a dotfile manager such as chezmoi. |

## Key Terms

| Term | Meaning |
| --- | --- |
| **Size on disk** | The space a file really uses on disk. It can differ from the file's length. |
| **Hard link** | One file with more than one name. neet counts it once. |
| **APFS** | The file system macOS uses. On APFS, copied files can share space. |
| **Dry run** | A cleanup plan that lists what it would do and changes nothing. |
| **Rule** | A short TOML entry that says which files neet may clean, and how risky that is. The Rust safety checks always apply on top. |
| **Risk tier** | How risky a rule is: `safe`, `caution`, or `expert`. |
| **Dotfile** | A settings file in your home folder, such as `~/.zshrc`. |
| **SSH agent** | A background program that holds unlocked SSH keys for you. |
| **Background item** | A program macOS starts without you opening it, such as a login item, launch agent, or launch daemon. |
| **Skill** | A folder of instructions an AI coding tool can load, described by a `SKILL.md` file. |
| **Sleep assertion** | A request from an app to keep the Mac awake. |
| **`sudo`** | Runs one command with admin rights, after asking for your password. |
| **`PATH`** | The ordered list of folders your shell searches when you type a program name. The first match runs. |
| **Local snapshot** | A copy of your disk that Time Machine keeps on the Mac itself. It holds space until macOS removes it. |
| **Purgeable space** | Space macOS counts as used, but can free on its own when it needs room, such as iCloud files it can download again. |
| **Build output** | Files a build tool makes from your source code, such as `target/` or `node_modules/`. The tool can make them again. |
