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
* [Planned Features](#planned-features)
* [AI Tools](#ai-tools)
* [Tests](#tests)

## The Promises

* Cleanup uses Trash. Docker and simulator tools, and emptying Trash, delete permanently after a separate warning and confirmation.
* Review every path and confirm before files move.
* Cleanup uses fixed folder lists. App removal, build folders, and installers each have their own checks.
* Cleanup protects personal files, cloud files, passwords, and keys. Selected build folders, installers, and chat transcripts have separate rules. Dotfile and skill edits need review and backup.
* Anything that changed since you reviewed it is skipped.
* Cleanup never uses admin rights.

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
| SSH | `.ssh/config` |
| Editors | `.vimrc`, `.config/nvim/init.lua`, `.config/nvim/init.vim`, `.nanorc`, `.editorconfig` |
| Terminal | `.tmux.conf`, `.config/tmux/tmux.conf`, `.config/kitty/kitty.conf`, `.config/ghostty/config`, `.config/ghostty/config.ghostty`, `.config/alacritty/alacritty.toml`, `.config/starship.toml`, `.wezterm.lua` |
| Tools | `.npmrc`, `.config/mise/config.toml`, `.config/gh/config.yml`, `.Brewfile` |

* Adding a file needs a change to this table and to the code, together.
* Never listed, read, or exported:
* Shell and editor history, such as `.zsh_history`, `.bash_history`, and `.viminfo`.
* Files that hold passwords or tokens: `.netrc`, `.git-credentials`, `.config/gh/hosts.yml`, and everything in `~/.aws`, `~/.docker`, and `~/.kube`.
* Everything in `~/.ssh` except `config`.
* Files a shell makes for itself, such as `.zcompdump`.
* `.npmrc` can hold a token. It is marked **secret**, and starts left out of exports.

### What May Change

* Only the files in the table, and, with [chezmoi](#chezmoi), the source file chezmoi keeps for each one.
* View only, never changed:
* `.ssh/config`. Changing it waits for the SSH feature.
* A file that is a link leading outside your home folder, or into a [protected folder](#protected-folders).
* A file neet cannot write as you. neet never uses `sudo` here.
* With chezmoi: templates, encrypted files, and files chezmoi builds from scripts or changes in place.
* Allowed links are edited at their destination. Parent links get the same check. chezmoi source paths containing links are refused.

### How A Change Runs

1. You make the change: edit a copy in your editor, or set a value in Configure. Your editor is `$VISUAL`, then `$EDITOR`, then `nano`.
2. neet checks the new text, when it knows how. See [Checks](#checks).
3. It shows the diff, and asks. Only `y` goes ahead.
4. It checks the file did not change since you opened it: the same identity on disk, the same last change time, permissions, link target, and contents. Save, Put Back, Add, Restore, and Export also keep the files they reviewed and refuse a change made while their question is open. If a file changed, nothing is written.
5. It saves a [backup](#backups).
6. It writes a new copy beside the file, with the same permissions, flushes it to disk, and swaps it in. A crash leaves the old file or the new one, never half of one.
7. With chezmoi, it writes the source file, then runs `chezmoi apply` for that one file, when it [may run chezmoi](#when-neet-may-run-chezmoi).

### Checks

* A check only reads the file. It never runs it.

| File | Check |
| --- | --- |
| zsh files | `zsh -n` |
| bash files and `.profile` | `bash -n` |
| fish | `fish --no-execute` |
| Git files | `git config --file <copy> --list` |
| TOML files | Read with neet's own TOML reader. |
| Everything else | None. The screen says there is no check. |

* `ssh -G` is not used for `.ssh/config`: it runs `Match exec` commands while it reads.
* Save, Put Back, and Restore also check the text they are about to save or restore.
* A failed check is shown with its message. You can go back to your editor, or drop the change. A file that fails its check is never written.

### Backups

* Kept in `~/.local/state/neet/backups/dotfiles/`, by file, then by the time of the change.
* The folder can only be read by you. Each backup keeps the file's permissions.
* Every change saves one first: edits, Configure, restores, and chezmoi's `apply` and `re-add`. With chezmoi, both the file in your home folder and its source file are saved.
* Adding a file to chezmoi saves none: it only makes a new source file, and never writes over one.
* neet never removes a backup. Remove old ones yourself.
* **Restore:** pick a backup, see the diff against the file now, and confirm. The file now is backed up first, so a restore can be undone too.

### Configure

* Configure changes one setting at a time, from a list neet knows for each program. Anything else is changed by editing.
* Each format changes only the lines of the settings it knows. Every other line, comment, and their order stay as they are.

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
* Values are checked before anything is written. Numbers must be whole numbers, settings with choices take only those, and a value with more than one line is refused.
* **Git:** values are read with `git config --file <copy> --list`, which reads only that file, not the files it includes.
* **tmux:** neet never runs tmux. It reads and changes only plain lines: `set`, `set-option`, `setw`, or `set-window-option`, with `-g` or `-s`, then the option and one value. Lines that append (`-a`), name a target (`-t`), hold more than one command, continue onto the next line, or sit inside `%if` blocks are left alone. The last plain line for an option wins, as in tmux.
  1. **Change:** only the value on the last line for the option is replaced, so the line keeps its spacing and any comment after it. With no line, one is added at the end, such as `set -g mouse on`, `set -s escape-time 0`, or `setw -g mode-keys vi`.
  2. **Remove:** every plain line for the option is taken out.
  3. A value with spaces or other characters tmux reads specially is written in single quotes. A value with a `'` in it is refused, and is changed by editing instead.
  4. There is no check for tmux files: tmux has no way to read a file without starting a server.
* **kitty:** neet never runs kitty. A line is an option, spaces, and the rest of the line as its value, and lines that start with `#` are comments, as kitty reads them. Files it includes are not read. A change replaces only the value on the last line for the option, keeping the spacing before it. With no line, one is added at the end, such as `background_opacity 0.95`. Removing takes out every line for the option. There is no check for kitty files.
* **Ghostty:** neet never runs Ghostty. A line is a key, `=`, and a value, with spaces around `=` ignored, and lines that start with `#` are comments, as Ghostty reads them. A value in double quotes is read without them, and an empty value counts as not set. Files it includes with `config-file` are not read. A change replaces only the value on the last line for the key, keeping its spacing and its quotes. With no line, one is added at the end, such as `font-size = 13`. A value with spaces at either end is written in quotes, and one that would need quotes but has a `"` in it is refused. Removing takes out every line for the key. There is no check for Ghostty files.
* **Starship and mise:** `a.b` means the key `b` in the table `[a]`. Changing a value keeps its spacing and comment, and adds a missing table at the end. Removing a value takes out its line, and only when what is left means the same, less that value, so the comments above it stay. A key that holds a table, a list, or an inline table, such as `node = ["20", "22"]` in mise, or a table that is a plain value, is refused, and is changed by editing. The file is checked with the TOML reader, as when editing.
* More programs are added one at a time. Each is listed here before it ships.
* **zsh:** settings only go in a block neet owns, between `# >>> neet >>>` and `# <<< neet <<<`. Lines outside it are never read or changed, and neet never runs zsh to read the file.
  1. With no block, one is added at the end of `.zshrc`, after an empty line, so its settings come after, and win over, the lines above it.
  2. Inside it, each setting is one line: `export EDITOR='nano'`, `HISTSIZE=50000`, or `setopt auto_cd` for on and `unsetopt auto_cd` for off. A change replaces the setting's last line and takes out any others for it. Other lines in the block stay.
  3. An `export` value is written in single quotes, so the shell reads it as it is. A value with a `'` in it is refused, and is changed by editing instead.
  4. Removing a setting takes out its lines. A block left with no settings is taken out, with the empty line before it.
  5. A file with a start marker and no end marker, an end marker first, or more than one block is refused, and is fixed by editing.
  6. The file then goes through the same `zsh -n` check as an edit.

### chezmoi

* neet works with [chezmoi](https://www.chezmoi.io), a dotfile manager, when it is installed.
* chezmoi renders templates and runs hooks, and either can run any program or ask a password manager. So neet reads chezmoi's folder itself, and runs chezmoi only to change a file, only when nothing it would run can run a program.

#### What neet reads itself

1. **The source folder:** `sourceDir` in chezmoi's config file, or `~/.local/share/chezmoi`, then the folder named in its `.chezmoiroot`, if there is one. The config file is `~/.config/chezmoi/chezmoi.toml` or `chezmoi.json`. A config in another format counts as unknown.
2. **Each listed file's source file**, by chezmoi's naming: `dot_` for a leading `.`, and the attribute prefixes `private_`, `readonly_`, `executable_`, and `empty_`.
3. **Whether it differs:** the source file and the file in your home folder are compared byte by byte. The screen says **saved**, **changed**, or **not saved**.
4. Source files that end in `.tmpl`, or start with `encrypted_`, `modify_`, `create_`, or `symlink_`, are view only. neet shows the source file, and never asks chezmoi what it would make.

#### When neet may run chezmoi

* Only when all of these are true. Otherwise neet still writes the source file, and tells you to run `chezmoi apply` for that file yourself.
  1. The config file is one neet can read, and has no `hooks` section. A hook runs on every chezmoi command, even `chezmoi source-path`.
  2. Every `.chezmoiignore`, `.chezmoiremove`, and `.chezmoiexternal` file in the source folder, and everything in a `.chezmoiexternals` folder, is plain, or only uses `if`, `else`, `end`, `eq`, `ne`, `and`, `or`, `not`, and values that start with `.`. chezmoi renders these on almost every command, even ones that only name a single file, with or without `.tmpl` in their name.
  3. The file is not view only.
  4. The file is not a link. chezmoi would replace the link with a plain file. For a link, neet writes the source file itself and says which command to run.

| Command | When | Changes |
| --- | --- | --- |
| `chezmoi apply --no-tty --force --exclude=scripts,externals -- <file>` | After a change to a source file, or to put the source version back. | That one file in your home folder. |
| `chezmoi re-add --no-tty -- <file>` | To keep the version in your home folder. | That file's source file. |
| `chezmoi add --no-tty -- <file>` | To let chezmoi manage a listed file. | Adds one source file. |

* Without chezmoi running, `a` writes the new source file itself, named the way `chezmoi add` names it: `dot_` for a leading `.`, `private_` for a file or folder only you can open, `empty_` for an empty file, and `executable_` for one that runs. It goes into folders chezmoi already has. The name was checked against chezmoi 2.72.2. The file is written beside its place first, then linked in, so it is whole or not there, and never replaces a file.
* `a` never adds a link: another tool, such as GNU Stow, may manage it.

* `--force` lets chezmoi replace a file it did not write last, which it otherwise stops to ask about. neet has already shown the diff, checked the file did not change, and backed it up. It touches only the one file named.
* neet names the file by its path in your home folder, such as `~/.zshrc`, never by where a link leads, since that is the path chezmoi knows.
* Before each command, neet reads chezmoi's settings again and refuses a changed source path, a hook, an unsafe template, or a file that is now view only.
* After each command, neet checks the file now matches, and says so if it does not.
* neet never runs `chezmoi status`, `diff`, `cat`, `managed`, `update`, or `apply` without a file. `status` without a file renders every template and runs `modify_` scripts. neet shows its own diff instead.
* Templates are never changed or rendered by neet.
* Checked with chezmoi 2.72.2, in a test folder where every kind of template and hook wrote down when it ran.

### Export

* Export puts your dotfiles in a Git repository, ready for GitHub.
* **Review for secrets first.** neet looks through every file that will be exported for:
  1. Private key blocks, such as `-----BEGIN OPENSSH PRIVATE KEY-----`.
  2. Known token shapes, such as `ghp_`, `github_pat_`, `sk-`, `AKIA`, and `xox`.
  3. Settings named like secrets, such as `password`, `token`, `secret`, and `_authToken`, with a value.
* Each match is listed with its file and line, its value hidden except the first four characters. You can leave the file out, or edit it, before anything is written.
* This is a review, not a promise that nothing secret remains.
* **With chezmoi:** neet offers only the source files of listed dotfiles that Git shows as changed, each with what the review found. You pick which go in. neet adds them with `git add -- <files>`, and commits only those with `git commit -m <message> -- <files>`, even if other changes are staged. Unreadable files stop the review. The selected files must still match the reviewed copies before anything is staged. You see the message first. Your repository's own Git hooks run as usual.
* **Without chezmoi:** neet starts chezmoi's own folder, `~/.local/share/chezmoi`, or the `sourceDir` in chezmoi's config, as a new repository. The folder must not exist, or be an empty folder that is not a link. Every listed dotfile on this Mac is reviewed the same way, except links that lead out of your home folder or into a protected folder. neet writes only the files you pick, exactly as they were reviewed, named as `chezmoi add` names them and with their permissions, plus a README listing them and a `.chezmoiignore` that leaves the README out. Folders and files are made new, never written over. Then it runs `git init` and commits. A file changed since the review, or a folder that filled in the meantime, stops it before anything is written. If Git has no name and email for you, nothing is written. Anyone can set up a new Mac from it with `chezmoi init --apply <repository>`.
* **Pushing** only happens after a box that names the remote and branch, and only `y` goes ahead. Then neet runs `git push` with the reviewed commit, remote address, and branch named explicitly. Other push settings cannot add branches or tags. A changed commit or destination stops the push. With no remote, it can create a private repository named `dotfiles` with `gh repo create dotfiles --private --source <folder> --remote origin --push`, after the same kind of box. A repository that already has a remote is refused.
* neet never force pushes, pulls, merges, or rebases. It pushes only to the remote branch the branch already tracks. If the remote has commits you do not, Git refuses the push, and neet says to pull first.
* Git runs with fsmonitor turned off and with no password prompt, so it never waits for typing or starts another program on its own.

## Startup Items

* Startup lists programs that start on their own. It only reads, except to turn your own launch agents off and back on.

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
* So apps that open at login are not listed. The screen says so, and `o` opens System Settings › General › Login Items & Extensions, where macOS lists them and turns them off.
* Listing never loads or starts an item and never uses `sudo`. Turning an item on after confirmation may ask launchd to load and start it, as described below.
* When `launchctl` cannot be read, the list says it is incomplete, and each item shows that whether it runs is not known.
* A plist that cannot be read, or is empty, is still listed, and says so.

### What May Change

* Only your own launch agents in `~/Library/LaunchAgents` may be turned off and back on, with `t`, after a question that names the label and the program it runs.

| Change | Commands, in order |
| --- | --- |
| Turn off | `launchctl disable gui/<uid>/<label>`, then `launchctl bootout gui/<uid>/<label>`, which stops it now if it runs. |
| Turn back on | `launchctl enable gui/<uid>/<label>`, then `launchctl bootstrap gui/<uid> <plist>`. |

* `disable` and `enable` are what keep the change after a restart. macOS keeps them in its own list, outside the plist.
* Turning off something that was not loaded is not an error, and neither is turning on something that is already loaded.
* Before any command runs, the state before is saved in `~/.local/state/neet/startup/<label>.json`: the plist, whether it was turned off, and whether it was loaded. It stays after neet closes, so you can always turn it back the way it was.
* The saved record starts with an unknown result. After the commands, it records whether nothing changed, only part finished, or the turn finished. Older records also show an unknown result.
* If only `disable` or `enable` succeeds, the result says the setting for future logins changed and names the command that failed. A failed state lookup leaves the current state unknown.
* The plist is never changed, moved, or removed. Nothing here uses `sudo`, and no command may show a prompt.
* neet refuses, and runs nothing, when:
  1. The item is not in `~/Library/LaunchAgents`, has no label, its label starts with `com.apple.`, or its label has a `/` in it.
  2. The plist is a link that leads out of your home folder, or is not a plain file.
  3. The plist could not be checked when listed, changed since the list or the question, or now names a different label. Look again before trying a changed item.
* View only, always:
  1. Items in `/Library/LaunchAgents` and `/Library/LaunchDaemons`. An admin installed them for every user, sometimes for device management.
  2. Background items inside apps, and apps that open at login. `o` opens System Settings, where macOS turns them off and on.
  3. Items signed by Apple, and everything in `/System/Library`.

## Planned Features

* Later features change files or settings in place, not through the Trash. Each has a fixed list of what it may change.

| Feature | May Change | How |
| --- | --- | --- |
| Shell PATH | Only the shell files in [Dotfiles](#which-files). Never `/etc/paths`. | Edit with a backup |
| SSH permissions | `~/.ssh` and the files directly inside it. | Change permissions |
| SSH known hosts | `~/.ssh/known_hosts` | `ssh-keygen -R` |
| SSH agent | Nothing on disk. | `ssh-add` |
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

## AI Tools

* Discovery reads file names and metadata in known folders. Opening Chats reads up to 512 KiB per transcript for titles and project paths, with a 32 MiB transcript budget per refresh. Gemini project markers read at most 4 KiB each. Other file contents stay closed until you open one. It never opens sign in files or databases, runs skills, or sends file contents to a service.
* Known user files:

| Tool | Settings and instructions | Skills | Chats |
| --- | --- | --- | --- |
| Claude Code | `~/.claude/settings.json`, `~/.claude/CLAUDE.md` | `~/.claude/skills/*/SKILL.md` | `~/.claude/projects/*/*.jsonl` |
| Codex | `~/.codex/config.toml`, `AGENTS.md`, `AGENTS.override.md` under `~/.codex` | `~/.agents/skills/*/SKILL.md`, `~/.codex/skills/*/SKILL.md`, including `.system/*` | `~/.codex/sessions` through year/month/day and `archived_sessions`, only `rollout-*.jsonl` |
| Gemini CLI | `~/.gemini/settings.json`, `~/.gemini/GEMINI.md` | `~/.gemini/skills/*/SKILL.md` and shared `.agents/skills` | `~/.gemini/tmp/*/chats/*.json` and `*.jsonl` |

* `CODEX_HOME` and `CLAUDE_CONFIG_DIR` replace their default roots when set. Resolved paths must stay inside home. Individual file links are skipped. Skill directory links inside home are supported.
* Project discovery checks the launch directory and its parents up to the first Git root, stopping before home. It looks for `CLAUDE.md`, `CLAUDE.local.md`, `.claude/CLAUDE.md`, `AGENTS.md`, `AGENTS.override.md`, `GEMINI.md`, tool settings, `.claude/settings.local.json`, and the `.claude`, `.codex`, `.gemini`, and `.agents` skill folders. It does not search every project on disk.
* Discovery stops at 20,000 directory entries and reports an incomplete list. Previews read at most 512 KiB. Settings can contain tokens, so their contents appear only after `Enter`. Chat previews show supported message text and report malformed records. Gemini JSONL edits and rewinds are not applied; previews report when those records appear. Settings, instructions, and bundled skills are view only.
* Linking a local skill reviews its instructions, source, and destination, then creates a directory link in the chosen user skill folder. Codex links into shared `~/.agents/skills`. The source must be inside home, contain a valid UTF-8 `SKILL.md`, and keep its reviewed instructions unchanged. Existing destinations are never replaced. Scripts and references remain in the source folder and are not run by neet. The source folder must stay in place. This is a live link, so later source changes affect the skill.
* Editing and restoring affect only user `SKILL.md` files of at most 1 MiB. Existing text files with invalid frontmatter can be opened for repair. Before saving, neet checks for frontmatter name and description fields and an instruction body. It does not fully validate YAML or fields specific to a tool.
* Edits use a private temporary copy and require a diff review. A failed edit or save keeps that draft until you repair it with `e`, discard it with `Esc`, or exit neet. The original stays unchanged when validation fails.
* Before writing, neet checks the original file and link destination again, saves a backup under `~/.local/state/neet/backups/ai/`, and swaps in the new file. Backup folders refuse links. Backups have mode `0600`; the skill keeps its permissions.
* `b` reviews restoring the latest backup and backs up the current version too.
* Chat cleanup reviews the exact transcript paths, titles, count, and size before moving them through Finder. It includes all selected chats, even those hidden by filters. Finder's Put Back can restore the files. This does not erase assistant indexes, related files, or cloud history. Cached entries may remain in the assistant's own history list.
* Before each move, neet rechecks the file's device, inode, size, modification time, change time, and path. It refuses file links and linked parents. `lsof` checks for open files; a failed check skips the file. Files written in the last minute are also skipped. Close assistants first: an idle assistant can reopen or recreate a transcript, so these checks are not a session lock.
* Missing-project tags use recorded absolute paths, including Gemini's `.project_root` marker. Unknown paths and permission errors are not treated as missing. neet never deletes a chat just because its project is missing.
* Resume, archive, assistant index cleanup, skill removal, remote skill downloads, and tool settings changes remain follow-ups.

* Gemini project markers follow its [project registry](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/config/projectRegistry.ts).
* File locations were checked against the official [Claude Code directory guide](https://code.claude.com/docs/en/claude-directory), [Codex skills guide](https://learn.chatgpt.com/docs/build-skills), [Gemini skills guide](https://geminicli.com/docs/cli/skills/), and [Gemini session guide](https://geminicli.com/docs/cli/session-management/). Formats can change; unsupported chat records stay out of the preview.

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
  14. Dotfiles: files outside the list, never listed files, links that lead out, a file changed after it was opened, a failed check, a crash in the middle of a write, restoring every backup, and secrets found before an export.
  15. Startup: plists that are empty, unreadable, or links; `launchctl` output that cannot be read marks the list incomplete; helpers inside apps count only when launchd knows them; nothing in `/System/Library` is read.
  16. AI Tools: known paths, bounded previews and summaries, missing-project tags, chat selection and removal checks, partial cleanup failures, skill links, stale reviews, edit and restore backups, draft recovery, search, and narrow terminals.
* Before release, a harmless test item is moved to the Trash by hand and restored with Put Back. This passed on macOS 26.5.
* Removing an app owned by root was tried the same way, and Put Back restored it.
* Planned features add their own tests, for changes outside their lists, links that lead out, and undoing each change.
