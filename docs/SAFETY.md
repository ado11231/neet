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
11. [Dotfiles](#dotfiles)
12. [Planned Features](#planned-features)
13. [Tests](#tests)

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
* Finder has a minute to answer each move. If it does not, the item is skipped with the reason. Finder may still move it later, so look in the Trash before trying again.
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

* `x` on the Docker screen resets Docker: every image, container, and volume goes at once, by moving Docker's disk image to the Trash. It is not permanent: moving it back restores it.
* neet asks first, in a yellow box that names the size and says what goes with it. Only `y` goes ahead. Then neet:
  1. Stops Docker Desktop with `docker desktop stop`, then waits, up to three minutes, until `lsof` says no process has the disk image open. If one still does, or `lsof` cannot tell, nothing moves.
  2. Checks the disk image with the [build folder check](#build-folders-and-installers), which takes only that one file.
  3. Moves it into `~/.Trash` itself, keeping its name, or adding a number if the Trash already has a `Docker.raw`. Finder cannot reach into another app's container folder, and hangs when asked, so Finder's Put Back does not know where it came from.
* Docker Desktop makes a new, empty disk image the next time it opens. To undo, move `Docker.raw` from the Trash back to `~/Library/Containers/com.docker.docker/Data/vms/0/data/` before opening Docker Desktop again.

## Dotfiles

* Dotfiles changes settings files in place, not through the Trash. Every change shows a diff first, is saved as a backup, and can be undone.
* This section is written before the feature is built. Each part ships view only first.

### Which Files

* Only these files are listed. Paths are inside your home folder.

| Group | Files |
| --- | --- |
| Shell | `.zshrc`, `.zprofile`, `.zshenv`, `.zlogin`, `.zlogout`, `.bashrc`, `.bash_profile`, `.profile`, `.inputrc`, `.config/fish/config.fish` |
| Git | `.gitconfig`, `.config/git/config`, `.config/git/ignore`, `.gitignore_global` |
| SSH | `.ssh/config` |
| Editors | `.vimrc`, `.config/nvim/init.lua`, `.config/nvim/init.vim`, `.nanorc`, `.editorconfig` |
| Terminal | `.tmux.conf`, `.config/tmux/tmux.conf`, `.config/kitty/kitty.conf`, `.config/ghostty/config`, `.config/alacritty/alacritty.toml`, `.config/starship.toml`, `.wezterm.lua` |
| Tools | `.npmrc`, `.config/mise/config.toml`, `.config/gh/config.yml`, `.Brewfile` |

* Adding a file needs a change to this table and to the code, together.
* Never listed, read, or exported:
  1. Shell and editor history, such as `.zsh_history`, `.bash_history`, and `.viminfo`.
  2. Files that hold passwords or tokens: `.netrc`, `.git-credentials`, `.config/gh/hosts.yml`, and everything in `~/.aws`, `~/.docker`, and `~/.kube`.
  3. Everything in `~/.ssh` except `config`.
  4. Files a shell makes for itself, such as `.zcompdump`.
* `.npmrc` can hold a token. It is marked **secret**, and starts left out of exports.

### What May Change

* Only the files in the table, and, with [chezmoi](#chezmoi), the source file chezmoi keeps for each one.
* View only, never changed:
  1. `.ssh/config`. Changing it waits for the SSH feature.
  2. A file that is a link leading outside your home folder, or into a [protected folder](#protected-folders).
  3. A file neet cannot write as you. neet never uses `sudo` here.
  4. With chezmoi: templates, encrypted files, and files chezmoi builds from scripts or changes in place.
* A file that is a link inside your home folder is changed where the link leads. The link itself stays.

### How A Change Runs

1. You make the change: edit a copy in your editor, or set a value in Configure. Your editor is `$VISUAL`, then `$EDITOR`, then `nano`.
2. neet checks the new text, when it knows how. See [Checks](#checks).
3. It shows the diff, and asks. Only `y` goes ahead.
4. It checks the file did not change since you opened it: the same identity on disk, the same last change time, and the same contents. If it changed, nothing is written.
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

* Changes are made on the edit copy, then go through the same check, diff, backup, and chezmoi steps as an edit.
* Values are read with `git config --file <copy> --list`, which reads only that file, not the files it includes. A value with more than one line is refused.
* More programs are added one at a time. Each is listed here before it ships.
* Shell settings, when added, only go in a block neet owns, between `# >>> neet >>>` and `# <<< neet <<<`. Lines outside it are never changed.

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

| Command | When | Changes |
| --- | --- | --- |
| `chezmoi apply --no-tty --force --exclude=scripts,externals -- <file>` | After a change to a source file, or to put the source version back. | That one file in your home folder. |
| `chezmoi re-add --no-tty -- <file>` | To keep the version in your home folder. | That file's source file. |
| `chezmoi add --no-tty -- <file>` | To let chezmoi manage a listed file. | Adds one source file. |

* Without chezmoi running, `a` writes the new source file itself, named the way `chezmoi add` names it: `dot_` for a leading `.`, `private_` for a file or folder only you can open, `empty_` for an empty file, and `executable_` for one that runs. It goes into folders chezmoi already has. The name was checked against chezmoi 2.72.2. The file is written beside its place first, then linked in, so it is whole or not there, and never replaces a file.
* `a` never adds a link: another tool, such as GNU Stow, may manage it.

* `--force` lets chezmoi replace a file it did not write last, which it otherwise stops to ask about. neet has already shown the diff, checked the file did not change, and backed it up. It touches only the one file named.
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
* **With chezmoi:** neet offers only the source files of listed dotfiles that Git shows as changed, each with what the review found. You pick which go in. neet adds them with `git add -- <files>`, and commits only those with `git commit -m <message> -- <files>`, even if other changes are staged. You see the message first. Your repository's own Git hooks run as usual.
* **Without chezmoi:** neet writes a new folder you pick, in chezmoi's layout, with a README listing the files, then runs `git init` and commits. The folder must not exist, or be empty, and may not be in `~/Library`, a cloud folder, or a protected folder. Anyone can set up a new Mac from it with `chezmoi init --apply <repository>`.
* **Pushing** only happens after a box that names the remote and branch, and only `y` goes ahead. Then neet runs `git push`. With no remote, it can create a private repository with `gh repo create --private --source <folder> --push`, after the same kind of box.
* neet never force pushes, pulls, merges, or rebases. It pushes only to the remote branch the branch already tracks. If the remote has commits you do not, Git refuses the push, and neet says to pull first.
* Git runs with fsmonitor turned off and with no password prompt, so it never waits for typing or starts another program on its own.

## Planned Features

* Later features change files or settings in place, not through the Trash. Each has a fixed list of what it may change.

| Feature | May Change | How |
| --- | --- | --- |
| Shell PATH | Only the shell files in [Dotfiles](#which-files). Never `/etc/paths`. | Edit with a backup |
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
  14. Dotfiles, once built: files outside the list, never listed files, links that lead out, a file changed after it was opened, a failed check, a crash in the middle of a write, restoring every backup, and secrets found before an export.
* Before release, a harmless test item is moved to the Trash by hand and restored with Put Back. This passed on macOS 26.5.
* Removing an app owned by root was tried the same way, and Put Back restored it.
* Planned features add their own tests, for changes outside their lists, links that lead out, and undoing each change.
