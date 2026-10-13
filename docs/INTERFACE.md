# Interface

* Screens, keys, and actions.
* How to review changes and read results.

## Contents

* [Every Screen](#every-screen)
* [Home](#home)
* [Skipped](#skipped)
* [Disk](#disk)
* [Quick Clean](#quick-clean)
* [Deep Clean](#deep-clean)
* [Review, Confirm, And Move](#review-confirm-and-move)
* [Trash](#trash)
* [Large Files](#large-files)
* [Remove App](#remove-app)
* [Startup](#startup)
* [Dotfiles](#dotfiles)
* [AI Tools](#ai-tools)
* [Planned Screens](#planned-screens)

## Every Screen

* neet scans your home folder in the background.
* The footer lists the keys for the current screen. `?` opens help.
* The selected row is bold. Text and symbols explain warnings as well as color.
* Lists and reviews scroll. Narrow screens keep the main actions and selected details.
* A loading box shows progress while work runs.

| Key | Action |
| --- | --- |
| Arrow keys, or `h` `j` `k` `l` | Move. |
| `Esc` | Go back one step. |
| `?` | Show the keys for this screen. |
| `q` | Quit. Does nothing while a question is open or files are moving. |

## Home

* Opens on Disk, followed by cleanup tools, Startup, Dotfiles, and AI Tools.
* Features marked `soon` are not built yet.
* Shows disk space, scan progress, skipped paths, and what Deep Clean found.
* Finder can count space macOS may clear as free. Home explains the difference.
* An incomplete scan shows `at least` before the home folder size.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` or `Right` | Open the selected screen. |
| `1` to `9` | Open that row. Rows after the ninth have no number. |
| `s` | Open Skipped. |

## Skipped

* Lists folders blocked by macOS, other read errors, and skipped disks.
* Gives Full Disk Access steps for your terminal when needed.
* Says when nothing was skipped.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Scroll. |
| `PgUp` / `PgDn` | Scroll a page. |
| `g` / `G` | Jump to the top or bottom. |

## Disk

* Browse the scan by folder, largest first.
* Rows show size, share, and name. Details explain the selected item and its cleanup limits.
* Folders end in `/` and links in `@`.
* Disk is view only. Use Quick Clean, Deep Clean, or Remove App to clean.
* The list stays as scanned until the next scan.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Right`, `Enter`, or `l` | Open the selected folder. |
| `Left` or `h` | Go up to the parent folder, keeping your place. |
| `s` | Sort by size, then name, then item count. |
| `g` / `G` | Jump to the first or last row. |

## Quick Clean

* Lists caches and logs, build folders, installers, Trash, Docker, simulators, and temporary files.
* Shows each group's size, count, largest items, and how it can be cleared.
* Build folders and installers go through selection and review.
* Docker and simulator removal is permanent and asks first.
* Temporary files are left to macOS.
* If Trash cannot be read, the screen says `no access` and explains Full Disk Access.
* Looking changes nothing. Open a row to see its actions.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `g` / `G` | Jump to the first or last row. |
| `Enter`, `Right`, or `l` | On Caches & logs, open [Deep Clean](#deep-clean). On build folders or installers, open [Pick](#pick). On Docker or simulator runtimes, open [Docker And Simulators](#docker-and-simulators). On the Trash, open the [Trash](#trash) screen. |
| `d` | Show the largest item of the row in [Disk](#disk). |

### Pick

* Lists build folders or installers, largest first, with paths, sizes, and ages.
* All allowed items start selected. Refused items show a reason.
* Clear anything you want to keep, then review the selection.
* Files go to Trash. See [allowed items](SAFETY.md#build-folders-and-installers).

| Key | Action |
| --- | --- |
| `Space` | Select or clear an item. |
| `a` | Select all, or clear all when all are selected. |
| `Enter` | Go to [Review](#review-confirm-and-move). |

### Docker And Simulators

* Permanent actions ask in a red box. Only `y` confirms.
* Simulators lists runtimes and their devices. Nothing starts selected.
* Removing a runtime also removes its simulators. Devices without a runtime have a separate row.
* Docker shows reported sizes and unused volumes. No volumes start selected.
* Prune removes unused Docker data and the volumes you select.
* Docker's sizes are estimates. Savings stay unknown until prune finishes.
* `c` lists running containers, images, and Compose projects. Scroll with arrows or `j` and `k`; use `Home` and `End` to jump. `c` or `Esc` returns.
* If prune frees no space, the result names running containers using images. Failed lookups stay visible. Volume results are listed separately.
* Reset stops Docker Desktop and moves its whole disk image to Trash. See [reset and restore steps](SAFETY.md#resetting-docker).
* If Docker is stopped, `o` opens it and `r` checks again. Reset also works while it is stopped.

| Key | Action |
| --- | --- |
| `↑` `↓` | Move between runtimes, or between Docker's unused volumes. |
| `Space` | Select or clear a runtime, the `No runtime` row, or a volume. |
| `Enter` | Ask before removing, or before the prune. |
| `c` | On Docker or its prune result, show containers keeping images in use. |
| `x` | On Docker, ask before resetting it to the Trash. |
| `o` | On Docker, open Docker Desktop. |
| `r` | On Docker, ask Docker again. |
| `y` | In the question, go ahead. |
| `n` or `Esc` | In the question, go back. |

## Deep Clean

* Lists cleanup rules, risk levels, item counts, and sizes.
* Details show what each rule removes, apps to close, found paths, and skip reasons.
* Results are kept until cleanup finishes or you press `r`.
* Safe rules start selected unless their app is open. See [risk levels](FEATURES.md#risk-levels).
* Selecting an expert rule asks you to type its ID. A wrong ID selects nothing.
* Busy app advice names the app to quit and says to press `r`.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between rules. |
| `g` / `G` | Jump to the first or last rule. |
| `Space` | Select or clear the rule. Does nothing if it found nothing, or if its app is open. |
| `Enter` | Review the selected rules. Does nothing when none are selected. |
| `r` | Look again. Your selection starts over. |

## Review, Confirm, And Move

* Deep Clean, Pick, and Remove App share this flow.
* Review every path, then confirm. Items are checked again before moving.
* Files take space in Trash until it is emptied. Finder's Put Back restores them.

| Screen | Shows | Keys |
| --- | --- | --- |
| Review | Selected paths, sizes, and totals. | Arrows, `PgUp` / `PgDn`, `g` / `G` scroll. `Enter` continues. `Esc` goes back. |
| Confirm | Count, size, and the move to Trash. | `y` moves. `n` or `Esc` cancels. |
| Move | Progress and moved count. | Wait until it finishes. |
| Result | Moved and skipped items, with reasons. | Arrows scroll. `Enter` or `Esc` returns Home. |

## Trash

* Open from Quick Clean to see Trash items, largest first.
* `e` asks before emptying. Only `y` confirms permanent deletion.
* Finder empties Trash on all disks, not just the items shown from your home folder.
* Emptying cannot be undone. See [Trash safety](SAFETY.md#emptying-the-trash).
* If macOS blocks the scan, enable Full Disk Access before emptying from neet.
* Other screens keep their old scan totals until the next scan.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between items. |
| `e` | Empty the Trash, after a question. |
| `y` / `n` | In the question: empty it for good, or go back. |
| `o` | Open the Trash in Finder, where Put Back works. |

## Large Files

```text
+ Filters --------------------------------------------------------------+  + Docker.raw -------------------------------------+
| Size       10 MB   50 MB  [100 MB]  500 MB   1 GB   5 GB  s changes size |  | Folder    ~/Library/.../com.docker.docker        |
| Unchanged  [any age]  30 days  3 months  6 months  1 year  a changes age |  | Size      11.0 GB                                |
+-----------------------------------------------------------------------+  | Share     59% of the files found                 |
+ Large Files ----------------------------------------------------------+  | Changed   28 days ago                            |
|        Size            Last changed   Name          Folder           |  | Type      Virtual disk                           |
| >   11.0 GB  ########  28 days ago    Docker.raw    ~/Library/...    |  |                                                  |
|      4.3 GB  ###.....   2 months ago  weights.bin   ~/Library/...    |  | Used by a virtual machine or Docker. Free it     |
|      3.1 GB  ##......   6 months ago  Xcode_16.dmg  ~/Downloads      |  | from the app that made it.                       |
|                                                                      |  +------------------------- Enter shows it in Disk +
|                                                                      |  + Total -------------------------------------------+
|                                                                      |  | Found     18.4 GB . 3 files                      |
|                                                                      |  | Showing   100 MB or more, any age                |
|                                                                      |  +--------------------------------------------------+
|                                                                      |  + Location ----------------------------------------+
|                                                                      |  |   11.0 GB  ##########  ~/Library/Containers      |
|                                                                      |  |    3.1 GB  ###.......  ~/Downloads               |
|                                                                      |  +--------------------------------------------------+
|                                                                      |  + Types -------------------------------------------+
|                                                                      |  |   11.0 GB  ##########  Virtual disk       1 file |
+ Found 3 files . 18.4 GB ---------------------------------------------+  +--------------------------------------------------+
```

* Large Files only shows files. Nothing is moved from here: `Enter` shows the file in Disk.
* **Filters, top:** both filters, with every choice listed and the current one green in brackets, and the key that changes each lined up on the right. When the box is too narrow for every choice, it shows only the current ones. Starts at 100 MB and any age.
* **Large Files:** a table of files from the scan, with no gap under its header. Folders are not listed.
  1. Its size, and a bar of it against the largest file, both red from 5 GB and yellow from 1 GB.
  2. When it last changed, such as ` 3 days ago`. The number is right aligned, so the ones line up with the ones of `39 days ago` and every unit starts in the same place. Files unchanged for a year or more have a magenta date.
  3. Its name and folder. The name column is only as wide as the longest name, up to 40 columns, and the folder gets the rest. A long name is shortened in the middle, so its extension shows. A long folder keeps its start, such as `~/Library`, and its last folders.
  4. The bottom edge reads `Found N files · size`. Up to 1,000 are listed, and the edge says when only the largest are shown.
  5. When no file matches, the box says so in the middle, with the keys to widen the filters.
* **Right, from 120 columns,** in small boxes as on the other screens: the selected file, titled with its name: its folder in blue, size, share of the files found, when it changed, its type from the extension, and a plain hint for common types, such as installers, archives, videos, program code, caches, and virtual disks. Then **Total**: everything found, and the filters in words. Then **Location**: the files added up by folder, largest first, with `~/Library` split one level further, since most large files are there. Under it, when there is room, **Types**: the files added up by type, with how many of each. Files of no known type count as **Other**. Bars are 10 wide with two spaces around them, and labels share one width, as on the other screens.
* Below 120 columns, a box titled with the file's name sits below the file table when at least 18 rows are available: its folder, its size, and when it changed and its type.
* Until the scan finishes, the screen shows the loading box.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `g` / `G` | Jump to the first or last file. |
| `s` | Change the size: 10 MB, 50 MB, 100 MB, 500 MB, 1 GB, 5 GB. |
| `a` | Change how long files must be unchanged: any age, 30 days, 3 months, 6 months, 1 year, 2 years. |
| `Enter`, `Right`, or `l` | Show the file in Disk. `Esc` comes back. |

## Remove App

```text
+ Remove App --------------------------------------------------------------------------------+
|   Name             Size                Folder          App ID                               |
| > Discord      500.4 MB  ##..........  /Applications   com.hnc.Discord                      |
|   Docker         2.6 GB  #######.....  /Applications   com.docker.docker                    |
|   Slingshot    585.7 KB  ............  ~/Applications  dev.slingshot.menubar                |
|   Xcode [blocked] 4.3 GB               /Applications   blocked: part of macOS               |
+ 15 apps can be removed . 9.1 GB --------------------------- By name . s sorts by size +
+ Discord -------------------------------------++ Total -------------------------------------+
| Location  /Applications/Discord.app          || Apps      16                               |
| Size      500.4 MB                           || Removable 15                               |
| App ID    com.hnc.Discord                    || Blocked   1                                |
| Status    can be removed                     || Size      9.1 GB                           |
|                                              |+--------------------------------------------+
| Press Enter to see the app & the files ...   |+ Folders -----------------------------------+
+----------------------------------------------+| /Applications   14 apps, for every user    |
+ Largest -------------------------------------+| ~/Applications  2 apps, only yours         |
|    2.6 GB  ##########  Docker                 |+--------------------------------------------+
|  500.4 MB  ##........  Discord                |+ Steps -------------------------------------+
|                                              || 1. Press Enter on an app to see its files. |
+----------------------------------------------++--------------------------------------------+

+ Discord ----------------------------------------------------------------- Goes to the Trash +
|   ✓         Size  Name                   Folder                          Kind              |
| > [✓]   500.4 MB  Discord.app            /Applications                   the app           |
|   [✓]     1.1 MB  com.hnc.Discord        ~/Library/Caches                made again        |
|   [ ]     4.1 KB  com.hnc.Discord.plist  ~/Library/Preferences           settings          |
|   [ ]     1.1 GB  discord                ~/Library/Application Support   matched by name   |
+ Selected . 2 items . 501.5 MB -----------------------------------------------------------+
+ Discord.app ---------------------------------++ Total -------------------------------------+
| Folder    /Applications                      || Found     1.6 GB . 6 items                 |
| Size      500.4 MB                           || Selected  501.5 MB . 2 items               |
| Kind      the app                            || App ID    com.hnc.Discord                  |
| Selected  yes                                |+--------------------------------------------+
|                                              |+ Steps / After cleanup ...                  |
| The app itself.                              ||                                            |
+----------------------------------------------++--------------------------------------------+
```

* **App list:** a table of each app in `/Applications` and `~/Applications`, with its name, its size, a bar of its size against the largest app, its folder, and its app ID (the name macOS uses to identify it). Apps neet can remove come first. Apps it will not remove are listed after them, marked `[blocked]`, with the reason in plain words in yellow: `part of macOS`, `a shortcut to another app`, or `neet can't tell which app it is`.
* **Colors:** sizes and bars are red from 5 GB and yellow from 1 GB, and bars are green below that. `/Applications` and app IDs use moonlight blue; `~/Applications` is magenta.
* Sizes are measured in the background from the moment neet starts, four apps at a time, and kept while neet runs, so the screen usually opens with every size and bar. An app not measured yet shows `…`. Press `s` to list the largest first, and again to go back to names.
* The table is only as tall as its rows, with no gap under its header. The bottom edge shows how many apps can be removed and how much space they take, with the sort order or a note on the top edge.
* **Below the app list,** in small boxes as in Quick Clean, side by side from 100 columns: the app the arrow is on (Location, Size, App ID, and Status: can be removed, open, or blocked, with what to do next), **Largest**, the ten largest apps with bars; then **Total** (apps, removable, blocked, size), **Folders** (what each Applications folder holds, in the list's colors), and **Steps**. On a screen too short for the boxes, a Selected box under the list keeps the path and app ID instead.
* **App files:** opening an app finds and measures its files, with the loading box and a timer. Then a table, only as tall as its rows, lists the app and each file, with a checkbox under a blue `✓` header, size, name, folder, and its kind: `the app`, `made again` in green, or one to check first in yellow, such as `may be your data`.
* **Below the file list:** the file the arrow is on (Folder, Size, Kind, Selected, and a plain sentence on why it starts selected or not), **Overview**, every file's size with a bar, green when selected; then **Total** (found, selected, app ID), **Steps**, and **After cleanup**, the disk now and after.
* The bottom edge shows the selected total, in green once something is selected.
* What is found, and what starts selected, is in [SAFETY.md](SAFETY.md#app-removal).

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` | On the app list, open the app. On the file list, go to [Review](#review-confirm-and-move). |
| `Space` | On the file list, select or clear a file. |
| `s` | On the app list, sort by size or by name. |

* An app that is open cannot be opened here. The note says to quit it first.

## Startup

```text
 Startup ------------------------------------------------------------ 15 items . 6 running
+ Programs -------------------------------++ DockerHelper ----------------------------------+
|   Allowed in the background             || Kind     a helper inside an app               |
| >   DockerHelper        o waiting       || App      Docker                               |
|     com.ollama.ollama   o waiting       || Label    com.docker.helper                    |
|   Your launch agents                    || Helper   /Applications/.../DockerHelper.app   |
|     com.google.Goog...  o waiting       || Signed   Docker Inc                           |
|     com.google.keyst... . not loaded !  || Starts   when you log in                      |
|   Launch daemons                        || Now      waiting, and starts when it is needed|
|     com.docker.vmnetd   * running       ||                                               |
+-----------------------------------------+| View only. Press o to turn it off in System   |
+ Legend ---------------------------------+| Settings, under Allow in the Background.      |
| * running     running now               |+-----------------------------------------------+
| o waiting     starts when it is needed  |+ Open at login --------------------------------+
| . not loaded  not running               || macOS shows apps that open at login only to   |
| off           turned off                || an admin, so they are not listed here.        |
| !             something is wrong        || o opens them in System Settings.              |
+-----------------------------------------++-----------------------------------------------+
```

* Startup opens with a loading box while it reads, about a second, then lists every program that starts on its own, grouped by kind: **Allowed in the background** (helpers inside apps that the app asked macOS to run), **Your launch agents**, **Launch agents for every user**, and **Launch daemons**. Its read and change rules are in [SAFETY.md](SAFETY.md#startup-items).
* Each row shows the name and whether it runs: **● running** in green, **○ waiting** in blue for one launchd starts when needed, **· not loaded**, or **off** in yellow when it is turned off. A yellow **!** means something is wrong with it, such as an empty plist or a missing program.
* The top line counts the items, how many run, and how many are turned off.
* The box beside the list shows the selected item: its kind, app, label, program, plist or helper, who signed it, when it starts, and what it is doing now, then any problem and whether `t` can turn it off or back on.
* Below it, **Open at login** says those apps are not listed, since macOS shows them only to an admin, and how many items belong to macOS and are only counted. **By kind** counts each kind, and how many of each run.
* Under 96 columns, the selected item shows under the list.
* `t` reviews turning one of your own launch agents off or back on. The question names the item and action. `y` confirms; `n` or `Esc` cancels. Other items stay view only.
* A failed turn shows **Not changed**, **Partly changed**, or **Change not confirmed**, with the reason. If the turn finished but its result could not be saved, it shows **Changed** with the save error.
* Saved history shows **Attempt** for failed, partial, or unknown results, and **Changed** for completed turns. Only completed turns appear under **Turned off by neet**.

| Key | Action |
| --- | --- |
| `t` | Review turning your own launch agent off or back on. |
| `o` | Open Login Items & Extensions in System Settings. |
| `f` | Show the selected plist or helper in Finder. |
| `r` | Look again. |

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
* **`Enter`** opens a small menu of only what can be done with the selected file, each with its key. A file that changed offers **Save my version**, **Use the saved version**, and **Show the changes**. One that is saved offers **Edit**, and **Configure** for Git, tmux, Starship, kitty, Ghostty, mise, and `.zshrc` files. One that is not saved also offers **Add to your dotfiles**. The keys also work straight from the list.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. Group names are skipped. |
| `g` / `G` | Jump to the first or last file. |
| `Enter` | Show what can be done with the file. |
| `e` | Edit a copy in your editor, then review it. See [Edit](#edit). |
| `r` | When the file changed: save this version in your dotfiles. See [Save Or Use The Saved Version](#save-or-use-the-saved-version). |
| `p` | When the file changed: use the saved version. |
| `a` | When the file is not saved: add it. See [Add To Your Dotfiles](#add-to-your-dotfiles). |
| `c` | Change the file's settings one at a time, for Git, tmux, Starship, kitty, Ghostty, mise, and `.zshrc` files. See [Configure](#configure). |
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
* Export scrolls to keep the selected file visible. The commit summary and message field have their own space below the list.
* Files with findings, and files that may hold a token, start left out. `Space` puts a file in or takes it out.
* **Commit** shows how many files go in, and the message. It names the files, such as `Update zshrc and gitconfig`, until you change it with `m`.
* `y` commits only those files. Then a box asks whether to push, and names the remote branch. Only `y` pushes. A push is never forced, and if the remote has commits you do not, nothing is pushed and the note says to pull first.
* With nothing to commit but commits not yet pushed, `x` goes straight to the push question.
* **Without chezmoi**, the Your dotfiles box says `x` starts a repository. `x` opens the same review, titled **Start your dotfiles**, listing every listed dotfile on this Mac by its home path. Links that lead out of your home folder, or into a protected folder, are left out. The Review box adds a **Start** line naming the folder, and a **GitHub** line in place of Push. The message starts as `Start dotfiles`.
* `y` writes the chosen files into chezmoi's folder, named as `chezmoi add` names them, with a README and a `.chezmoiignore` that keeps the README out of your home folder, then runs `git init` and commits. The screen then reads the folder as chezmoi's, so every file shows saved or not saved.
* With no remote, and `gh` installed, a box asks whether to make a private GitHub repository named `dotfiles` and push to it. Only `y` makes it. Without `gh`, a note says how to add a remote yourself.

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
|   Setting               Value            About                                                 |
|   user.name             You              Name on your commits.                                 |
|   user.email            you@example.com  Email on your commits.                                |
|   init.defaultBranch    not set          Branch name for new repositories.                     |
|   core.editor           not set          Editor for commit messages.                           |
| > pull.rebase           true             Rebase instead of merge when pulling.                 |
|   push.autoSetupRemote  not set          Push a new branch without naming the remote.          |
+------------------------------------------------------------------------------------------------+
+ Changes ---------------------------------------------------------------------------------------+
| user.email  you@example.com                                                                    |
| pull.rebase  true                                                                              |
+------------------------------------------------------------------------------------------------+
```

* `c` opens the settings Configure knows for the file's program: Git, for `.gitconfig` and `.config/git/config`; tmux, for `.tmux.conf` and `.config/tmux/tmux.conf`; Starship, for `.config/starship.toml`; kitty, for `.config/kitty/kitty.conf`; Ghostty, for `.config/ghostty/config` and `.config/ghostty/config.ghostty`; mise, for `.config/mise/config.toml`; and zsh, for `.zshrc`, where neet writes only between its `# >>> neet >>>` marker lines, and the screen says so. Other files say to use `e`. The settings for each are listed in [SAFETY.md](SAFETY.md#configure).
* Each setting shows its value in the file, or **not set**, and what it does. A value about to change is yellow. Rows start right under the header. The Setting and Value columns are as wide as their longest entry, and About takes the rest, wrapping onto a second line when it does not fit.
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

## AI Tools

* Home key `8` opens AI Tools. Files, Chats, and Skills share one list with a tool filter and path search. Rows lead with the file, skill name, or chat title. Codex and Gemini filters include shared `.agents/skills`.
* The selected path, scope, size, and available actions appear beside the list, or below it on narrow terminals. Opening Chats reads bounded local summaries. Other file contents are read only when opened. The scrollable preview includes the full path and scope on small terminals.
* Chats show a saved title or first user message, the recorded project path, and a `Missing project` tag when that directory no longer exists. Unknown or unreadable paths are not tagged as missing. Search matches titles, paths, and tags.
* Space selects a chat, `a` toggles all chats shown by the current filters, and `n` clears every selection. The status includes selections hidden by filters. `d` reviews all selected chats, including hidden selections, or the highlighted chat when none are selected.
* The chat removal review keeps the count, size, skipped count, hidden selection count, and local cleanup limits above a scrolling file list. Ready and skipped chats have separate headings. Each entry shows its title, tool, size, project, and full transcript path.
* The **Next** box keeps the close app advice, Finder's Put Back, and confirmation keys visible while scrolling. `y` moves the ready files to Trash in the background; `n` or `Esc` cancels. If no files are ready, it offers refresh advice and no move action.
* Results keep the totals above a scrolling list of **Not moved**, **Unconfirmed**, and **Moved to Trash** files. Failed files show their reason. If cleanup stops early, check the Trash before retrying unconfirmed files. Unmoved chats stay selected. Finder's Put Back restores moved files. Assistant indexes, related files, and cloud history remain.
* Chat previews show user and assistant messages from supported JSON and JSONL records. Tool calls and results are left out. Invalid records and preview limits are reported.
* Linking a skill asks for a local folder containing `SKILL.md`. `Tab` picks the target tool. The review names the source and destination and shows the instructions. `y` creates a directory link. Codex uses the shared `.agents/skills` folder. The source folder must stay in place. After linking, the Skills list selects it. Restart the assistant or reload its skills afterward.
* Editing opens a private copy in your editor. The diff must be confirmed with `y` before the original is backed up and replaced. Restore follows the same review.
* A failed load keeps the typed path and shows the error. A failed edit or save keeps the draft: `e` reopens it, and `Esc` discards it. If the original changed, copy your work before discarding the draft and open the skill again.
* Refresh keeps the selected file when it is still listed.

| Key | Action |
| --- | --- |
| `Tab` / `Shift-Tab` | Switch Files, Chats, and Skills. |
| `p` | Cycle All tools, Claude Code, Codex, Gemini CLI, and Shared. Codex and Gemini also show shared skills. |
| `/` | Filter by path, tool, chat title, or project. `Enter` keeps the filter; `Esc` clears it while typing or browsing. |
| `Enter` | Open the selected file or chat. |
| `Space` | Select or deselect the highlighted chat. |
| `a` / `n` | Toggle all shown chats / clear every chat selection. In a chat removal review, `n` cancels. |
| `d` / `Delete` | Review moving selected chat files, or the highlighted chat, to Trash. |
| `i` | Link a local skill folder. `Tab` or `Shift-Tab` chooses the tool while typing. |
| `e` | Edit a user skill, or reopen a kept draft, then review. |
| `b` | Review restoring the latest skill backup. |
| `y` | Confirm a reviewed skill change or chat removal. |
| `o` | Reveal the selected file in Finder, from the list or its preview. |
| `r` / `w` | Refresh / show skipped paths. |
| `Up` / `Down`, `PageUp` / `PageDown` | Move or scroll a preview or review. |
| `Esc` | Cancel input or review, discard a kept draft, close a preview, clear a search, then return Home. |

## Planned Screens

* Draft layouts for later features. They may change before they are built.

| Screen | Layout | Keys |
| --- | --- | --- |
| Space Breakdown | A list of parts that add up to the disk's used space: the scan, skipped folders, apps, macOS, snapshots, purgeable space, and anything unexplained. Opened with `b` on Home. | `Enter` opens a folder in Disk. |
| SSH | Sections for hosts, keys, agent keys, known hosts, and permission problems. | Fix permissions, add or remove agent keys, remove known hosts. |
| PATH | Each folder in search order, with how many programs it holds, where it was added, and any problem. A preview shows the programs it holds. | `w` finds which copy of a program runs. `K` / `J` move a folder. `a` adds, `x` removes, `Enter` saves with a backup. |
| Settings | Sections for power mode, graphics switching, refresh rate, Game Mode, what keeps the Mac awake, and wake settings. | Each change shows what it will do first, and can be undone. |
