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
10. [Dotfiles](#dotfiles)
11. [Planned Screens](#planned-screens)

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
* While a loading box shows, the footer of keys is left out, since they do nothing until it is done.
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
  .     *      .        +       .        +- neet ------------------------------+
      .     .      *        .            | > 1 Quick Clean                     |
  *        .           .        *        |   2 Deep Clean                      |
       .        Art        .             |   ...                               |
   .       *          .          .       |   6 Startup       soon              |
       .        .        *       .       |     Quit                            |
  +        .        .        .           +-------------------------------------+
     .        *         .       *        + Quick Clean ------------------------+
  .      .        .        .        .    |                                     |
      *       .        +       .         | Find reclaimable space.             |
  .        .        *        .           |                                     |
     .        .         .       .        +-------------------------------------+
  .       .       .        .       *     + Disk -------------------------------+
     .        *        .       .         | ##################.......  64% used |
  *      .        .         .       .    | Free     88.1 GB of 245.1 GB        |
     .       .        *        .         | Used     157.0 GB                   |
  .      +        .        .        .    | Finder   96.2 GB free . 8.1 GB ...  |
      .       .        .       *         +-------------------------------------+
  *       .       *        .       .     + Home folder ------------------------+
     .        .       .        .         | Size     at least 58.8 GB           |
  .      *        .        +        .    | Items    714,329 . scanned in 8s    |
      .       .        .       .         | Blocked  147 paths . s shows them   |
  +       .       *        .       *     | Can free ~1.8 GB in caches and logs |
     .        .       .        .         +-------------------------------------+
 up/down move . enter open . s skipped . ? help . q quit
```

* **Left:** the neet art. Stars cover the whole screen evenly, corners and the space above and below the boxes included, and never sit between or right beside the boxes. It takes about 45% of the width, and more on a wide screen, since the right side is at most 72 columns. It is hidden when the terminal is narrower than 90 columns.
* **Right:** the menu and three boxes, each sized to its lines, a row apart, centred top to bottom like the art. When room is short, the rows between them go first, then the last boxes.
* **Menu:** one numbered row per feature.
  1. The rows go from the quickest way to free space to the most detailed: Quick Clean, Deep Clean, Remove App, Large Files, then Disk.
  2. Quick Clean comes first, and is selected when neet opens.
  3. Features not built yet remain readable and have a yellow `soon` label. The selection skips them. No other row has a note.
* **The selected row:** its name as the title, and what it does, with a blank row above and below, so it stands a little taller than the boxes under it.
* **Disk:** a gauge of how full the disk is, as wide as the box allows. It turns yellow at 75% and red at 90%, and the free space is green, yellow, or red to match. Then free, total, and used space. When macOS can clear 100 MB or more on its own, a last line says how much free space Finder shows, counting that purgeable space. It is read about once a minute.
* **Home folder:** scan progress, then the folder's total size and item count. If some folders could not be read, the size is marked `at least`, and yellow lines say how many were blocked by macOS or could not be read, and to press `s`. **Can free** shows `finding...`, then in green the total every Deep Clean rule found. It is worked out again after each cleanup, and when you press `r` in Deep Clean.

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
+ Quick Clean -------------------------------------------+  + Space ---------------------------------------+
|   Item                        Size              Found  |  | Largest items                                |
|                                                Cleared |  |   20.8 GB  ~/Documents/rust/slingshot/target |
|   Caches and logs          ~193 MB  ......        neet |  |    5.4 GB  ~/Documents/rust/neet/target      |
| > Project build folders    31.5 GB  ######   26   neet |  |                                              |
|   Installers in Downloads     none                neet |  | Overview                                     |
|   Trash                     4.4 MB  ......  2,195  you |  | Project build folders   31.5 GB  ##########  |
|   Docker disk image        11.0 GB  ###...     1   you |  | Simulators              17.3 GB  ######....  |
|   Simulators               17.3 GB  ####..     2   you |  | Docker disk image       11.0 GB  ####......  |
|   Temporary files           3.1 GB  #.....  6,033 macOS|  |                                              |
+------- neet can clear ~31.7 GB . you can free 28.3 GB -+  | After cleanup                                |
+ Project build folders ---------------------------------+  | ##########################====....  64% -> 51%|
| How                                                    |  | Free now   86.9 GB of 245.1 GB               |
| 1. Enter: list build folders.                          |  | Cleanup    ~31.7 GB by neet                  |
| ...                                                    |  | By hand    28.3 GB by you                    |
| About                                                  |  | Free after ~118.6 GB                         |
| Project node_modules and Rust target folders. ...      |  |                                              |
| Summary                                                |  |                                              |
| Size       31.5 GB . 26 items                          |  |                                              |
| Share      ######........ 52% of everything found      |  |                                              |
| Cleared by neet, to the Trash, after you review it     |  |                                              |
+--------------------------------------------------------+  +----------------------------------------------+
```

* One table of everything taking space that can be cleared, so you can start with the biggest wins.
* **Rows,** always in this order, what neet clears first:
  1. Caches and logs: what every Deep Clean rule found.
  2. Project build folders and installers in Downloads, from the scan. neet moves these to the Trash after you pick and review them.
  3. The Trash, from the scan, which you empty yourself. macOS only lets a terminal read `~/.Trash` with Full Disk Access. Without it, the row says `no access` in yellow instead of `none`, and its Summary says how to turn it on.
  4. The Docker disk image, from the scan, and simulators, asked of `xcrun simctl` when there are simulators on the Mac: runtimes, and simulators left without one. neet asks their own tools to remove them, which is permanent.
  5. Temporary files, measured in `/private/var/folders`, which macOS clears. Simulators and temporary files are worked out in the background and show moonlight blue `scanning` until done.
* **Columns:** the size and a bar of it against the largest row, both red from 5 GB and yellow from 1 GB, how many items, and who clears it: `neet` in green, `neet, permanently` in red, `you` in yellow, or `macOS` in moonlight blue. Temporary files are left to macOS, since deleting them by hand can break running apps.
* The bottom edge adds up what neet can clear and what you can free yourself.
* **The boxes run the full height.** Each side is a stack of small boxes, one for each section, titled with the section's name, such as **About** or **Overview**. The boxes share the height evenly, so a tall screen has no empty space below them. Boxes that do not fit are left out, the last first. From 45 rows, the table's rows stand a row apart.
* **Below the table,** titled with the selected row: **Steps**, its numbered steps; **About**; **Location**, its folders in blue; **Impact**, what changes, and **Undo**: Put Back in green, or none in red for what is deleted for good; and **Summary**: its size and item count, a bar of its share of everything found, and who clears it, in the table's color. On a short screen, Where it is and After it is cleared are left out first.
* **Space,** on the right: **Largest items**, as many as fit: rules for Caches and logs, folders and installers by path, and the largest item for the Trash and Docker, or why there is no list. **Overview**: every row that found something, largest first, with a bar in the color of who clears it. **After cleanup**: a gauge of the disk with the space the cleanup frees in green, then free space now, what neet can clear in green, what you clear by hand in yellow when there is any, and free space after. Space is hidden below 130 columns, and After cleanup moves to the bottom of the box under the table.
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

* Lists every build folder, or every installer, largest first, each with a checkbox, its size, its path with the folder in blue, and when anything inside last changed. A change in the last 7 days is yellow, since you may be working in it. The path column is only as wide as the longest path, so the age sits beside it, and the table is only as tall as its rows.
* **Below the table,** titled with the selected item's name, when there is room: its project or folder, size, when it changed, whether it is selected, and what it is, such as what `node_modules` or `target` holds and how it comes back. Then **Total**: what was found, what is selected, and how many items changed this week. **Steps**: the three steps. **Overview**: every item as a bar, green when selected. **After cleanup**: a gauge of the disk, free space now, what is selected, and free space after. Each is its own box. From 100 columns they sit in two columns: the item and Overview on the left, and Total, Steps, and After cleanup on the right.
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
* **Below the Simulators table,** titled with the selected runtime: its build, size, when it was last used, and the simulators on it by name, then **Warning: deleted for good, not moved to the Trash**, which every screen size keeps. Then **Removal**, **Total**: runtimes, simulators, and what is selected, in red, **Steps**: the three steps, and **After cleanup**: the disk now and after the selected runtimes go. Each is its own box. From 100 columns they sit in two columns: the runtime and Removal on the left, and the rest on the right.
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
|                                                      |  + Location -----------------------------+
| Press Enter to see every path first.                 |  |    4.7 GB  ##########  npm cache       |
|                                                      |  |    1.2 GB  ###.......  Homebrew ...    |
|                                                      |  |    8.8 GB  in all . 6.0 GB selected    |
+------------------------------------------------------+  +----------------------------------------+
```

* neet looks for everything the rules cover once, in the background, as soon as it opens. Deep Clean opens on that result, so going back and opening it again does not look again. If Deep Clean opens before the look is done, the loading box shows a timer. Nothing changes while it looks.
* neet looks again after a cleanup, and when you press `r`, such as after removing files yourself.
* **Left, top, 55% of the width:** a table of rules, largest first, with a checkbox, the rule's name, its risk level, how many items it found, and their size. Rules that found nothing are listed last with no checkbox. The bottom edge shows what every rule found together.
* **Left, bottom:** the Selected box. Each selected rule with its size, then the total. Before anything is selected, it says how selecting works. Below that, **Risk levels**, for the risks the rules use, and **Total**: everything found, what is selected, that `Enter` shows every path first, and that it goes to the Trash. Each is its own box, as in Quick Clean.
* **Right:** the rule the arrow is on, in boxes that fit what they hold:
  1. **Top,** titled with the rule's name: what it removes, its risk level and what that means on one line, apps to close first, how recent files it keeps, and the folder it looks in, in blue.
  2. **Found:** every path it found, largest first, with its size and a bar against the largest. When they share a folder, only their names are shown.
  3. **Skipped:** only when something was skipped. Every path with the reason. A reason shared by more than two paths is shown once, with how many. It takes at most a third of the height.
  4. **Location:** every rule that found something, largest first, with a bar, green when selected. The rule the arrow is on is bold. The last line adds up everything found and what is selected. Below it, when there is room, **Risk**: each risk the rules use, with the space its rules found, a bar of it against everything found in the risk's color, and how many of its rules found something; and **No findings**: the rules that looked and found nothing, by name. On a screen of 34 rows or more, every box on the right keeps its size whichever rule is selected: the rule's box is as tall as the longest rule's text, Location keeps the bottom 45% of the column, and the found box fills the rest, with what was skipped as its own section below what was found. Otherwise it takes whatever room is left, and is left out when there is none.
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
| Review | A scrolling list of every selected path, with its size and a bar against the largest. Paths are grouped by rule, or by folder for Remove App, and rules that found nothing are left out. The title shows the item count and total size. From 100 columns, small boxes beside it show the **Total** (size, items, groups, and that it goes to the Trash), an **Overview** bar for each group, the **Steps**, and **After cleanup**, the disk now and after. | Scroll with `Up` / `Down`, `PgUp` / `PgDn`, `g` / `G`. `Enter` goes on. `Esc` goes back. |
| Confirm | A box, 60 columns wide, in the middle of the review. It shows the item count and total size, and that Finder moves them to the Trash. | `y` moves them. `n` or `Esc` goes back. |
| Move | The loading box in the middle of the screen, with a progress bar and how many items have moved. The first time, macOS asks whether your terminal may control Finder. | No key works until every item is done. |
| Result | A box in the middle of the screen, up to 72 columns wide. A green `✓` line says how many items moved, then the space they take in the Trash, and how to use Put Back. Every skipped item follows, with its reason below it. If nothing moved, the first line is yellow. | Scroll a long result with `Up` / `Down`. `Enter` or `Esc` goes back to Home. |

## Large Files

```text
+ Large Files ---------------------------------------------------------+  + Docker.raw ------------------------------+
| At least    10 MB   50 MB  [100 MB]  500 MB   1 GB   5 GB  s to change |  | Folder   ~/Library/.../com.docker.docker  |
| Unchanged  [any age]  30 days  3 months  6 months  1 year  a to change |  | Size     11.0 GB                          |
+----------------------------------------------------------------------+  | Share    59% of the files found           |
+ Files ----------------------------------------------------------------+  | Changed  28 days ago                      |
|        Size            Last changed   Name          Folder           |  | Type     Virtual disk                     |
|                                                                      |  |                                           |
| >   11.0 GB  ########  28 days ago    Docker.raw    ~/Library/...    |  | Used by a virtual machine or Docker. Free |
|      4.3 GB  ###.....   2 months ago  weights.bin   ~/Library/...    |  | it from the app that made it.             |
|      3.1 GB  ##......   6 months ago  Xcode_16.dmg  ~/Downloads      |  +------------------ Enter shows it in Disk +
|                                                                      |  + Location ---------------------------------+
|                                                                      |  |   11.0 GB ###### ~/Library/Containers     |
|                                                                      |  |                                           |
|                                                                      |  |    3.1 GB ##.... ~/Downloads              |
|                                                                      |  |                                           |
|                                                                      |  | By type                                   |
|                                                                      |  |   11.0 GB ###### Virtual disk      1 file |
+------------------------------------------- 3 files . 18.4 GB in all -+  |    4.3 GB ##.... Data or model     1 file |
                                                                          +-------------------------------------------+
```

* Large Files only shows files. Nothing is moved from here: `Enter` shows the file in Disk.
* **Top:** both filters, with every choice listed and the current one green in brackets. Below 100 columns of panel width, show only the active size and age with `s` and `a` to change them. Starts at 100 MB and any age.
* **Files:** a table of files from the scan. Folders are not listed.
  1. Its size, and a bar of it against the largest file, both red from 5 GB and yellow from 1 GB.
  2. When it last changed, such as ` 3 days ago`. The number is right aligned, so the ones line up with the ones of `39 days ago` and every unit starts in the same place. Files unchanged for a year or more have a magenta date.
  3. Its name and folder. The name column is only as wide as the longest name, up to 40 columns, and the folder gets the rest. A long name is shortened in the middle, so its extension shows. A long folder keeps its start, such as `~/Library`, and its last folders.
  4. The bottom edge shows how many files match and their total size. Up to 1,000 are listed, and the edge says when only the largest are shown.
  5. When no file matches, the box says so in the middle, with the keys to widen the filters.
* **Right, from 120 columns:** the selected file, titled with its name: its folder, size, share of the files found, when it changed, its type from the extension, and a plain hint for common types, such as installers, archives, videos, program code, caches, and virtual disks. Below it, **Location** adds the files up **Folders**, largest first, with `~/Library` split one level further, since most large files are there. Under the folders, when there is room, **Types** adds them up by type, with how many files of each. Files of no known type count as **Other**.
* Below 120 columns, a Selected box sits below the file table when at least 18 rows are available: the file's name, its folder, and when it changed and its type.
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

## Dotfiles

```text
 Dotfiles ----------------------------------------------------------------- 4 saved . 1 changed . 1 not saved
+ Files -----------------------------------------------------++ .gitconfig ----------------------------------+
|   Shell                                                    || Path     ~/.gitconfig                        |
|     .zshrc           ✓ saved                               || Saved as dot_gitconfig                       |
|     .zprofile        ✓ saved                               || Changed  today                               |
|   Git                                                      || Backups  0                                   |
| >   .gitconfig       ! changed                             ||                                              |
|   SSH                                                      || ! Changed since it was saved. Enter saves    |
|     .ssh/config      . view only                           || this version or uses the saved one.          |
|   Terminal                                                 |+----------------------------------------------+
|     .tmux.conf       + not saved                           |+ Preview -------------------------------------+
|     starship.toml    ✓ saved                               || [user]                                       |
|   Tools                              + .gitconfig ------------------+ = Home Version                      |
|     .npmrc           ✓ saved . secret| > Save my version        r   |                                     |
+--------------------------------------|   Use the saved version  p   |                                     |
+ Your dotfiles -----------------------|   Show the changes       d   |                                     |
| Repo     you/dotfiles . main         +------------------------------+                                     |
| Export   nothing waiting                                   ||                                              |
| Push     up to date                                        ||                                              |
| Folder   ~/.local/share/chezmoi                            ||                                              |
+------------------------------------------------------------++----------------------------------------------+
```

* What may change, and how, is in [SAFETY.md](SAFETY.md#dotfiles).
* "Saved" means saved in your dotfiles: chezmoi keeps a copy of the file, its source file, in a Git repository.
* **Top line:** how many files are saved, changed, and not saved. It says so when chezmoi is not found.
* **Files:** the listed files that exist, grouped by shell, Git, SSH, editors, terminal, and tools. `.` shows the missing ones too. Files in `~/.config` are shown without it. Each has a status:
  1. **✓ saved** in green: it matches its saved copy.
  2. **! changed** in yellow: it differs from its saved copy.
  3. **\+ not saved** in blue: chezmoi has no copy of it yet.
  4. **· view only** in yellow, with the reason in the details, **· left out** when `.chezmoiignore` names it, and **· not on this Mac**.
  5. **secret** in red after the status: it may hold a token, so its preview is hidden and it starts left out of exports.
* **Your dotfiles**, below the list down to the bottom, when there is room. First: the repository and branch, whether files wait to be exported or pushed, and chezmoi's folder, and why neet will not run chezmoi when it will not. Then **Legend**, for the marks the list shows. Then **Backups**, how many neet keeps of the listed files, the newest, and where they are. Like Quick Clean, each is its own box. When room is short, Backups goes first, then the legend.
* **The list** is 45% of the width, between 48 and 64 columns, so its rows are never mostly empty.
* **Right, from 96 columns,** taking the rest of the width: the selected file: its path, where a link leads, its saved copy, when it changed, and how many backups it has. Below, what its status means and what to do. Under that, the start of the file. A file that may hold a token never shows its preview.
* Below 96 columns, the details sit under the list, and the preview is hidden.
* **`Enter`** opens a small menu of only what can be done with the selected file, each with its key. A file that changed offers **Save my version**, **Use the saved version**, and **Show the changes**. One that is saved offers **Edit**, and **Configure** for Git, tmux, Starship, kitty, and mise files. One that is not saved also offers **Add to your dotfiles**. The keys also work straight from the list.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. Group names are skipped. |
| `g` / `G` | Jump to the first or last file. |
| `Enter` | Show what can be done with the file. |
| `e` | Edit a copy in your editor, then review it. See [Edit](#edit). |
| `r` | When the file changed: save this version in your dotfiles. See [Save Or Use The Saved Version](#save-or-use-the-saved-version). |
| `p` | When the file changed: use the saved version. |
| `a` | When the file is not saved: add it. See [Add To Your Dotfiles](#add-to-your-dotfiles). |
| `c` | Change the file's settings one at a time, for Git, tmux, Starship, kitty, and mise files. See [Configure](#configure). |
| `d` | Show how the file differs from its saved copy, or else what changed since its last backup. Any other key goes back. |
| `b` | List its backups, newest first. `Enter` shows what restoring one changes, then `y` restores it. |
| `x` | Export: review for secrets, commit, then push. See [Export](#export). |
| `.` | Show or hide the files that are not on this Mac. |

### Edit

```text
+ Change to .zshrc --------------------------------------------------------------------------------+
| Writes   dot_zshrc in chezmoi, then chezmoi apply ~/.zshrc                                       |
| Backup   first, in ~/.local/state/neet/backups/dotfiles                                          |
| Check    zsh -n  passed                                                                          |
| Changes  +1 -1                                                                                   |
+--------------------------------------------------------------------------------------------------+
+ Diff --------------------------------------------------------------------------------------------+
| line 1                                                                                           |
|   # ~/.zshrc                                                                                     |
|                                                                                                  |
| - export EDITOR=nvim                                                                             |
| + export EDITOR=hx                                                                               |
|   export PATH=$HOME/bin:$PATH                                                                    |
|                                                                                                  |
|   HISTSIZE=50000                                                                                 |
```

* `e` opens a copy of the file in your editor: `$VISUAL`, then `$EDITOR`, then `nano`. neet waits until it closes.
* With chezmoi, the copy is of the source file, and a file that changed must be saved, or the saved version used, first.
* When the editor closes with changes, the review shows:
  1. **Writes:** the file, or the source file and then `chezmoi apply` for it. When neet may not run chezmoi, it says to run `chezmoi apply` yourself.
  2. **Backup:** where the old version goes.
  3. **Check:** passed in green, failed in red with its message, or why there is no check.
  4. **Changes** and the **Diff:** removed lines in red, added lines in green, with three lines around each change. A file that may hold a token hides its diff.
* A file that fails its check cannot be written. `e` goes back to the editor with your changes.
* Afterwards, a box says what was written, or why nothing was.

| Key | Action |
| --- | --- |
| `y` | Write it. |
| `e` | Edit again. |
| `Up` / `Down` | Scroll the diff. |
| `Esc` | Drop the change. Nothing is written. |

### Export

```text
+ Export ---------------------------------------------------------------------------------------+
| Review for secrets, then pick what goes in.                                                    |
|                                                                                                |
|   [x] dot_zshrc                   nothing found                                                |
| > [ ] dot_npmrc                   1 line that may be secret                                    |
|         line 1  npm token  npm_****                                                            |
|                                                                                                |
| Commit   1 file . "Update zshrc"                                                               |
| Push     to origin/main, after a question                                                       |
|                                                                                                |
| This is a review, not a promise that nothing secret remains.                                   |
+------------------------------------------------------------------------------------------------+
```

* `x` looks at the repository chezmoi's folder is in. It lists the source files of listed dotfiles that have changes not yet committed. Each one shows what the review for secrets found, and the selected file shows each finding's line, kind, and first four characters.
* Files with findings, and files that may hold a token, start left out. `Space` puts a file in or takes it out.
* **Commit** shows how many files go in, and the message. It names the files, such as `Update zshrc and gitconfig`, until you change it with `m`.
* `y` commits only those files. Then a box asks whether to push, and names the remote branch. Only `y` pushes. A push is never forced, and if the remote has commits you do not, nothing is pushed and the note says to pull first.
* With nothing to commit but commits not yet pushed, `x` goes straight to the push question.
* Without chezmoi, `x` says so. Starting a repository in chezmoi's layout comes next.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between files. |
| `Space` | Put the file in, or take it out. |
| `m` | Change the commit message. `Enter` keeps it, `Esc` cancels. |
| `y` | Commit, then ask about pushing. |
| `Esc` | Go back. Nothing is committed. |

### Configure

```text
+ Git settings . ~/.gitconfig ------------------------------------------------------------------+
|   Setting               Value                 About                                           |
|                                                                                                |
|   user.name             You                   Name on your commits.                           |
|   user.email            you@example.com       Email on your commits.                          |
|   init.defaultBranch    not set               Branch name for new repositories.               |
|   core.editor           not set               Editor for commit messages.                     |
| > pull.rebase           true                  Rebase instead of merge when pulling.           |
|   push.autoSetupRemote  not set               Push a new branch without naming the remote.    |
+------------------------------------------------------------------------------------------------+
+ Changes ---------------------------------------------------------------------------------------+
| user.email  you@example.com                                                                    |
| pull.rebase  true                                                                              |
+------------------------------------------------------------------------------------------------+
```

* `c` opens the settings Configure knows for the file's program: Git, for `.gitconfig` and `.config/git/config`; tmux, for `.tmux.conf` and `.config/tmux/tmux.conf`; Starship, for `.config/starship.toml`; kitty, for `.config/kitty/kitty.conf`; and mise, for `.config/mise/config.toml`. Other files say to use `e`. The settings for each are listed in [SAFETY.md](SAFETY.md#configure).
* Each setting shows its value in the file, or **not set**, and what it does. A value about to change is yellow.
* `Enter` changes the selected setting. Settings with a few choices, such as on and off, or vi and emacs, step through each, then not set. Text and number settings, such as a font size of `13.5`, open a box to type in, where `Enter` keeps it, `Esc` cancels, and an empty value means not set. A value the setting does not take, such as a word for a number, stays in the box with the reason in red.
* **Changes** lists every change so far. `y` makes them all on the copy, then opens the same review as [Edit](#edit), with the check and the diff. `Esc` drops them all.
* As with editing, a file that changed must be saved, or the saved version used, first.

### Backups

* `b` lists the file's backups in a box, newest first, each with the time it was saved, in UTC, and its size.
* `Enter` shows the restore question: the time of the backup, that the file as it is now is backed up first, and the diff from now to the backup. Only `y` goes ahead.
* With chezmoi, a restored file may then differ from its saved copy. `r` saves it there.

### Save Or Use The Saved Version

* `r` and `p` show the same layout as the review: what is saved or replaced, the backup, and the diff. Only `y` goes ahead.
* `r` makes chezmoi's source file match the file in your home folder, with `chezmoi re-add`, or by writing the source file when neet may not run chezmoi.
* `p` puts chezmoi's version back with `chezmoi apply`. When neet may not run chezmoi, it says which command to run instead.

### Add To Your Dotfiles

* `a` shows the same layout: the source file it adds, such as `dot_tmux.conf`, how, and the whole file as new lines. Only `y` goes ahead.
* It runs `chezmoi add` for that one file, or writes the source file itself, named as chezmoi would, when neet may not run chezmoi.
* The file in your home folder stays as it is. The new source file waits for [Export](#export).
* Not for a file `.chezmoiignore` leaves out, a view only file, or a link, which another tool may manage.

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

### Dotfiles: Still To Come

* Drafts for the rest of [Dotfiles](#dotfiles).

| Feature | Layout |
| --- | --- |
| Start a repository | Without chezmoi, `x` offers to start a Git repository in chezmoi's layout, then add the chosen files to it. |
| More Configure programs | Settings for more programs, such as Ghostty, the same way as kitty. Shell settings only in a block neet owns. |

