# Interface

* How each screen is laid out, what it shows, and which keys it uses.
* For anyone using neet, or changing a screen.
* For what each feature does, read [FEATURES.md](FEATURES.md).

## Contents

1. [Every Screen](#every-screen)
2. [Home](#home)
3. [Skipped](#skipped)
4. [Disk](#disk)
5. [Quick Clean](#quick-clean)
6. [Deep Clean](#deep-clean)
7. [Review, Confirm, And Move](#review-confirm-and-move)
8. [Large Files](#large-files)
9. [Remove App](#remove-app)
10. [Planned Screens](#planned-screens)

## Every Screen

* Run `neet` to open it. It opens on Home and starts scanning your home folder in the background.
* Each screen fills the terminal. The footer lists keys for the current screen and wraps when needed.
* The row under the arrow turns bold and keeps its colors, so sizes, bars, and labels read the same as the rows around it. Body text uses the terminal foreground. The artwork’s moonlight blue (`#82aaff`) marks headings, keys, and progress; green marks selection and success; yellow marks caution; red marks errors and permanent removal. Labels and symbols carry the same meaning as the color. Gray and dim styles are reserved for the unchanged Home artwork.
* Screens stack: each one opens on top of the last, and `Esc` goes back one step.
* Boxes, such as help and questions, open in the middle of the screen, on top of it.
* The screen redraws about four times a second, so progress stays current.
* While a screen waits for slow work, such as the scan, it shows a small box in the middle: what neet is doing, a spinner, how far it has got, and what shows when it is done.

* Numeric headers and values share a right edge. Names and paths align left. Column widths account for the border, padding, and selection arrow; numeric widths grow to fit formatted values.
* At narrow widths, decorative bars disappear first, followed by secondary metadata. Names, sizes, checkboxes, and risk labels stay visible. Selected details retain hidden metadata. Names and paths shorten by terminal cells without splitting Unicode graphemes.
* Dialogs and loading boxes use the same word wrapping for measurement and rendering. Review, results, and skipped paths scroll through wrapped lines.
* Screen copy uses short labels and commands. Confirmations still name the action, whether removal is permanent, and how to restore items sent to Trash.

| Key | Action |
| --- | --- |
| Arrow keys, or `h` `j` `k` `l` | Move. |
| `Esc` | Go back one step. |
| `?` | Show the keys for this screen. |
| `q` | Quit. Does nothing while a question is open or files are moving. |

## Home

```text
+--------------------------------+  +- neet ------------------------------+
|                                |  | > 1 Quick Clean   start here        |
|                                |  |   2 Deep Clean    ~8.8 GB found     |
|                                |  |   3 Remove App                      |
|              Art               |  |   4 Large Files                     |
|                                |  |   5 Disk          412.2 GB used     |
|                                |  |     ...                             |
|                                |  |     Quit                            |
|                                |  +-------------------------------------+
|                                |  +-------------------------------------+
|                                |  | Quick Clean                         |
|                                |  | Start here: everything taking space |
|                                |  | [########......] 83% used           |
|                                |  | 82.0 GB free of 494.4 GB            |
|                                |  | Scanning your home folder...        |
+--------------------------------+  +-------------------------------------+
 up/down move . enter open . s skipped . ? help . q quit
```

* **Left:** the neet art. It takes about 45% of the width, and is hidden when the terminal is narrower than 90 columns.
* **Right, top:** the menu, one numbered row per feature.
  1. The rows go from the quickest way to free space to the most detailed: Quick Clean, Deep Clean, Remove App, Large Files, then Disk.
  2. Quick Clean comes first, marked `start here`, and is selected when neet opens.
  3. Deep Clean shows `finding...`, then the total that every rule found. This is worked out again after each cleanup, and when you press `r` in Deep Clean.
  4. Disk shows how much of the disk is used.
  5. Features not built yet remain readable and have a yellow `soon` label. The selection skips them.
* **Right, bottom:** details for the selected row:
  1. What the feature does.
  2. A 16 character gauge of how full the disk is, then free and total space on the next line. The gauge turns yellow at 75% and red at 90%.
  3. When macOS can clear 100 MB or more on its own, a line says how much free space Finder shows, counting that purgeable space. It is read about once a minute.
  4. Scan progress, then the home folder's total size. If some folders could not be read, the size is marked `at least`, and a yellow line says how many were blocked by macOS or could not be read, and to press `s`.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` or `Right` | Open the selected screen. |
| `1` to `9` | Open that row. Rows after the ninth have no number. |
| `s` | Open Skipped. |

## Skipped

* A single scrolling list:
  1. If macOS blocked some folders, numbered steps to turn on Full Disk Access for the terminal app neet runs in. The app is named when neet can tell which it is, such as Terminal, iTerm, kitty, Ghostty, or Visual Studio Code.
  2. Folders macOS blocked, sorted, under one heading with their count.
  3. Any other folder the scan could not read, with the reason.
  4. Folders on other disks, which the scan does not enter.
* Says so when nothing was skipped.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Scroll. |
| `PgUp` / `PgDn` | Scroll a page. |
| `g` / `G` | Jump to the top or bottom. |

## Disk

```text
+ ~/Library ------------ 58.2 GB . 312 items . sort: size +  +- Caches -----------------+
| > ##########..  48%   28.0 GB  Application Support/     |  | ####......  31%  3.1 GB  |
|   ####........  22%   12.8 GB  Caches/                  |  | ##........  12%  1.2 GB  |
|   ##..........   9%    5.2 GB  Containers/              |  | ...                      |
+---------------------------------------------------------+  +--------------------------+
```

* **Left, 55% of the width:** the current folder, largest first. A table aligns Size, percent, and Name beneath their headers, with a 12 character bar when space allows. Folders end in `/`, links in `@`.
* Sizes and bars of 5 GB or more are red, and of 1 GB or more yellow, as in Large Files.
* The title shows the folder's path on the left, and its size, item count, and sort order on the right.
* **Right:** the selected row, titled with its name in bold moonlight blue. The preview is hidden when the terminal is narrower than 100 columns.
  1. Its path, and for well known folders, such as `~/Library/Caches` or `.npm`, what they hold in plain words.
  2. Its size and share of the current folder, its item count, and when it last changed.
  3. Whether neet cleans it: a green `✓` inside a folder neet cleans, a yellow `◆` for a cleanup folder whose items neet cleans, a red `✗` for a protected folder, and a normal foreground `·` for anywhere else.
  4. For a folder, what is inside, in the same order and colors as the left.
* Until the scan finishes, the screen shows the loading box, with how many items and how much space the scan has counted.
* The selected row is bold, with an arrow in front.
* Disk only shows what is there. Cleaning happens in Quick Clean and Deep Clean.
* The screen shows the scan as it was. An item moved to the Trash stays listed until the next scan.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Right`, `Enter`, or `l` | Open the selected folder. |
| `Left` or `h` | Go up to the parent folder, keeping your place. |
| `s` | Sort by size, then name, then item count. |
| `g` / `G` | Jump to the first or last row. |

## Quick Clean

```text
+ Quick Clean -------------------------------------------+  + Largest ------------------------------------+
|   Item                        Size              Found  |  |   20.8 GB  ~/Documents/rust/slingshot/target |
|                                                Cleared |  |    5.4 GB  ~/Documents/rust/neet/target      |
|   Caches and logs          ~193 MB  ......        neet |  |    2.1 GB  ~/Documents/app/node_modules      |
| > Project build folders    31.5 GB  ######   26   neet |  |  956.2 MB  ~/Documents/web/node_modules      |
|   Installers in Downloads     none                neet |  |       ...                                    |
|   Trash                     4.4 MB  ......  2,195  you |  |                                              |
|   Docker disk image        11.0 GB  ###...     1   you |  |                                              |
|   Simulators               17.3 GB  ####..     2   you |  |                                              |
|   Temporary files           3.1 GB  #.....  6,033 macOS|  |                                              |
+------- neet can clear ~31.7 GB . you can free 28.3 GB -+  |                                              |
+ Project build folders ---------------------------------+  |                                              |
| Cleared by neet                                        |  |                                              |
| node_modules folders, and Rust target folders, ...     |  |                                              |
| How                                                    |  |                                              |
| 1. Press Enter to list every build folder.             |  |                                              |
| 2. All start selected. Clear any you are working in.   |  |                                              |
| 3. Review, confirm, and they go to the Trash.          |  |                                              |
+--------------------------------------------------------+  +----------------------------------------------+
```

* One table of everything taking space that can be cleared, so you can start with the biggest wins.
* **Rows,** always in this order, what neet clears first:
  1. Caches and logs: what every Deep Clean rule found.
  2. Project build folders and installers in Downloads, from the scan. neet moves these to the Trash after you pick and review them.
  3. The Trash, from the scan, which you empty yourself.
  4. The Docker disk image, from the scan, and simulators, asked of `xcrun simctl` when there are simulators on the Mac: runtimes, and simulators left without one. neet asks their own tools to remove them, which is permanent.
  5. Temporary files, measured in `/private/var/folders`, which macOS clears. Simulators and temporary files are worked out in the background and show moonlight blue `scanning` until done.
* **Columns:** the size and a bar of it against the largest row, both red from 5 GB and yellow from 1 GB, how many items, and who clears it: `neet` in green, `neet, permanently` in red, `you` in yellow, or `macOS` in moonlight blue. Temporary files are left to macOS, since deleting them by hand can break running apps.
* The bottom edge adds up what neet can clear and what you can free yourself.
* **Below the table:** numbered actions for the selected row, followed by its description when space allows.
* **Right:** the largest items of the selected row, as many as fit: rules for Caches and logs, folders and installers by path, and the largest item for the Trash and Docker. When there is no list, the reason sits in the middle of the box. It is hidden when the terminal is narrower than 130 columns.
* Build folders and installers that are empty, or already gone since the scan, are left out.
* Below 120 columns, the selected path and age sit below the file table when at least 18 rows are available.
* Until the scan finishes, the screen shows the loading box.
* Nothing on this screen changes a file.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `g` / `G` | Jump to the first or last row. |
| `Enter`, `Right`, or `l` | On Caches and logs, open [Deep Clean](#deep-clean). On build folders or installers, open [Pick](#pick). On Docker or simulator runtimes, open [Docker And Simulators](#docker-and-simulators). On the Trash, show it in [Disk](#disk). |
| `d` | Show the largest item of the row in [Disk](#disk). |

### Pick

```text
+ Project build folders ----------------------------------------------------------------+
|             Size  Path                                                     Changed      |
|                                                                                         |
| > [✓]    20.8 GB  ~/Documents/rust/slingshot/target                       today        |
|   [✓]     2.1 GB  ~/Documents/app/node_modules                            14 days ago  |
|   [ ]   956.2 MB  ~/Documents/web/node_modules                            2 months ago |
+ Selected: 2 items . 22.9 GB --------------- Everything goes to the Trash, where Put Back works +
```

* Lists every build folder, or every installer, largest first, each with a checkbox, its size, its path with the folder in blue, and when anything inside last changed. A change in the last 7 days is yellow, since you may be working in it.
* Everything starts selected. Measuring and checking each item shows the loading box first.
* An item the check refuses remains readable, with the reason in yellow. What is allowed is in [SAFETY.md](SAFETY.md#build-folders-and-installers).

| Key | Action |
| --- | --- |
| `Space` | Select or clear an item. |
| `a` | Select all, or clear all when all are selected. |
| `Enter` | Go to [Review](#review-confirm-and-move). |

### Docker And Simulators

* These cannot go to the Trash, so neet asks their own tool to remove them. Removing ends in a red question that lists what goes and says it is permanent. Only `y` goes ahead. See [SAFETY.md](SAFETY.md#tools-neet-runs).
* **Simulators:** a table of each runtime with a checkbox, its version, build, size, the day a simulator last used it, and how many simulators run on it. Simulators left without a runtime share one yellow `No runtime` row. None start selected. `Enter` asks, then removes each runtime with `xcrun simctl`, then the simulators on it, and the `No runtime` simulators if selected.
* **Docker:** a table of what `docker system df` reports: images, containers, volumes, and build cache, with how many, how many are in use, their size, and how much can be freed, in yellow. Below it, the volumes no container uses, none selected, which `Space` adds to the prune. The box below that says, in colored labels, what `docker system prune --all` removes, what it keeps, and how to reset. `Enter` asks, then runs it and removes the selected volumes.
* **Reset Docker:** `x` asks, in a yellow box, to stop Docker Desktop and move its whole disk image to the Trash. See [Resetting Docker](SAFETY.md#resetting-docker).
* If Docker Desktop is not running, a small box in the middle says so, with numbered steps: `o` opens it, then `r` asks Docker again. `x` resets Docker without opening it.
* When done, a small box in the middle ticks off what went and how much space it freed.

| Key | Action |
| --- | --- |
| `↑` `↓` | Move between runtimes, or between Docker's unused volumes. |
| `Space` | Select or clear a runtime, the `No runtime` row, or a volume. |
| `Enter` | Ask before removing, or before the prune. |
| `x` | On Docker, ask before resetting it to the Trash. |
| `o` | On Docker, open Docker Desktop. |
| `r` | On Docker, ask Docker again. |
| `y` | In the question, go ahead. |
| `n` or `Esc` | In the question, go back. |
| `o` / `r` | On Docker, open Docker Desktop, or ask again. |

## Deep Clean

```text
+ Deep Clean ------------------------------------------+  + Homebrew downloads --------------------+
|        Rule                Risk     Items    Size    |  | Installers and bottles Homebrew        |
|                                                      |  | downloaded. Installed programs stay.   |
|   [✓]  npm cache           caution        3  4.7 GB  |  | Risk    caution  You may need to ...   |
| > [✓]  Homebrew downloads  caution       14  1.2 GB  |  | Folder  ~/Library/Caches/Homebrew      |
|   [ ]  User logs           caution       27 83.0 MB  |  +----------------------------------------+
|        Yarn cache          caution     none       .  |  + Found . 14 items . 1.2 GB -----------+
+ 10 of 14 rules found 8.8 GB -------------------------+  |    1.1 GB  ##########  downloads       |
+ Selected --------------------------------------------+  |   50.8 MB  ..........  bootsnap        |
|    4.7 GB  npm cache                                 |  +------------- Items go to the Trash ----+
|    1.2 GB  Homebrew downloads                        |  + Skipped . 179 left in place ---------+
|                                                      |  | 179 paths  a link, left in place       |
|    6.0 GB  total, in 17 items                        |  +----------------------------------------+
|                                                      |  + Where the space is -------------------+
| Press Enter to see every path first.                 |  |    4.7 GB  ##########  npm cache       |
|                                                      |  |    1.2 GB  ###.......  Homebrew ...    |
|                                                      |  |    8.8 GB  in all . 6.0 GB selected    |
+------------------------------------------------------+  +----------------------------------------+
```

* neet looks for everything the rules cover once, in the background, as soon as it opens. Deep Clean opens on that result, so going back and opening it again does not look again. If Deep Clean opens before the look is done, the loading box shows a timer. Nothing changes while it looks.
* neet looks again after a cleanup, and when you press `r`, such as after removing files yourself.
* **Left, top, 55% of the width:** a table of rules, largest first, with a checkbox, the rule's name, its risk level, how many items it found, and their size. Rules that found nothing are listed last with no checkbox. The bottom edge shows what every rule found together.
* **Left, bottom:** the Selected box. Each selected rule with its size, then the total. Before anything is selected, it says how selecting works.
* **Right:** the rule the arrow is on, in boxes that fit what they hold:
  1. **Top,** titled with the rule's name: what it removes, its risk level and what that means on one line, apps to close first, how recent files it keeps, and the folder it looks in, in blue.
  2. **Found:** every path it found, largest first, with its size and a bar against the largest. When they share a folder, only their names are shown.
  3. **Skipped:** only when something was skipped. Every path with the reason. A reason shared by more than two paths is shown once, with how many. It takes at most a third of the height.
  4. **Where the space is:** every rule that found something, largest first, with a bar, green when selected. The rule the arrow is on is bold. The last line adds up everything found and what is selected. It takes whatever room is left, and is left out when there is none.
  5. If a list is too long, its last line says how many more there are.
* When the terminal is narrower than 100 columns, the details go under the list, and the list's bottom edge shows the selected total instead.
* A selected checkbox is a green `[✓]`. The arrow's row shows its name in bold, so the checkbox and risk keep their colors.
* A note at the top of the details explains when a rule was not selected, or could not be. Rules that failed to load are listed there too.
* `safe` rules start selected, unless their app is open.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between rules. |
| `g` / `G` | Jump to the first or last rule. |
| `Space` | Select or clear the rule. Does nothing if it found nothing, or if its app is open. |
| `Enter` | Review the selected rules. Does nothing when none are selected. |
| `r` | Look again. Your selection starts over. |

* **Selecting an `expert` rule** opens a box that asks you to type the rule's ID. `Enter` checks it. A wrong ID selects nothing. `Esc` closes the box. While the box is open, every key but `Esc` is typed into it.

## Review, Confirm, And Move

* Deep Clean, Pick, and Remove App all end in these same four steps.

| Step | Layout | Keys |
| --- | --- | --- |
| Review | A scrolling list of every selected path, with sizes. Paths are grouped by rule, or by folder for Remove App, and rules that found nothing are left out. The title shows the item count and total size. From 100 columns, a Summary box beside it shows the total, a bar for each group, and what happens next. | Scroll with `Up` / `Down`, `PgUp` / `PgDn`, `g` / `G`. `Enter` goes on. `Esc` goes back. |
| Confirm | A box, 60 columns wide, in the middle of the review. It shows the item count and total size, and that Finder moves them to the Trash. | `y` moves them. `n` or `Esc` goes back. |
| Move | The loading box in the middle of the screen, with a progress bar and how many items have moved. The first time, macOS asks whether your terminal may control Finder. | No key works until every item is done. |
| Result | A box in the middle of the screen, up to 72 columns wide. A green `✓` line says how many items moved, then the space they take in the Trash, and how to use Put Back. Every skipped item follows, with its reason below it. If nothing moved, the first line is yellow. | Scroll a long result with `Up` / `Down`. `Enter` or `Esc` goes back to Home. |

## Large Files

```text
+ Large Files ---------------------------------------------------------+  + Docker.raw ------------------------------+
| At least    10 MB   50 MB  [100 MB]  500 MB   1 GB   5 GB  s to change |  | Folder   ~/Library/.../com.docker.docker  |
| Unchanged  [any age]  30 days  3 months  6 months  1 year  a to change |  | Size     11.0 GB                          |
+----------------------------------------------------------------------+  | Share    59% of the files found           |
+ Files, largest first ------------------------------------------------+  | Changed  28 days ago                      |
|        Size            Last changed   Name          Folder           |  | Type     Virtual disk                     |
|                                                                      |  |                                           |
| >   11.0 GB  ########  28 days ago    Docker.raw    ~/Library/...    |  | Used by a virtual machine or Docker. Free |
|      4.3 GB  ###.....  2 months ago   weights.bin   ~/Library/...    |  | it from the app that made it.             |
|      3.1 GB  ##......  6 months ago   Xcode_16.dmg  ~/Downloads      |  +------------------ Enter shows it in Disk +
|                                                                      |  + Where they are --------------------------+
|                                                                      |  |   11.0 GB ###### ~/Library/Containers     |
+------------------------------------------- 3 files . 18.4 GB in all -+  |    3.1 GB ##.... ~/Downloads              |
                                                                          +-------------------------------------------+
```

* Large Files only shows files. Nothing is moved from here: `Enter` shows the file in Disk.
* **Top:** both filters, with every choice listed and the current one green in brackets. Below 100 columns of panel width, show only the active size and age with `s` and `a` to change them. Starts at 100 MB and any age.
* **Files, largest first:** a table of files from the scan. Folders are not listed.
  1. Its size, and a bar of it against the largest file, both red from 5 GB and yellow from 1 GB.
  2. When it last changed. Files unchanged for a year or more have a magenta date.
  3. Its name and folder. A long name is shortened in the middle, so its extension shows. A long folder keeps its start, such as `~/Library`, and its last folders.
  4. The bottom edge shows how many files match and their total size. Up to 1,000 are listed, and the edge says when only the largest are shown.
  5. When no file matches, the box says so in the middle, with the keys to widen the filters.
* **Right, from 120 columns:** the selected file, titled with its name: its folder, size, share of the files found, when it changed, its type from the extension, and a plain hint for common types, such as installers, archives, videos, and virtual disks. Below it, **Where they are** adds the files up by folder, largest first. `~/Library` is split one level further, since most large files are there.
* Below 120 columns, the selected path and age sit below the file table when at least 18 rows are available.
* Until the scan finishes, the screen shows the loading box.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `g` / `G` | Jump to the first or last file. |
| `s` | Change the smallest size: 10 MB, 50 MB, 100 MB, 500 MB, 1 GB, 5 GB. |
| `a` | Change how long files must be unchanged: any age, 30 days, 3 months, 6 months, 1 year, 2 years. |
| `Enter`, `Right`, or `l` | Show the file in Disk. `Esc` comes back. |

## Remove App

```text
+ Remove App --------------------------------------------------------------------------------+
|   Name             Size                Folder          Bundle ID                            |
|                                                                                            |
| > Discord      500.4 MB  ##..........  /Applications   com.hnc.Discord                      |
|   Docker         2.6 GB  #######.....  /Applications   com.docker.docker                    |
|   Slingshot    585.7 KB  ............  ~/Applications  dev.slingshot.menubar                |
|   Xcode          4.3 GB                /Applications   not removable: Apple app             |
+ 15 apps you can remove take 9.1 GB ---------------------------- By name . s sorts by size +

+ Discord (com.hnc.Discord) -----------------------------------------------------------------+
|             Size  Name                   Folder                          Note              |
|                                                                                            |
| > [✓]   500.4 MB  Discord.app            /Applications                                     |
|   [✓]     1.1 MB  com.hnc.Discord        ~/Library/Caches                                  |
|   [ ]     4.1 KB  com.hnc.Discord.plist  ~/Library/Preferences           settings          |
|   [ ]     1.1 GB  discord                ~/Library/Application Support   matched by name   |
+ Selected: 2 items . 501.5 MB ----------------- Items go to the Trash, where Put Back works +
+--------------------------------------------------------------------------------------------+
| The app itself.                                                                            |
+--------------------------------------------------------------------------------------------+
```

* **App list:** a table of each app in `/Applications` and `~/Applications`, with its name, its size, a bar of its size against the largest app, its folder, and its bundle ID (the name macOS uses to identify it). Apps neet can remove come first. Apps it will not remove are listed after them, with a yellow refusal reason, such as `Apple app` or `link`.
* **Colors:** sizes and bars are red from 5 GB and yellow from 1 GB, and bars are green below that. `/Applications` and bundle IDs use moonlight blue; `~/Applications` is magenta.
* Sizes are measured in the background, one app after another, and show `…` until then. Press `s` to list the largest first, and again to go back to names.
* The bottom edge shows how many apps can be removed and how much space they take, with the sort order or a note on the top edge. A Selected box keeps the current app’s path and bundle ID or refusal visible.
* **App files:** opening an app finds and measures its files, with the loading box and a timer. Then a table lists the app and each file, with a checkbox, size, name, folder, and a note on anything left unselected, such as `may be your data`.
* The box under the table shows the bundle ID and selected path, then explains the file the arrow is on: why it starts selected or not, and what to check first.
* The bottom edge shows the selected total, in green once something is selected.
* What is found, and what starts selected, is in [SAFETY.md](SAFETY.md#app-removal).

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` | On the app list, open the app. On the file list, go to [Review](#review-confirm-and-move). |
| `Space` | On the file list, select or clear a file. |
| `s` | On the app list, sort by size or by name. |

* An app that is open cannot be opened here. The note says to quit it first.

## Planned Screens

* Draft layouts for later features. They may change before they are built.

| Screen | Layout | Keys |
| --- | --- | --- |
| Space Breakdown | A list of parts that add up to the disk's used space: the scan, skipped folders, apps, macOS, snapshots, purgeable space, and anything unexplained. Opened with `b` on Home. | `Enter` opens a folder in Disk. |
| Projects | A list of build folders in code projects, with project, last change, folder, and size. Folders that cannot be cleaned show the reason. | `Space` selects, `s` sorts, `Enter` reviews. |
| Startup | Sections for login items, background items, launch agents, and launch daemons. Each row shows the program, whether it runs, and whether it is signed. | Turn off, turn back on. |
| SSH | Sections for hosts, keys, agent keys, known hosts, and permission problems. | Fix permissions, add or remove agent keys, remove known hosts. |
| PATH | Each folder in search order, with how many programs it holds, where it was added, and any problem. A preview shows the programs it holds. | `w` finds which copy of a program runs. `K` / `J` move a folder. `a` adds, `x` removes, `Enter` saves with a backup. |
| AI Tools | Sections for tools, settings, instruction files, skills, and project skills. View only. | `Enter` opens a file. |
| Settings | Sections for power mode, graphics switching, refresh rate, Game Mode, what keeps the Mac awake, and wake settings. | Each change shows what it will do first, and can be undone. |

### Dotfiles

* A draft. The rules it follows are in [SAFETY.md](SAFETY.md#dotfiles).

```text
+ Dotfiles ----------------------------------------- chezmoi . you/dotfiles . 2 to commit . up to date +
+ Files -------------------------------------------------+  + .zprofile -------------------------------+
|        Size    Changed       Status                    |  | Path     ~/.zprofile                     |
| Shell                                                  |  | Source   dot_zprofile                    |
|   .zshrc       895 B   12 days ago   in sync           |  | Changed  2 days ago                      |
| > .zprofile    193 B   2 days ago    changed in home   |  | Check    zsh -n  passed                  |
|   .zshenv       89 B   12 days ago   in sync           |  | Backups  3                               |
| Git                                                    |  |                                          |
|   .gitconfig   284 B   8 days ago    in sync           |  | Changed in home since chezmoi            |
|   git/ignore    41 B   1 month ago   in sync           |  | wrote it. r keeps it, p puts the         |
| SSH                                                    |  | source back.                             |
|   config       312 B   3 months ago  view only         |  +------------------------------------------+
| Terminal                                               |  + Preview ---------------------------------+
|   kitty.conf   2.1 KB  1 month ago   in sync           |  | eval "$(/opt/homebrew/bin/brew shell...  |
|   starship     1.4 KB  1 month ago   in sync           |  | export EDITOR=nvim                       |
| Tools                                                  |  |                                          |
|   .npmrc       112 B   1 month ago   may hold secrets  |  |                                          |
+--------------------------------------- 11 files found -+  +------------------------------------------+
```

* **Top edge:** whether chezmoi is in use, the repository, how many files wait to be committed, and whether it is ahead of or behind the remote, as last fetched. Without chezmoi it says so, and `x` offers to start a repository.
* **Files:** only the files that exist, grouped by shell, Git, SSH, editors, terminal, and tools. `.` shows the missing ones too, so you can create one. Long paths keep their file name.
  1. Size and when it last changed.
  2. Status: **in sync** in green, **changed in home** or **changed in source** in yellow, **not in chezmoi**, **view only** with the reason in the details, and **may hold secrets** in red.
* **Right:** the selected file: its path, its source file in chezmoi, when it changed, its check and whether the file passes it now, and how many backups it has. Below, a note on its status and the keys that fix it. Under that, a preview of the start of the file.
* Below 120 columns, the details sit under the list, and the preview is hidden.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `e` | Edit a copy in your editor. When it closes: the check, the diff, then `y` writes it. |
| `c` | Configure the selected program's settings, one at a time. |
| `d` | Show the diff between the file and its source file, or its latest backup. |
| `b` | List its backups, newest first. `Enter` shows the diff, then `y` restores it. |
| `r` / `p` | When changed in home: `r` keeps the home version in chezmoi, `p` puts the source version back. |
| `a` | Let chezmoi manage the file. |
| `x` | Export: review for secrets, commit, then push. |
| `.` | Show or hide missing files. |

#### Configure

```text
+ Git settings . ~/.gitconfig -----------------------------------------------+
|   Setting              Value                About                          |
| > user.name            Your Name            Name on your commits.          |
|   user.email           you@example.com      Email on your commits.         |
|   init.defaultBranch   master               Branch name for new repos.     |
|   core.editor          not set              Editor for commit messages.    |
|   pull.rebase          false                Rebase instead of merge.       |
+-------------------------------------------------- Enter changes . y saves -+
+ Change --------------------------------------------------------------------+
|   [init]                                                                   |
| -     defaultBranch = master                                               |
| +     defaultBranch = main                                                 |
+----------------------------------------------------------------------------+
```

* Each setting shows its value now, or **not set**, and what it does. `Enter` changes it: on and off flip, choices cycle, text opens a box to type in.
* **Change** shows the diff of everything changed so far. `y` saves it all as one change, with one backup. `Esc` drops it.

#### Export

```text
+ Export . you/dotfiles -----------------------------------------------------+
| Review for secrets                                                         |
|   ok    .zshrc, .zprofile, .gitconfig, kitty.conf, starship.toml           |
|   !     .npmrc  line 2  //registry.npmjs.org/:_authToken=npm_****          |
|                                                                            |
| Commit  2 files . "Update zprofile and gitconfig"                          |
| Push    origin/master . 1 commit                                           |
+-------------------------------------- Space leaves out . m message . y go -+
```

* **Review for secrets** comes first. Each match shows its file, line, and the start of its value. `Space` leaves the file out, `e` edits it.
* **Commit** lists the files and the message. `m` changes the message.
* **Push** names the remote and branch. `y` commits, then asks again in its own box before pushing. Without a remote, that box offers a new private GitHub repository.
