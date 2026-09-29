# Interface

* The planned screens, keyboard keys, and review steps for neet.
* For anyone building or reviewing the terminal interface.
* No screens are built yet.
* For what each feature does and its status, read [FEATURES.md](FEATURES.md). For the rules every change must follow, read [SAFETY.md](SAFETY.md).

| Screens | Phase |
| --- | --- |
| Home, Disk, Clean | P1 |
| Large Files, Remove App | P1.5. Remove App waits on its [design question](FEATURES.md#design-questions). |
| Startup, SSH, Dotfiles, PATH, AI Tools, Settings, Space Breakdown, Projects | P2. Draft designs that still need checking on real Macs. |

## Contents

1. [The Terminal Interface](#the-terminal-interface)
2. [The Home Screen](#the-home-screen)
3. [The Skipped Screen](#the-skipped-screen)
4. [The Space Breakdown Screen](#the-space-breakdown-screen)
5. [Moving Between Screens](#moving-between-screens)
6. [The Disk Screen](#the-disk-screen)
7. [The Clean Screen](#the-clean-screen)
8. [The Projects Screen](#the-projects-screen)
9. [The SSH Screen](#the-ssh-screen)
10. [The Dotfiles Screen](#the-dotfiles-screen)
11. [The PATH Screen](#the-path-screen)
12. [The Startup Screen](#the-startup-screen)
13. [The AI Tools Screen](#the-ai-tools-screen)
14. [The Settings Screen](#the-settings-screen)

## The Terminal Interface

* Run `neet` to open it.
* The only options are `--version`, which prints the version, and `--help`, which prints a short usage note. There are no other commands or options.
* neet opens on the Home screen, and starts scanning your home folder straight away, in the background.
* Each feature has its own screen. You open it from the Home menu, and go back to Home when you are done.

## The Home Screen

```text
+----------------------------------+-----------------------------+
|                                  |  > Disk          412 GB used|
|                                  |    Clean         ~18 GB     |
|           ASCII art              |    Large Files       soon   |
|                                  |    Startup           soon   |
|                                  |    ...                      |
|                                  |    Quit                     |
|                                  +-----------------------------+
|                                  | Browse your folders by size |
|                                  | Disk [#######...] 82% used  |
|                                  | Scanning: 1.2M files        |
+----------------------------------+-----------------------------+
 up/down move . enter open . ? help . q quit
```

* **Left:** ASCII art of a sleeping cat behind the neet wordmark, both in the blue `#82aaff`, under a sky of grey stars that thins out towards uneven edges. It takes about 45% of the width, and always leaves a gap before the menu. On a terminal narrower than about 90 columns the art is hidden and the menu fills the screen.
* **Right, top:** the menu. One row per feature, with a short summary, such as used space or what can be cleaned.
* **Right, bottom:** a panel that explains the selected row, and shows:
  1. A gauge of how full the disk is, with free and total space. It turns yellow above 75% and red above 90%.
  2. Scan progress, and whether the last scan was complete.
* Features that are not built yet are shown dimmed, marked `soon`. The selection skips over them.
* The Home screen replaces a separate Dashboard.
* Folder sizes and disk totals are shown separately. They can differ because of APFS copies, snapshots, and space macOS frees on its own.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` or `Right` | Open the selected screen. |
| `1` to `9` | Open that row directly. |
| `s` | Open the Skipped screen. |
| `b` | Open the Space Breakdown screen. Proposed for P2. |
| `?` | Help. |
| `q` | Quit. |

## The Skipped Screen

* Lists what the scan could not read, with the reason for each path, and the folders on other disks it did not enter.
* When macOS blocked a folder, it explains how to turn on Full Disk Access for your terminal.
* Says so when nothing was skipped.
* When the scan is incomplete, the Home panel says how many paths were skipped and to press `s`.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Scroll. |
| `PgUp` / `PgDn` | Scroll a page. |
| `g` / `G` | Jump to the top or bottom. |

## The Space Breakdown Screen

* Proposed for P2. Opened with `b` on Home.
* Explains why the disk's used space is bigger than the home folder scan.
* View only. It never deletes snapshots or frees purgeable space.

```text
 Disk used                               412 GB
   Home folder scan                      290 GB   [##############......]
   Skipped in home folder              unknown    3 paths, press s
   Apps in /Applications                  38 GB
   macOS and its other volumes            24 GB
   Local Time Machine snapshots           2 snapshots, size estimated
   Purgeable                              31 GB
   Not explained                          29 GB
```

| Part | Where The Number Comes From |
| --- | --- |
| Home folder scan | The scan neet already ran. |
| Skipped in home folder | Folders the scan could not read. Their size is unknown, so it is never guessed. |
| Apps in /Applications | A quick scan of `/Applications`, which changes nothing. |
| macOS and its other volumes | The used space of the system, Preboot, Recovery, and VM volumes, from `diskutil apfs list`. |
| Local snapshots | `tmutil listlocalsnapshots /`. macOS may not report their sizes, so the screen says when a size is estimated or missing. |
| Purgeable | The difference between the free space macOS reports for important files and the plain free space. |
| Not explained | Whatever is left. Shown plainly, never spread over the other parts. |

* Each part has a short note on what it is, and whether you can do anything about it. For example, snapshots go away on their own, or when Time Machine backs up.
* Every source above must be checked on real Macs before this is built. See the [design question](FEATURES.md#design-questions).

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` | Open the selected part in Disk, when it is a folder. |
| `?` | Explain the selected part. |

## Moving Between Screens

* Screens stack. Clean opens the path review, and the path review opens the confirmation.
* `Esc` always goes back one step. From a feature screen it goes back to Home.
* Going back never skips a step forward. The path review can only be left by going back, or by moving on to the confirmation.

| Key | Action |
| --- | --- |
| `Esc` | Back one step. |
| `?` | Help for the current screen. |
| `q` | Quit, when no dialog is open. |

* Arrow keys work everywhere. The Vim keys `h`, `j`, `k`, and `l` work outside text fields.
* `Left` belongs to each screen. In Disk it opens the parent folder, so use `Esc` to leave.

## The Disk Screen

* A column browser that starts at your home folder.
* The left column lists the current folder. Each row shows a bar and a percent for its share of the folder, its size on disk, and its name. Folders end in `/`, and symbolic links in `@`.
* The title shows the folder's path, its total size, how many items are inside it, and the sort order.
* The right column previews the selected row: a folder's contents, or a file's size, kind, and path. It is hidden on terminals narrower than 100 columns.
* Until the scan finishes, the screen shows its progress instead.

| Key | Action |
| --- | --- |
| `Right`, `Enter`, or `l` | Open the selected folder. |
| `Left` or `h` | Go up to the parent folder, with the folder you left still selected. |
| `Up` / `Down` | Move the selection. |
| `s` | Sort by size, then name, then items. The selection stays on the same row. |
| `d` | Plan a cleanup of the selected item. Not built yet. |
| `g` / `G` | Jump to the first or last row. |

* A cleanup started here goes through the same checks, review, and question as any other.

## The Clean Screen

```text
+ Clean -----------------------------------------------------+
|   [x] Xcode DerivedData   safe       nothing found     0 B |
| > [ ] npm cache           caution          3 items  4.6 GB |
|   [ ] User logs           caution         27 items 83.0 MB |
+ Selected: 0 items, 0 B . they go to the Trash -------------+
+ npm cache -------------------------------------------------+
| Packages npm downloaded before. ...                        |
| caution . You may need to download, index, or sign in ... |
|                                                            |
|    4.3 GB  ~/.npm/_cacache/content-v2                      |
|    skipped ~/Library/Logs/App  changed in the last 7 days  |
+------------------------------------------------------------+
```

* When it opens, neet loads the rules and makes a dry run plan in the background. Nothing is changed while it looks. On a real home folder this took about 7 seconds.
* Each row shows a rule's name, risk tier, number of items, and estimated size. Rules that found nothing are dimmed.
* The list's bottom edge shows how many items and how much space the selected rules cover.
* The panel below explains the selected rule: what it removes, its tier, apps to close, its minimum age, and every path it found or skipped, with the reason.
* Rules that could not be loaded are listed at the top of the panel.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between rules. |
| `g` / `G` | Jump to the first or last rule. |
| `Space` | Select or clear a rule. A rule that found nothing cannot be selected. An `expert` rule opens a box where you type the rule's ID, then `Enter`. A wrong ID selects nothing, and `Esc` closes the box. Clearing never needs typing. |
| `Enter` | Review the paths the selected rules found. Does nothing when no rule is selected. |

| Tier | How It Is Selected |
| --- | --- |
| `safe` | Selected from the start. |
| `caution` | You select it yourself. |
| `expert` | You select it and type a confirmation. |

* A rule cannot be selected while its app is open.
* Every cleanup follows the same steps:
  1. Select rules.
  2. Review every path.
  3. Confirm the number of items, their size, and that they go to the Trash.
  4. neet checks each path again, then moves it to the Trash.
* The path review cannot be skipped.
* The first cleanup makes macOS ask whether neet may control Finder. Finder moves the items, so Put Back works.
* When it finishes, the screen shows how much space the cleaned items take up in the Trash, and that emptying the Trash frees it.

### Review, Question, And Cleanup

| Step | What It Shows | Keys |
| --- | --- | --- |
| Review | Every path the selected rules found, grouped by rule, with sizes. The title shows the item count and total size. | `Up` / `Down`, `PgUp` / `PgDn`, and `g` / `G` scroll. `Enter` goes on to the question. `Esc` goes back to Clean. |
| Question | A box over the review: how many items, their size, that Finder moves them to the Trash, and that each is checked again. | `y` moves them. `n` or `Esc` goes back. `q` does nothing here. |
| Cleanup | A progress bar while items move, and a note that macOS may ask about controlling Finder the first time. | No key leaves this step until every item is dealt with. |
| Result | How many items moved, the space they take up in the Trash, how to use Put Back, and every skipped item with its reason. | `Enter` or `Esc` goes back to Home. The next visit to Clean makes a fresh plan. |

## The Projects Screen

* Proposed for P2. Waits on its [design question](FEATURES.md#design-questions), because project folders are outside the cleanup roots.
* Lists build output inside your code projects, found by the home folder scan. The folders and tiers are in [FEATURES.md](FEATURES.md#project-build-folders).

```text
 Project                      Last Changed   Folder          Size
 ~/code/old-game              14 months      target/         9.2 GB
 ~/Documents/rust/neet        today          target/         2.1 GB   too recent
 ~/code/site                  5 months       node_modules/   1.4 GB
 ~/code/scratch               8 months       node_modules/   610 MB   not ignored by Git
```

* Rows that cannot be selected are dimmed, with the reason: too recent, no Git repository, not ignored by Git, or missing its project file.
* The title shows how many folders are selected and their total size.

| Key | Action |
| --- | --- |
| `Space` | Select or clear a folder. |
| `a` | Select every folder in projects not changed within a chosen number of months. |
| `s` | Sort by size, then last changed, then project name. |
| `Enter` | Review the selected paths. |
| `?` | Explain the selected folder, and what removing it costs. |

* A cleanup from here goes through the same steps as Clean: review every path, confirm, check each path again, then move it to the Trash.

## The SSH Screen

* Shows what is in `~/.ssh` without ever showing a private key.

| Section | Shows |
| --- | --- |
| Config | Each host in `~/.ssh/config`, with its host name, user, port, and key. |
| Keys | Each key's type, size, fingerprint, comment, whether it has a passphrase, and whether its `.pub` file exists. |
| Agent | The keys loaded in the SSH agent, from `ssh-add -l`. |
| Known hosts | Each entry in `~/.ssh/known_hosts`. |
| Permissions | Files and folders that other users can read or change. |

* It can make three changes:

| Action | What Happens |
| --- | --- |
| Fix permissions | Sets `~/.ssh` to `700`, private keys and `config` to `600`, and public keys to `644`. The old permissions are saved so you can undo. |
| Agent keys | Adds a key with `ssh-add`, or removes it with `ssh-add -d`. Key files are not touched. |
| Old hosts | Removes the entries you select with `ssh-keygen -R`, which keeps `known_hosts.old`. neet also saves its own backup. |

* Each change shows what it will do and asks first.
* To edit `~/.ssh/config`, use the Dotfiles screen.

## The Dotfiles Screen

* Lists the dotfiles neet knows about, but only the ones on your Mac. If you have no tmux settings, there is no tmux row.

| Group | Files |
| --- | --- |
| Shell | `~/.zshrc`, `~/.zprofile`, `~/.zshenv`, `~/.bashrc`, `~/.bash_profile`, `~/.profile`, `~/.config/fish/config.fish` |
| Git | `~/.gitconfig`, `~/.config/git/config`, `~/.config/git/ignore`, `~/.gitignore_global` |
| SSH | `~/.ssh/config` |
| Editors | `~/.vimrc`, `~/.config/nvim/init.lua`, `~/.config/nvim/init.vim` |
| Terminal | `~/.tmux.conf`, `~/.config/tmux/tmux.conf`, `~/.config/starship.toml`, `~/.inputrc` |

### Edit A Dotfile

1. neet saves a backup in `~/.local/state/neet/backups/`, named with the date and time.
2. It opens the file in your editor, from `$VISUAL` or `$EDITOR`.
3. When you close the editor, it shows what changed.
4. It checks the file for mistakes, where a check exists:

   | File | Check |
   | --- | --- |
   | zsh | `zsh -n` |
   | bash | `bash -n` |
   | Git | `git config --list --file` |
   | SSH | `ssh -G` |

5. You keep the change, edit again, or restore the backup.

### Export Dotfiles

1. Select the files to export.
2. neet looks for anything that looks like a token or password, and shows you each match.
3. Choose where to save it. The default is your home folder.
4. It writes a `.tar.gz` archive, with a list of what was included and what was left out.

* These are never exported: private keys, `~/.netrc`, `~/.aws/credentials`, `~/.npmrc`, and `~/.pypirc`.
* To keep dotfiles in Git, unpack the archive into a repository.

## The PATH Screen

* Proposed for P2.
* Shows your `PATH` as an ordered list, and which copy of a program runs.

```text
 #  Folder                         Programs  Added By              Problems
 1  /opt/homebrew/bin                   212  brew shellenv
 2  ~/.cargo/bin                         18  ~/.zshrc line 12
 3  ~/.local/bin                          0  ~/.zshrc line 14      does not exist
 4  /usr/local/bin                       31  /etc/paths
 5  ~/.cargo/bin                         18  ~/.zprofile line 3    listed twice
 6  /usr/bin                            980  /etc/paths
```

* The right column previews the selected folder: the programs in it, and which of them are hidden by an earlier folder.
* Folders from `/etc/paths` and `/etc/paths.d` are marked view only.

| Problem | What It Means |
| --- | --- |
| Listed twice | Only the first one is used. The later one does nothing. |
| Does not exist | The folder is missing, so it only slows down the search. |
| Others can change it | Another user could put a program here that runs in place of yours. |
| Relative folder | A folder such as `.` changes with where you are, so a program in any folder could run. |

### Which Program Wins

* Press `w` and type a program name.
* The screen lists every copy on your `PATH`, in order, and marks the one that runs.
* For programs with a known version option, such as `--version`, it shows each copy's version. It never runs an unknown program to find out.

### Changing The PATH

1. Move a folder with `K` and `J`, add one with `a`, or remove one with `x`. Nothing is written yet.
2. neet shows the old and new `PATH` side by side.
3. It saves a backup of the shell file, writes the change, shows what changed in the file, and runs its syntax check, the same as [editing a dotfile](#edit-a-dotfile).
4. It starts a fresh login shell and shows the `PATH` it gets.
5. You keep the change, or restore the backup.

* Open terminals keep their old `PATH`. The screen says to open a new one.
* A line such as `eval "$(brew shellenv)"` moves as a whole. neet never edits inside it.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `w` | Find which copy of a program runs. |
| `K` / `J` | Move the selected folder up or down. |
| `a` | Add a folder. |
| `x` | Remove the selected folder. |
| `Enter` | Review and write the changes. |
| `?` | Explain the selected problem. |

## The Startup Screen

* Lists the programs that start on their own. Many never appear in the Dock or menu bar.

| Section | Shows | Read From |
| --- | --- | --- |
| Login items | Apps that open when you log in. | The login item list |
| Background items | Items listed under Allow in the Background in System Settings. | `sfltool dumpbtm` |
| Launch agents | Programs that run while you are logged in. | `~/Library/LaunchAgents`, `/Library/LaunchAgents` |
| Launch daemons | Programs that run for the whole Mac, even before anyone logs in. | `/Library/LaunchDaemons` |

* Each row shows the program, publisher information where available, whether it is running, and whether it is signed.
* Items under `/System/Library` are shown, but can never be changed.
* Turning an item off:
  1. neet shows the exact command, such as `launchctl disable gui/501/com.example.helper`.
  2. It saves the item's current state.
  3. It runs the command, with `sudo` when the item runs for the whole Mac.
* Turning it back on puts back the saved state.
* neet never edits or deletes a launch agent or launch daemon file.
* If neet cannot read the background items, the screen says its list may be incomplete.

## The AI Tools Screen

* Shows the settings, instruction files, and skills that Claude Code and Codex keep on your Mac.
* View only. There are no actions that change a file.

| Section | Shows | Read From |
| --- | --- | --- |
| Tools | Which tools are set up, their folders, and how much space each uses. | `~/.claude`, `~/.codex` |
| Settings | Each tool's settings file. | `~/.claude/settings.json`, `~/.codex/config.toml` |
| Instructions | Global instruction files, when they exist. | `~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md` |
| Skills | Each skill's name, description, tool, source, and path. | `SKILL.md` files in each tool's `skills` folder and plugins |
| Project skills | Skills saved inside project folders, grouped by project. | Folders such as `<project>/.claude/skills`, found by the home folder scan |

| Key | Action |
| --- | --- |
| `Enter` | Open the selected file or skill. |
| `Up` / `Down` | Move the selection. |

* Opening a skill shows its `SKILL.md` and lists the other files in its folder.
* Sign in files, chat history, session logs, and databases are never listed or opened.

## The Settings Screen

* Shows only settings that change speed, battery life, or sleep.
* Sections that do not apply to this Mac are hidden.

### Power Mode

| Preset | Effect |
| --- | --- |
| Battery Life | Turns on Low Power Mode. Slower and cooler, and the battery lasts longer. |
| Balanced | The macOS default. |
| Maximum Performance | Turns on High Power Mode, on Macs that support it. Faster during long, heavy work, with more fan noise and power use. |

* Set separately for battery and for the charger.
* Uses `pmset`. The exact setting names differ between macOS versions, and must be checked on a real Mac before this is built.

### Graphics Switching

* Only on Intel Macs with two GPUs.
* Turns Automatic Graphics Switching on or off, with `pmset gpuswitch`.
* On saves battery by using the built in GPU for light work. Off always uses the faster GPU.

### Refresh Rate

* Lists each display, including ProMotion and supported external displays, with:
  1. Its current refresh rate.
  2. The rates it offers.
  3. Whether it supports Adaptive Sync.
* Switching:
  1. Pick a rate. Higher is smoother. Lower uses less power.
  2. neet switches the display, then asks whether to keep it.
  3. If you do not answer within 15 seconds, it switches back. So if the screen goes blank, it fixes itself.
* Uses CoreGraphics display modes.

### Game Mode

* Shows whether this Mac offers Game Mode.
* Explains how to get it: open a supported game in full screen. macOS then turns Game Mode on and lets the game use the CPU and GPU first.
* neet cannot turn Game Mode on. It only explains.

### What Keeps Your Mac Awake

| View | Shows | Read From |
| --- | --- | --- |
| Now | Apps keeping the Mac awake right now, and the reason each gives. | `pmset -g assertions` |
| Overnight | Each time the Mac woke while asleep, and what woke it. | `pmset -g log` |

* Each row links to the app or setting that caused it. It can show the app in Finder, or jump to the matching wake setting below.
* This view changes nothing. neet never quits an app.

### Wake Settings

* These are draft controls. Which Macs support them, and what they really do, must be checked before they are built.

| Setting | Turning It Off Means | `pmset` Key |
| --- | --- | --- |
| Wake for network access | The Mac no longer wakes for network requests, such as file sharing. | `womp` |
| Power Nap | The Mac no longer checks mail or runs backups while asleep. | `powernap` |
| Keep network connections alive | Some network features may stop working during sleep. The exact effects need checking. | `tcpkeepalive` |

### Every Change

1. neet shows the exact command it will run.
2. It saves the current value.
3. It runs the command, with `sudo` only when administrative rights are required.
4. Undo puts back the saved value.

* Refresh rate changes show the new display mode instead of a command. They use CoreGraphics, and switch back after 15 seconds unless you keep them.
* Game Mode, What Keeps Your Mac Awake, and Overnight are view only.
