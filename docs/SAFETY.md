# Safety

* Which files neet may read or change.
* Required checks, backups, and ways to undo.

## Contents

* [The Promises](#the-promises)
* [How A Cleanup Runs](#how-a-cleanup-runs)
* [Allowed Folders](#allowed-folders)
* [Protected Folders](#protected-folders)
* [The Path Check](#the-path-check)
* [The Check Before Moving](#the-check-before-moving)
* [Cleanup Rules](#cleanup-rules)
* [App Removal](#app-removal)
* [Build Folders And Installers](#build-folders-and-installers)
* [Tools neet Runs](#tools-neet-runs)
* [Emptying The Trash](#emptying-the-trash)
* [Dotfiles](#dotfiles)
* [Startup Items](#startup-items)
* [Space Breakdown](#space-breakdown)
* [Settings](#settings)
* [Left Out](#left-out)
* [Planned Features](#planned-features)
* [AI Tools](#ai-tools)
* [Tests](#tests)

## The Promises

* Cleanup uses Trash. Docker and simulator tools, and emptying Trash, delete permanently after a separate warning and confirmation.
* Review every path and confirm before files move.
* Cleanup uses fixed folder lists. App removal, build folders, and installers each have their own checks.
* Cleanup protects personal files, cloud files, passwords, and keys. Selected build folders, installers, and chat transcripts have separate rules. Dotfile and skill edits need review and backup.
* Anything that changed since you reviewed it is skipped.
* Cleanup never uses admin rights. Power settings are the only change that does, and only through `sudo` after review.

## How A Cleanup Runs

* Find every item the selected rules cover. Nothing changes yet.
* Show every path.
* Show the number of items, their total size, and that they go to the Trash.
* Ask you to confirm.
* Check each item again, right before it moves.
* Ask Finder to move it to the Trash, so Put Back can restore it.

* There is no command or option that skips the review or the question.
* The first cleanup makes macOS ask whether your terminal may control Finder.
* Finder has one minute to answer each move. A timeout is reported. Check Trash before retrying because Finder may still finish later.
* Results show moved and skipped items. Space is freed only when Trash is emptied.

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

neet refuses these, and everything inside them:

| Path | Why |
| --- | --- |
| `/System`, `/usr`, `/Library` | macOS and shared system files. |
| Your home folder itself | Far too broad. |
| `~/Documents`, `~/Desktop`, `~/Downloads` | Your own files. Only [build folders and installers](#build-folders-and-installers) may be taken from them. |
| `~/Library/Mobile Documents`, `~/Library/CloudStorage` | iCloud and other cloud files. |
| `~/Library/Keychains`, `~/.ssh` | Passwords and keys. |
| `~/Library/Mail`, `~/Library/Messages` | Mail and Messages, with their attachments. Some may be the only copy. |
| Any `.git` folder | Project history. |
| `~/Library/Developer/Xcode/Archives` | Builds of apps you shipped. They cannot be made again. |
| The top folder of any disk | Far too broad. |

## The Path Check

* Before review, each item must pass these checks:
* Refuses empty and relative paths, and `..` parts.
* Resolve parent links to find the real location. Check the item itself as it is.
* Refuses anything protected.
* Requires the item to be inside an allowed folder, and not the folder itself.
* Refuses a link, unless it leads somewhere inside the same allowed folder that is not protected.
* Record the real path and file identity.
* Only the path check and the separate app, build folder, and installer checks can approve a cleanup item.
* Links are never moved. Finder might move what a link points to instead of the link.

## The Check Before Moving

* Right before each item moves, neet checks that:
* The item still passes the same check that approved it.
* Its real path and identity on disk are unchanged.
* The apps its rule needs closed are still closed.
* Nothing inside it changed more recently than its rule allows.
* If any check fails, or neet cannot be sure, the item is skipped and listed with the reason.
* If neet cannot tell whether an app is open, it treats it as open.
* **Known gap:** a file can still change between the last check and Finder moving its path.

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
* Run the tests.
* Read every path it finds on a real Mac.
* Write down the macOS and app versions you checked.

## App Removal

App removal has its own path check, then uses the normal review, confirmation, and Trash flow.

### Which Apps

* Only an app directly inside `/Applications` or `~/Applications`.
* Refused:
* Apple's apps, whose bundle ID starts with `com.apple.`.
* Links, such as `/Applications/Safari.app`.
* Apps in subfolders, such as `/Applications/Utilities`.
* Apps without a bundle ID neet can read.
* Apps that are open, checked when you open one and again before each move.
* An app owned by another user, such as one from the App Store, makes Finder ask for your password. neet never uses `sudo`.

### Which Files

* Match exact names directly inside these Library folders, ignoring letter case. Partial names do not count.
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

* Quick Clean uses separate checks for build folders, installers, and Docker reset. Review and confirmation are still required.
* Every item starts selected. You can clear any of them before the review.

| Item | Taken only when |
| --- | --- |
| `node_modules` | It is a folder, inside your home folder. |
| `target` | It is a folder with a `Cargo.toml` beside it. |
| Installers | The name ends in `.dmg`, `.pkg`, `.iso`, or `.xip`, in any case, anywhere inside `~/Downloads`. |
| Docker's disk image | It is a file at exactly `~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`. Only [resetting Docker](#resetting-docker) takes it, after Docker Desktop has quit. |

* Refused:
* Links, whatever they point to.
* Anything else in `~/Library`, or in a hidden folder directly in your home folder, such as `~/.vscode`. Tools keep their own copies there.
* A build folder inside another one, such as `node_modules` inside `node_modules`. The outer one is taken whole.
* Every other protected folder still applies, such as `.git`, iCloud, keychains, and `~/.ssh`.
* Build folders come back when you install or build again. An installer only comes back if you download it again.

## Tools neet Runs

* Quick Clean asks simulator and Docker tools to remove their own data. These commands delete permanently.
* Runtimes and images can be downloaded again. Simulator and volume data may have no other copy. Emptying Trash is also permanent.

| Tool | Command | Removes |
| --- | --- | --- |
| Simulator runtimes | `xcrun simctl runtime delete <id>`, once per runtime you select | The runtime. |
| Simulators | `xcrun simctl delete <udid>`, once per simulator | The simulators on each runtime that was removed, and simulators left without a runtime if you select them. Their apps and data go too. |
| Docker | `docker system prune --all --force` | Stopped containers, networks no container uses, every image no container uses, and the build cache. Volumes are kept. |
| Docker volumes | `docker volume rm -- <name>`, once per volume you select | A volume no container uses, with its data. Docker refuses if a container uses it. |

* Before any of these runs, neet:
* Show runtime details and devices, or Docker sizes and unused volumes.
* Starts with nothing selected for runtimes, simulators, and volumes.
* Warn that removal is permanent. Only `y` confirms.
* Only IDs made of letters, digits, and hyphens are passed to `simctl`, so they can never be `all` or an option. Only volume names made of letters, digits, `_`, `.`, and `-`, starting with a letter or digit, are passed to `docker`, after `--`.
* Docker sizes are estimates. They are not used to promise free space after prune.
* `docker container ls` checks running containers, images, and Compose labels before and after prune, with a ten second limit. Failed checks stay visible.
* None of these commands uses admin rights. neet never starts Docker on its own; `o` opens Docker Desktop only when you press it.

### Leftover Simulator Downloads

* macOS keeps a runtime's download in `/System/Library/AssetsV2/com_apple_MobileAsset_*SimulatorRuntime` after `simctl` removes the runtime. It marks them never collected, and System Integrity Protection blocks deleting them, even with `sudo`.
* Checked on macOS 26.5 with Xcode 26.6: adding the runtime again from the download with `xcrun simctl runtime add` and deleting it with `simctl` left the download in place.
* neet reads each download's `Info.plist` with `plutil` and its size from file details. A download whose build matches an installed runtime is in use and is not listed.
* neet only shows them. It never deletes, moves, or changes system files, and never offers to.

### Resetting Docker

* Reset moves Docker's whole disk image, including containers and volumes, to Trash. It can be restored by hand.
* A yellow question names the size and what will move. Only `y` confirms:
* Stop Docker Desktop with `docker desktop stop`. Wait up to three minutes for `lsof` to confirm no process has the image open. Refuse if the check fails.
* Recheck the exact Docker image path listed above.
* Move it directly to `~/.Trash`, adding a number if its name exists. Finder cannot do this move, so Put Back is unavailable.
* To restore, move `Docker.raw` from Trash to `~/Library/Containers/com.docker.docker/Data/vms/0/data/` before reopening Docker. Otherwise Docker makes an empty image.

## Emptying The Trash

* Trash lists scanned items in `~/.Trash`, largest first, excluding `.DS_Store`.
* `e` asks before permanent deletion and warns that Put Back stops working. Only `y` confirms.
* Finder empties Trash through `osascript`, including Trash on other disks. The command has a ten minute limit.
* `o` opens the Trash in Finder, where Put Back works, and changes nothing.
* If Trash could not be read, neet refuses to empty it and explains Full Disk Access.
* The first time, macOS asks whether neet may control Finder, as it does for moving to the Trash.

## Dotfiles

Dotfile edits need a diff review and backup. They do not use Trash.

### Which Files

Only these files are listed. Paths are inside your home folder.

| Group | Files |
| --- | --- |
| Shell | `.zshrc`, `.zprofile`, `.zshenv`, `.zlogin`, `.zlogout`, `.bashrc`, `.bash_profile`, `.profile`, `.inputrc`, `.config/fish/config.fish` |
| Git | `.gitconfig`, `.config/git/config`, `.config/git/ignore`, `.gitignore_global` |
| Editors | `.vimrc`, `.config/nvim/init.lua`, `.config/nvim/init.vim`, `.nanorc`, `.editorconfig` |
| Terminal | `.tmux.conf`, `.config/tmux/tmux.conf`, `.config/kitty/kitty.conf`, `.config/ghostty/config`, `.config/ghostty/config.ghostty`, `.config/alacritty/alacritty.toml`, `.config/starship.toml`, `.wezterm.lua` |
| Tools | `.npmrc`, `.config/mise/config.toml`, `.config/gh/config.yml`, `.Brewfile` |

* Adding a file needs a change to this table and to the code, together.
* Never listed, read, or exported:
* Shell and editor history, such as `.zsh_history`, `.bash_history`, and `.viminfo`.
* Files that hold passwords or tokens: `.netrc`, `.git-credentials`, `.config/gh/hosts.yml`, and everything in `~/.aws`, `~/.docker`, and `~/.kube`.
* Everything in `~/.ssh`.
* Files a shell makes for itself, such as `.zcompdump`.
* `.npmrc` can hold a token. It is marked **secret**, and starts left out of exports.

### What May Change

* Only the files in the table, and, with [chezmoi](#chezmoi), the source file chezmoi keeps for each one.
* View only, never changed:
* A file that is a link leading outside your home folder, or into a [protected folder](#protected-folders).
* A file neet cannot write as you. neet never uses `sudo` here.
* With chezmoi: templates, encrypted files, and files chezmoi builds from scripts or changes in place.
* Allowed links are edited at their destination. Parent links get the same check. chezmoi source paths containing links are refused.

### How A Change Runs

* Edit a copy in `$VISUAL`, `$EDITOR`, or `nano`, or use Configure.
* neet checks the new text, when it knows how. See [Checks](#checks).
* It shows the diff, and asks. Only `y` goes ahead.
* Recheck identity, change time, permissions, links, and contents. Edits, Save, Put Back, Add, Restore, and Export refuse files changed after review.
* It saves a [backup](#backups).
* Write beside the file, keep permissions, flush to disk, then replace it. A crash must not leave half a file.
* With chezmoi, it writes the source file, then runs `chezmoi apply` for that one file, when it [may run chezmoi](#when-neet-may-run-chezmoi).

### Checks

A check only reads the file. It never runs it.

| File | Check |
| --- | --- |
| zsh files | `zsh -n` |
| bash files and `.profile` | `bash -n` |
| fish | `fish --no-execute` |
| Git files | `git config --file <copy> --list` |
| TOML files | Read with neet's own TOML reader. |
| Everything else | None. The screen says there is no check. |

* Save, Put Back, and Restore also check the text they are about to save or restore.
* Failed checks block saving. Return to the editor or cancel.

### Backups

* Kept in `~/.local/state/neet/backups/dotfiles/`, by file, then by the time of the change.
* The folder can only be read by you. Each backup keeps the file's permissions.
* Edits, Configure, restore, apply, and re-add save backups first. With chezmoi, save both home and source copies.
* Adding a file to chezmoi saves none: it only makes a new source file, and never writes over one.
* neet never removes a backup. Remove old ones yourself.
* Restore shows a diff and asks first. It backs up the current file too.

### Configure

* Configure changes only known settings. Use the editor for others.
* Keep unrelated lines, comments, and order.

| Program | Settings | Written with |
| --- | --- | --- |
| Git | `user.name`, `user.email`, `init.defaultBranch`, `core.editor`, `pull.rebase`, `push.autoSetupRemote` | `git config --file <copy> -- <key> <value>`, or `--unset-all` to remove one |
| tmux | `mouse`, `history-limit`, `base-index`, `renumber-windows`, `mode-keys`, `escape-time`, `status-position`, `default-terminal` | neet, line by line |
| Starship | `add_newline`, `line_break.disabled`, `character.success_symbol`, `character.error_symbol`, `directory.truncation_length`, `cmd_duration.min_time`, `command_timeout`, `scan_timeout` | neet, with a TOML editor that keeps comments and layout |
| kitty | `font_family`, `font_size`, `scrollback_lines`, `window_padding_width`, `background_opacity`, `cursor_blink_interval`, `macos_option_as_alt`, `enable_audio_bell` | neet, line by line |
| Ghostty | `theme`, `font-size`, `background-opacity`, `window-padding-x`, `window-padding-y`, `cursor-style`, `scrollback-limit`, `macos-option-as-alt`, `mouse-hide-while-typing`, `copy-on-select` | neet, line by line |
| zsh, in `.zshrc` | `EDITOR`, `VISUAL`, `HISTSIZE`, `SAVEHIST`, and the options `auto_cd`, `share_history`, `hist_ignore_all_dups`, `correct` | neet, only inside its own block |
| mise | `tools.node`, `tools.python`, `tools.go`, `tools.ruby`, `settings.auto_install`, `settings.jobs`, `settings.experimental` | neet, with the same TOML editor as Starship |

* Changes are made on the edit copy, then go through the same check, diff, backup, and chezmoi steps as an edit.
* Refuse invalid values and multiple lines. Each setting keeps its own type and allowed values.
* **Git:** values are read with `git config --file <copy> --list`, which reads only that file, not the files it includes.
* **tmux:** read only plain `set`, `set-option`, `setw`, or `set-window-option` lines with `-g` or `-s`. Leave appends, targets, joined commands, continued lines, and `%if` blocks alone. Never run tmux.
* Change the last plain value, keeping spacing and comments. Add a line if none exists.
* **Remove:** every plain line for the option is taken out.
* Quote values when needed. Refuse a value containing `'` and use the editor instead.
* There is no check for tmux files: tmux has no way to read a file without starting a server.
* **kitty:** read only this file, without running kitty or reading includes. Change the last value, keeping spacing; remove all copies of a setting when unset. No syntax check is available.
* **Ghostty:** read `key = value` lines without running Ghostty or reading includes. Keep spacing and quotes, change the last value, or remove all copies when unset. Refuse values that cannot be quoted safely. No syntax check is available.
* **Starship and mise:** edit TOML values while keeping comments and layout. Add missing tables, but refuse settings that hold lists or tables. Remove a key only if other settings keep their meaning. Check the result as TOML.
* More programs are added one at a time. Each is listed here before it ships.
* **zsh:** only edit between `# >>> neet >>>` and `# <<< neet <<<`. Do not read settings outside the block or run the file.
* Add a missing block at the end, after a blank line.
* Use one line per setting. Replace its last line and remove older copies. Keep other lines.
* Quote export values. Refuse a value containing `'` and use the editor instead.
* Remove all lines for an unset setting. Remove an empty block and its leading blank line.
* Refuse missing, reversed, or repeated markers.
* The file then goes through the same `zsh -n` check as an edit.

### chezmoi

* neet works with [chezmoi](https://www.chezmoi.io), a dotfile manager, when it is installed.
* Templates and hooks can run programs. Read chezmoi files directly; run it only for reviewed changes that pass the checks below.

#### What neet reads itself

* Read `sourceDir` from chezmoi TOML or JSON config, or use `~/.local/share/chezmoi`. Apply `.chezmoiroot` if present. Other config formats are unknown.
* Find source names using `dot_`, `private_`, `readonly_`, `executable_`, and `empty_` prefixes.
* Compare home and source bytes to mark saved, changed, or not saved.
* `.tmpl`, `encrypted_`, `modify_`, `create_`, and `symlink_` source files stay view only. Never render them.

#### When neet may run chezmoi

* Only when all of these are true. Otherwise neet still writes the source file, and tells you to run `chezmoi apply` for that file yourself.
* The config file is one neet can read, and has no `hooks` section. A hook runs on every chezmoi command, even `chezmoi source-path`.
* Every `.chezmoiignore`, `.chezmoiremove`, and `.chezmoiexternal` file in the source folder, and everything in a `.chezmoiexternals` folder, is plain, or only uses `if`, `else`, `end`, `eq`, `ne`, `and`, `or`, `not`, and values that start with `.`. chezmoi renders these on almost every command, even ones that only name a single file, with or without `.tmpl` in their name.
* The file is not view only.
* The file is not a link. chezmoi would replace the link with a plain file. For a link, neet writes the source file itself and says which command to run.

| Command | When | Changes |
| --- | --- | --- |
| `chezmoi apply --no-tty --force --exclude=scripts,externals -- <file>` | After a change to a source file, or to put the source version back. | That one file in your home folder. |
| `chezmoi re-add --no-tty -- <file>` | To keep the version in your home folder. | That file's source file. |
| `chezmoi add --no-tty -- <file>` | To let chezmoi manage a listed file. | Adds one source file. |

* When chezmoi cannot run, Add writes a new source file using its naming and permissions. Write a complete temporary copy, then link it into place without replacing anything.
* `a` never adds a link: another tool, such as GNU Stow, may manage it.

* `--force` is used only after review, file checks, and backup, for the one named file.
* Commands name the home path, such as `~/.zshrc`, not a link destination.
* Before each command, recheck source paths, hooks, templates, and whether the file may still be changed.
* After each command, neet checks the file now matches, and says so if it does not.
* Never run `chezmoi status`, `diff`, `cat`, `managed`, `update`, or `apply` without a file. They can render templates or run scripts. Show neet's own diff.
* Templates are never changed or rendered by neet.
* Checked with chezmoi 2.72.2, in a test folder where every kind of template and hook wrote down when it ran.

### Export

* Export puts your dotfiles in a Git repository, ready for GitHub.
* **Review for secrets first.** neet looks through every file that will be exported for:
* Private key blocks, such as `-----BEGIN OPENSSH PRIVATE KEY-----`.
* Known token shapes, such as `ghp_`, `github_pat_`, `sk-`, `AKIA`, and `xox`.
* Settings named like secrets, such as `password`, `token`, `secret`, and `_authToken`, with a value.
* Findings show file, line, and only the first four characters of the value. Leave out or edit the file before exporting.
* Secret checks cannot prove a file contains no secrets.
* **With chezmoi:** offer only changed source copies of listed dotfiles. Recheck selected files before staging and commit only those files, even if others are staged. Show the message first. Unreadable files block review. Repository Git hooks still run.
* **Without chezmoi:** require a missing or empty source folder that is not a link, plus a Git name and email. Recheck all reviewed files and destinations, then create only selected copies, a README, and `.chezmoiignore` before `git init` and commit. Never overwrite existing files or follow protected links.
* Push only after `y` confirms the reviewed commit, remote address, and tracked branch. Refuse changed commits or destinations. Other Git push settings must not add branches or tags. With no remote, a separate confirmation may create a private `dotfiles` repository through `gh`.
* Never force push, pull, merge, or rebase. A rejected push says to pull with Git first.
* Git runs with fsmonitor and terminal password prompts turned off. Repository hooks still run.

## Startup Items

Startup only reads unless you confirm a change to your own launch agent.

### What Is Read

| Kind | Where | Read with |
| --- | --- | --- |
| Your launch agents | `~/Library/LaunchAgents/*.plist` | neet reads the plist itself. |
| Launch agents for every user | `/Library/LaunchAgents/*.plist` | neet reads the plist itself. |
| Launch daemons | `/Library/LaunchDaemons/*.plist` | neet reads the plist itself. |
| Allowed in the background | Helpers inside apps in `/Applications` and `~/Applications`: `Contents/Library/LoginItems`, `LaunchAgents`, and `LaunchDaemons` | neet reads them itself, and lists only those launchd knows, which the app asked macOS to run. |
| Whether each is turned off, and runs | launchd | `launchctl list`, `launchctl print-disabled gui/<uid>` and `system`, and `launchctl print system/<label>` |
| Who made it, and whether it is signed | The program file | `codesign --display`, which only reads it |

* Never read: `/System/Library`, and macOS's own items, which are only counted. Never the `.btm` database, which only root can read.
* Never run: `sfltool dumpbtm`, which asks for an admin password, and `osascript`, which asks for Automation permission. Nothing neet runs here may show a prompt.
* Apps that open at login are not listed. `o` opens System Settings, General, Login Items & Extensions.
* Listing never loads or starts an item and never uses `sudo`. Turning an item on after confirmation may ask launchd to load and start it, as described below.
* When `launchctl` cannot be read, the list says it is incomplete, and each item shows that whether it runs is not known.
* A plist that cannot be read, or is empty, is still listed, and says so.

### What May Change

Only your own launch agents in `~/Library/LaunchAgents` may be turned off and back on, with `t`, after a question that names the label and the program it runs.

| Change | Commands, in order |
| --- | --- |
| Turn off | `launchctl disable gui/<uid>/<label>`, then `launchctl bootout gui/<uid>/<label>`, which stops it now if it runs. |
| Turn back on | `launchctl enable gui/<uid>/<label>`, then `launchctl bootstrap gui/<uid> <plist>`. |

* `disable` and `enable` are what keep the change after a restart. macOS keeps them in its own list, outside the plist.
* Turning off something that was not loaded is not an error, and neither is turning on something that is already loaded.
* Save the old plist path, disabled state, and loaded state in `~/.local/state/neet/startup/<label>.json` before any command. Keep it after exit for recovery.
* Records start with an unknown result, then record no change, partial change, or completion. Older records stay unknown.
* If only enable or disable succeeds, report the login setting change and failed command. A failed state check leaves the running state unknown.
* The plist is never changed, moved, or removed. Nothing here uses `sudo`, and no command may show a prompt.
* neet refuses, and runs nothing, when:
* The item is not in `~/Library/LaunchAgents`, has no label, its label starts with `com.apple.`, or its label has a `/` in it.
* The plist is a link that leads out of your home folder, or is not a plain file.
* The plist could not be checked when listed, changed since the list or the question, or now names a different label. Look again before trying a changed item.
* View only, always:
* Items in `/Library/LaunchAgents` and `/Library/LaunchDaemons`. An admin installed them for every user, sometimes for device management.
* Background items inside apps, and apps that open at login. `o` opens System Settings, where macOS turns them off and on.
* Items signed by Apple, and everything in `/System/Library`.

## Space Breakdown

* View only. Nothing is changed, and nothing asks for admin rights.
* `diskutil info -plist /` and `diskutil apfs list -plist` give the startup disk's volumes and their sizes.
* `tmutil listlocalsnapshots /` counts local snapshots. Their size needs admin rights, so it stays in Not measured.
* Folder sizes come from file details on the Data volume at `/System/Volumes/Data`, leaving out the home folder, other volumes, and other disks. No file is opened.
* Folders macOS will not let neet read are counted, not retried with more rights.

## Settings

### Power

* `pmset -g custom`, `-g batt`, `-g cap`, `-g`, and `-g assertions` are read without admin rights.
* Only these keys may change, to the values listed in Settings: `lowpowermode`, `displaysleep`, `sleep`, `disksleep`, `powernap`, `womp`, and `disablesleep` back to `0`.
* Each change shows its command, such as `sudo /usr/bin/pmset -a displaysleep 15`, before `y`. neet hands the terminal to `sudo`, which asks for your password, then comes back. neet itself never runs as root and never sees the password.
* Before the first change to a key, its value for battery and charger is saved in `~/.local/state/neet/settings/power.json`. `u` runs `sudo pmset -b` and `-c` with those values, then forgets them. Later changes keep the first saved values.

### Mac Settings

* Only the 31 settings listed in [FEATURES.md](FEATURES.md#settings) may change, each to one of its listed values, or back to macOS's default by deleting its key.
* `defaults read` checks the value again just before writing. If it changed since you looked, nothing is written.
* Before the first change to a setting, its value, or that it was not set, is saved in `~/.local/state/neet/settings/mac.json`. `u` writes that value back, or deletes the key.
* After a change, `killall` restarts Finder, the Dock, or SystemUIServer when the setting needs it. They start again on their own. Keyboard settings apply to apps when they next open, and some need you to log out.
* Choosing `~/Pictures/Screenshots` makes that folder if it is missing.

### Preferences

* Saved to `~/.config/neet/settings.toml`, written as a new copy and swapped in. A link there is refused.
* A missing file uses the defaults. Unknown values and broken files fall back to the defaults with a warning in Preferences. Keys from newer versions are ignored.
* Preferences only choose what screens open with. Every review, confirmation, and check still applies.

## Left Out

Checked and decided against, on macOS 26.5. Each stays protected.

| Idea | Why it is left out |
| --- | --- |
| Mail attachment cleanup | Attachments live inside Mail's mailbox folders. For POP and On My Mac mailboxes they can be the only copy, and telling accounts apart means reading Mail's account data. Mail's own Settings, General, Remove unedited downloads clears the copies it opens. |
| System log cleanup | `/private/var/log` and `/Library/Logs` belong to root, and `log erase` needs root. macOS rotates those logs and caps the unified log store in `/private/var/db/diagnostics` on its own. Your own logs in `~/Library/Logs` are a Deep Clean rule. Space shows the rest under macOS data. |

## Planned Features

Later features change files or settings in place, not through the Trash. Each has a fixed list of what it may change.

| Feature | May Change | How |
| --- | --- | --- |
| Shell PATH | Only the shell files in [Dotfiles](#which-files-1). Never `/etc/paths`. | Edit with a backup |
| Refresh rate | The mode of a connected display. It switches back after 15 seconds unless you keep it. | CoreGraphics |

* Every change:
* Shows what it will do first.
* Saves the old value, in `~/.local/state/neet/`, so it can be undone.
* Writes files by writing a new copy and swapping it in, so a crash never leaves half a file.
* Refuses links that lead out of your home folder.
* A command that needs admin rights is shown first, then run once with `sudo`.
* neet itself never runs as root.
* neet never reads the inside of a private key.

## AI Tools

* AI discovery reads names and file details. Chats reads at most 512 KiB per transcript and 32 MiB per refresh; Gemini project markers use at most 4 KiB each. Other contents open only on request. Never read sign in files or databases, run skills, or send contents to a service.
* Known user files:

| Tool | Settings and instructions | Skills | Chats |
| --- | --- | --- | --- |
| Claude Code | `~/.claude/settings.json`, `~/.claude/CLAUDE.md` | `~/.claude/skills/*/SKILL.md` | `~/.claude/projects/*/*.jsonl` |
| Codex | `~/.codex/config.toml`, `AGENTS.md`, `AGENTS.override.md` under `~/.codex` | `~/.agents/skills/*/SKILL.md`, `~/.codex/skills/*/SKILL.md`, including `.system/*` | `~/.codex/sessions` through year/month/day and `archived_sessions`, only `rollout-*.jsonl` |
| Gemini CLI | `~/.gemini/settings.json`, `~/.gemini/GEMINI.md` | `~/.gemini/skills/*/SKILL.md` and shared `.agents/skills` | `~/.gemini/tmp/*/chats/*.json` and `*.jsonl` |

* `CODEX_HOME` and `CLAUDE_CONFIG_DIR` replace their default roots when set. Resolved paths must stay inside home. Individual file links are skipped. Skill directory links inside home are supported.
* Check the launch folder and parents to the first Git root, stopping before home. Do not search every project.
* Project files: `CLAUDE.md`, `CLAUDE.local.md`, `.claude/CLAUDE.md`, `AGENTS.md`, `AGENTS.override.md`, `GEMINI.md`, tool settings including `.claude/settings.local.json`, and skills under `.claude`, `.codex`, `.gemini`, or `.agents`.
* Stop discovery at 20,000 entries and mark it incomplete. Previews stop at 512 KiB. Settings appear only after Enter. Report bad chat records; Gemini edits and rewinds are not applied. Settings, instructions, and bundled skills stay view only.
* Skill links require review of source, destination, and unchanged instructions. Source must be inside home with a UTF-8 `SKILL.md`. Never replace a destination. Codex uses shared `.agents/skills`. Scripts stay in the source and never run. The source must remain; later edits affect the live link.
* Edit or restore only user `SKILL.md` files up to 1 MiB. Allow broken files to open for repair. Require name, description, and instructions before saving; this is not a full YAML or tool format check.
* Edit a private copy and review its diff. Keep failed drafts until repaired, discarded, or neet exits. Failed checks leave the original alone.
* Before writing, neet checks the original file and link destination again, saves a backup under `~/.local/state/neet/backups/ai/`, and swaps in the new file. Backup folders refuse links. Backups have mode `0600`; the skill keeps its permissions.
* `b` reviews restoring the latest backup and backs up the current version too.
* Chat cleanup reviews exact transcript paths, titles, count, and size, including hidden selections. Finder moves them to Trash and Put Back can restore them. Cloud history and Codex's own database remain.
* With a Claude Code chat, its related items go to Trash too: `projects/<project>/<id>/`, `file-history/<id>/`, `session-env/<id>/`, and `todos/<id>*.json`, where `<id>` is the chat's session ID. Each is shown in the review. Links are left out, and an item replaced after review stays.
* After the transcripts move, neet removes the lines naming those chats from `~/.claude/history.jsonl`, `~/.codex/history.jsonl`, and `~/.codex/session_index.jsonl`. Other lines, and lines neet cannot read, stay. Each file is backed up under `~/.local/state/neet/backups/ai/history/` first, written only if unchanged since it was read, and skipped if it is a link or larger than 1 MiB.
* `c` runs `claude --resume <id>` in the chat's project folder, or `codex resume <id>`. `z` runs `codex archive <id>` or `codex unarchive <id>`; checked on Codex 0.162.1, which moves the file to `archived_sessions` and back unchanged. Only session IDs shaped like a UUID are passed. Gemini CLI resumes and deletes chats by a list number that can change, so neet does not run them. `codex delete` deletes permanently, so neet does not use it.
* Before a chat move, recheck device, inode, size, change times, and path. Refuse linked files or parents, files open in `lsof`, failed open checks, and files written in the last minute. Close assistants first; an idle assistant can still reopen a file.
* Tag only known missing project paths. Unknown paths and read errors are not missing. Never delete a chat just because its project is missing.
* Plugin skills: Claude Code's `plugins/installed_plugins.json` gives each plugin's install folder, which must be inside home; only the `enabledPlugins` key of its `settings.json` is used. Codex plugins come from `~/.codex/plugins/cache/<marketplace>/<plugin>/<version>`, and Gemini CLI extensions from `~/.gemini/extensions/<name>`. Only `skills/*/SKILL.md` inside them is listed. Plugin skills are view only, because the tool replaces them on update.
* Removing a skill link works only on a link directly in `~/.claude/skills`, `~/.agents/skills`, `~/.codex/skills`, or `~/.gemini/skills`. Copies, bundled skills, plugin skills, and project links are refused.
* Before removing, neet checks that the link is still a link and still points where it did at review. Only the link is removed; the folder it points to is never opened or changed. `u` recreates the same link, and refuses if something now uses its name.
* Remote skill downloads and tool settings changes remain follow-ups.

* Gemini project markers follow its [project registry](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/config/projectRegistry.ts).
* File locations were checked against the official [Claude Code directory guide](https://code.claude.com/docs/en/claude-directory), [Codex skills guide](https://learn.chatgpt.com/docs/build-skills), [Gemini skills guide](https://geminicli.com/docs/cli/skills/), and [Gemini session guide](https://geminicli.com/docs/cli/session-management/). Formats can change; unsupported chat records stay out of the preview.

## Tests

* Default tests use temporary folders and fake actions. Optional real tool tests are marked ignored and need a separate run.
* They cover:
* `..` parts, doubled slashes, and paths outside the allowed folders.
* Allowed folders themselves, the home folder, and the top of a disk.
* Protected folders written in other ways.
* Links that lead out, and links swapped in after the review.
* Files replaced after the review.
* Names neet does not support.
* Your own rules reaching outside the allowed folders, and a `safe` level in your rule counting as `caution`.
* Unsupported patterns, and `*` too early in a path.
* Paths in a container outside its `Caches` folder, and Xcode archives.
* An old folder holding a recently changed file.
* App removal refusing Apple's apps, links, apps in subfolders, open apps, files outside the table, and partial names.
* Build folders and installers: accepted in Documents, Desktop, and Downloads, and refused when nested, in `~/Library` or a hidden folder, inside `.git` or `~/.ssh`, a link, a lone `target`, or a file that is not an installer.
* Tool commands: only runtime IDs reach `simctl`, and what `simctl` and `docker` print is read as data.
* Dotfiles: files outside the list, never listed files, links that lead out, a file changed after it was opened, a failed check, a crash in the middle of a write, restoring every backup, and secrets found before an export.
* Startup: plists that are empty, unreadable, or links; `launchctl` output that cannot be read marks the list incomplete; helpers inside apps count only when launchd knows them; nothing in `/System/Library` is read.
* AI Tools: known paths, bounded previews and summaries, missing-project tags, chat selection and removal checks, partial cleanup failures, skill links, stale reviews, edit and restore backups, draft recovery, search, and narrow terminals.
* Before release, a harmless test item is moved to the Trash by hand and restored with Put Back. This passed on macOS 26.5.
* Removing an app owned by root was tried the same way, and Put Back restored it.
* Planned features add their own tests, for changes outside their lists, links that lead out, and undoing each change.
