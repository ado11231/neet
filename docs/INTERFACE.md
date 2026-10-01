# Interface

* How each screen is laid out, what it shows, and which keys it uses.
* For anyone using neet, or changing a screen.
* For what each feature does, read [FEATURES.md](FEATURES.md).

## Contents

1. [Every Screen](#every-screen)
2. [Home](#home)
3. [Skipped](#skipped)
4. [Disk](#disk)
5. [Clean](#clean)
6. [Review, Confirm, And Move](#review-confirm-and-move)
7. [Large Files](#large-files)
8. [Remove App](#remove-app)
9. [Planned Screens](#planned-screens)

## Every Screen

* Run `neet` to open it. It opens on Home and starts scanning your home folder in the background.
* Each screen fills the terminal. The bottom row always lists the keys for the current screen.
* Screens stack: each one opens on top of the last, and `Esc` goes back one step.
* Boxes, such as help and questions, open in the middle of the screen, on top of it.
* The screen redraws about four times a second, so progress stays current.
* While a screen waits for slow work, such as the scan, it shows a small box in the middle: what neet is doing, a spinner, how far it has got, and what shows when it is done.

| Key | Action |
| --- | --- |
| Arrow keys, or `h` `j` `k` `l` | Move. |
| `Esc` | Go back one step. |
| `?` | Show the keys for this screen. |
| `q` | Quit. Does nothing while a question is open or files are moving. |

## Home

```text
+--------------------------------+  +- neet ------------------------------+
|                                |  | > 1 Disk          412.2 GB used     |
|                                |  |   2 Clean         ~8.8 GB found     |
|                                |  |   3 Large Files                     |
|              Art               |  |   4 Remove App                      |
|                                |  |   5 Startup       soon              |
|                                |  |     ...                             |
|                                |  |     Quit                            |
|                                |  +-------------------------------------+
|                                |  +-------------------------------------+
|                                |  | Disk                                |
|                                |  | Browse your folders by size.        |
|                                |  | [########......] 83% used           |
|                                |  | 82.0 GB free of 494.4 GB            |
|                                |  | Scanning your home folder...        |
+--------------------------------+  +-------------------------------------+
 up/down move . enter open . s skipped . ? help . q quit
```

* **Left:** the neet art. It takes about 45% of the width, and is hidden when the terminal is narrower than 90 columns.
* **Right, top:** the menu, one numbered row per feature.
  1. Disk shows how much of the disk is used.
  2. Clean shows `finding...`, then the total that every rule found. This is worked out again after each cleanup.
  3. Features not built yet are dimmed and marked `soon`. The selection skips them.
* **Right, bottom:** details for the selected row:
  1. What the feature does.
  2. A 16 character gauge of how full the disk is, then free and total space on the next line. The gauge turns yellow at 75% and red at 90%.
  3. When macOS can clear 100 MB or more on its own, a line says how much free space Finder shows, counting that purgeable space. It is read about once a minute.
  4. Scan progress, then the home folder's total size. If some folders could not be read, the size is marked `at least`, and a yellow line says how many were blocked by macOS or could not be read, and to press `s`.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` or `Right` | Open the selected screen. |
| `1` to `9` | Open that row. |
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

* **Left, 55% of the width:** the current folder, largest first. Each row shows a 12 character bar and a percent for its share of the folder, its size, and its name. Folders end in `/`, links in `@`.
* Sizes and bars of 5 GB or more are red, and of 1 GB or more yellow, as in Large Files.
* The title shows the folder's path on the left, and its size, item count, and sort order on the right.
* **Right:** the selected row, titled with its name in cyan. The preview is hidden when the terminal is narrower than 100 columns.
  1. Its path, and for well known folders, such as `~/Library/Caches` or `.npm`, what they hold in plain words.
  2. Its size and share of the current folder, its item count, and when it last changed.
  3. Whether `d` can clean it: a green `✓` when it can, a yellow `◆` for a cleanup folder whose items can be cleaned, a red `✗` for a protected folder, and a gray `·` for anywhere else.
  4. For a folder, what is inside, in the same order and colors as the left.
* Until the scan finishes, the screen shows the loading box, with how many items and how much space the scan has counted.
* The selected row is bold cyan, with an arrow in front.
* The screen shows the scan as it was. An item moved to the Trash stays listed until the next scan.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Right`, `Enter`, or `l` | Open the selected folder. |
| `Left` or `h` | Go up to the parent folder, keeping your place. |
| `s` | Sort by size, then name, then item count. |
| `g` / `G` | Jump to the first or last row. |
| `d` | Clean the selected item. Opens [Review](#review-confirm-and-move), or a box that says in plain sentences why neet will not move it. Any key closes the box. |

## Clean

```text
+ Clean -----------------------------------------------+  + Homebrew downloads --------------------+
|        Rule                Risk     Found    Size    |  | Installers and bottles Homebrew        |
|                                                      |  | downloaded. Installed programs stay.   |
|   [✓]  npm cache           caution  3 items  4.7 GB  |  |                                        |
| > [✓]  Homebrew downloads  caution 14 items  1.2 GB  |  | Risk    caution                        |
|   [ ]  User logs           caution 27 items 83.0 MB  |  | You may need to download again.        |
|        Yarn cache          caution     none       .  |  |                                        |
+ 10 of 14 rules found 8.8 GB -------------------------+  | Folder  ~/Library/Caches/Homebrew      |
+ Selected --------------------------------------------+  |                                        |
|    4.7 GB  npm cache                                 |  | Found 14 items . 1.2 GB                |
|    1.2 GB  Homebrew downloads                        |  |    1.1 GB  downloads                   |
|                                                      |  |   50.8 MB  bootsnap                    |
|    6.0 GB  total, in 17 items                        |  |                                        |
|                                                      |  | Skipped 179 left in place              |
| Press Enter to see every path first.                 |  | 179 paths  a link, left in place       |
+------------------------------------------------------+  + Put Back works ------------------------+
```

* When Clean opens, it looks for everything the rules cover. This takes a few seconds, and the loading box shows a timer. Nothing changes while it looks.
* **Left, top, 55% of the width:** a table of rules, largest first, with a checkbox, the rule's name, its risk level, how many items it found, and their size. Rules that found nothing are listed last, dimmed, with no checkbox. The bottom edge shows what every rule found together.
* **Left, bottom:** the Selected box. Each selected rule with its size, then the total. Before anything is selected, it says how selecting works.
* **Right:** the rule the arrow is on:
  1. What it removes.
  2. Its risk level and what that means.
  3. Apps to close first, how recent files it keeps, and the folder it looks in, when every path is in one folder.
  4. Every path it found, largest first. When they share a folder, only their names are shown.
  5. Every path it skipped, with the reason. A reason shared by more than two paths is shown once, with how many.
  6. If a list is too long, its last line says how many more there are.
* When the terminal is narrower than 100 columns, the details go under the list, and the list's bottom edge shows the selected total instead.
* A selected checkbox is a green `[✓]`. The arrow's row shows its name in cyan, so the checkbox and risk keep their colors.
* A note at the top of the details explains when a rule was not selected, or could not be. Rules that failed to load are listed there too.
* `safe` rules start selected, unless their app is open.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between rules. |
| `g` / `G` | Jump to the first or last rule. |
| `Space` | Select or clear the rule. Does nothing if it found nothing, or if its app is open. |
| `Enter` | Review the selected rules. Does nothing when none are selected. |

* **Selecting an `expert` rule** opens a box that asks you to type the rule's ID. `Enter` checks it. A wrong ID selects nothing. `Esc` closes the box. While the box is open, every key but `Esc` is typed into it.

## Review, Confirm, And Move

* Clean, Disk, Large Files, and Remove App all end in these same four steps.

| Step | Layout | Keys |
| --- | --- | --- |
| Review | A scrolling list of every selected path, with sizes. Paths are grouped by rule, or by folder for Remove App, and rules that found nothing are left out. The title shows the item count and total size. From 100 columns, a Summary box beside it shows the total, a bar for each group, and what happens next. | Scroll with `Up` / `Down`, `PgUp` / `PgDn`, `g` / `G`. `Enter` goes on. `Esc` goes back. |
| Confirm | A box, 60 columns wide, in the middle of the review. It shows the item count and total size, and that Finder moves them to the Trash. | `y` moves them. `n` or `Esc` goes back. |
| Move | A progress bar. The first time, macOS asks whether your terminal may control Finder. | No key works until every item is done. |
| Result | How many items moved, the space they take in the Trash, how to use Put Back, and every skipped item with its reason. | `Enter` or `Esc` goes back to Home. |

## Large Files

```text
+ Large Files: 109 files of 100.0 MB or more, 39.5 GB in all -------------------+
|        Size  Last Changed   Name              Folder                          |
|                                                                               |
| >   11.0 GB  28 days ago    Docker.raw        ~/Library/.../Data/vms/0/data   |
|      4.3 GB  2 months ago   weights.bin       ~/Library/.../2025.8.8.1141     |
|    457.4 MB  1 year ago     fca1ae...tar.gz   ~/Library/Caches/Homebrew       |
+-------------------------------------------------------------------------------+
```

* One full screen table of files from the scan, largest first. Folders are not listed.
* **Columns:** size, when the file last changed, its name, and the folder it is in.
  1. Sizes of 5 GB or more are red, and 1 GB or more yellow.
  2. Files changed in the last 30 days have a dimmed date. Files unchanged for a year or more have a magenta date.
  3. A long name is shortened in the middle, so its extension shows. A long folder keeps its start, such as `~/Library`, and its last folders.
* The title shows the filters, how many files match, and their total size.
* Starts at 100 MB and any age. Lists up to 1,000 files, and the bottom edge says how many more match.
* Until the scan finishes, the screen shows the loading box.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `g` / `G` | Jump to the first or last file. |
| `s` | Change the smallest size: 10 MB, 50 MB, 100 MB, 500 MB, 1 GB, 5 GB. |
| `a` | Change how long files must be unchanged: any age, 30 days, 3 months, 6 months, 1 year, 2 years. |
| `Enter`, `Right`, or `l` | Show the file in Disk. `Esc` comes back. |
| `d` | Clean the file, as in Disk. Most files are refused, with the reason. |

## Remove App

```text
+ Remove App: 15 apps you can remove --------------------------------------------------------+
|   Name        Bundle ID               Folder                                               |
|                                                                                            |
| > Discord     com.hnc.Discord         /Applications                                        |
|   Slingshot   dev.slingshot.menubar   ~/Applications                                       |
|   Xcode       com.apple.dt.Xcode      /Applications    not removable: Apple app            |
+ Only apps directly in /Applications and ~/Applications are listed -------------------------+

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

* **App list:** a table of each app in `/Applications` and `~/Applications`, with its name, its bundle ID (the name macOS uses to identify it), and its folder. Apps neet can remove come first. Apps it will not remove are listed after them, dimmed, with the reason, such as `Apple app` or `link`.
* The title shows how many apps can be removed. The bottom edge shows a note, such as why an app cannot be opened.
* **App files:** opening an app finds and measures its files, with the loading box and a timer. Then a table lists the app and each file, with a checkbox, size, name, folder, and a note on anything left unselected, such as `may be your data`.
* The box under the table explains the file the arrow is on: why it starts selected or not, and what to check first.
* The bottom edge shows the selected total, in green once something is selected.
* What is found, and what starts selected, is in [SAFETY.md](SAFETY.md#app-removal).

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` | On the app list, open the app. On the file list, go to [Review](#review-confirm-and-move). |
| `Space` | On the file list, select or clear a file. |

* An app that is open cannot be opened here. The note says to quit it first.

## Planned Screens

* Draft layouts for later features. They may change before they are built.

| Screen | Layout | Keys |
| --- | --- | --- |
| Space Breakdown | A list of parts that add up to the disk's used space: the scan, skipped folders, apps, macOS, snapshots, purgeable space, and anything unexplained. Opened with `b` on Home. | `Enter` opens a folder in Disk. |
| Projects | A list of build folders in code projects, with project, last change, folder, and size. Folders that cannot be cleaned are dimmed with the reason. | `Space` selects, `s` sorts, `Enter` reviews. |
| Startup | Sections for login items, background items, launch agents, and launch daemons. Each row shows the program, whether it runs, and whether it is signed. | Turn off, turn back on. |
| SSH | Sections for hosts, keys, agent keys, known hosts, and permission problems. | Fix permissions, add or remove agent keys, remove known hosts. |
| Dotfiles | Settings files grouped by shell, Git, SSH, editors, and terminal. | Edit with a backup, export. |
| PATH | Each folder in search order, with how many programs it holds, where it was added, and any problem. A preview shows the programs it holds. | `w` finds which copy of a program runs. `K` / `J` move a folder. `a` adds, `x` removes, `Enter` saves with a backup. |
| AI Tools | Sections for tools, settings, instruction files, skills, and project skills. View only. | `Enter` opens a file. |
| Settings | Sections for power mode, graphics switching, refresh rate, Game Mode, what keeps the Mac awake, and wake settings. | Each change shows what it will do first, and can be undone. |
