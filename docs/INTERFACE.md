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

* Lists files from the scan by size and when they last changed.
* Starts at 100 MB and any age. Lists up to 1,000 files and says when more exist.
* Details show the selected file, totals by folder, and file types.
* This screen is view only. `Enter` shows the file in Disk.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `g` / `G` | Jump to the first or last file. |
| `s` | Change the size: 10 MB, 50 MB, 100 MB, 500 MB, 1 GB, 5 GB. |
| `a` | Change how long files must be unchanged: any age, 30 days, 3 months, 6 months, 1 year, 2 years. |
| `Enter`, `Right`, or `l` | Show the file in Disk. `Esc` comes back. |

## Remove App

* Lists apps in `/Applications` and `~/Applications`, with sizes and blocked reasons.
* Sizes load in the background and stay available while neet runs.
* Open an app to see its files. Caches start selected; personal data and settings do not.
* An open app must be quit first. Apple apps and app links are refused.
* Review selected files before moving them to Trash. See [App Removal](SAFETY.md#app-removal).

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the selection. |
| `Enter` | On the app list, open the app. On the file list, go to [Review](#review-confirm-and-move). |
| `Space` | On the file list, select or clear a file. |
| `s` | On the app list, sort by size or by name. |

## Startup

* Groups startup items by kind and shows whether each is running, waiting, not loaded, or off.
* Details show the program, source file, signature, and any problem.
* `t` reviews turning your own launch agent off or back on. `y` confirms; `n` or `Esc` cancels.
* Other items stay view only. Apps that open at login are reached through System Settings.
* Results say whether nothing changed, part changed, or the change could not be confirmed.
* Saved history separates attempts from completed changes. See [Startup safety](SAFETY.md#startup-items).

| Key | Action |
| --- | --- |
| `t` | Review turning your own launch agent off or back on. |
| `o` | Open Login Items & Extensions in System Settings. |
| `f` | Show the selected plist or helper in Finder. |
| `r` | Look again. |

## Dotfiles

* Lists known settings files, grouped by program, with a preview and backups.
* A file can be saved, changed, not saved, view only, ignored, or missing.
* Saved means it matches chezmoi's copy. Files marked secret hide their preview and start left out of exports.
* `Enter` shows the actions for the selected file. Action keys also work from the list.
* If a file changed, save it or use the saved version before editing.
* See [file limits and checks](SAFETY.md#dotfiles).

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

* Opens a copy in `$VISUAL`, `$EDITOR`, or `nano`.
* Review shows the destination, backup, check result, and diff.
* Failed checks block saving. `e` returns to the editor.
* With chezmoi, neet edits the source copy and applies it when allowed.
* Files that may hold tokens hide the diff.

| Key | Action |
| --- | --- |
| `y` | Write it. |
| `e` | Edit again. |
| `Up` / `Down` | Scroll the diff. |
| `Esc` | Drop the change. Nothing is written. |

### Export

* Lists changed dotfiles and possible secrets. Files with findings start left out.
* Review the files and commit message. Secret checks cannot catch everything.
* `y` commits only selected files, then asks separately before pushing to the named branch.
* Without chezmoi, export can start a local dotfiles repository from selected files.
* With no remote, neet can offer to create a private GitHub repository if `gh` is installed.
* Nothing is pushed without confirmation. See [Export safety](SAFETY.md#export).

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move between files. |
| `Space` | Put the file in, or take it out. |
| `m` | Change the commit message. `Enter` keeps it, `Esc` cancels. |
| `y` | Commit, then ask about pushing. |
| `Esc` | Go back. Nothing is committed. |

### Configure

* Changes known settings for Git, tmux, Starship, kitty, Ghostty, mise, and zsh.
* zsh changes stay between neet's marker lines in `.zshrc`.
* `Enter` changes a value. Invalid values show a reason.
* `y` opens the normal edit review with a check and diff. `Esc` drops the changes.
* Other settings use the editor. See [supported settings](SAFETY.md#configure).

### Backups

* `b` lists backups, newest first, with the saved time in UTC and size.
* `Enter` shows what restore will change. Only `y` restores.
* Restore backs up the current file first.
* With chezmoi, `r` saves the restored version in its source copy.

### Save Or Use The Saved Version

* `r` saves your current file in chezmoi's source copy.
* `p` replaces your file with chezmoi's saved copy.
* Both show the change and backup before `y` confirms.
* If neet cannot safely run chezmoi, it shows the command to run yourself.

### Add To Your Dotfiles

* `a` reviews adding a listed file to chezmoi. Only `y` confirms.
* The home file stays unchanged. The new source copy waits for Export.
* Ignored files, view only files, and links cannot be added.

## AI Tools

* Files, Chats, and Skills share tool and search filters.
* Opening a file shows its path and preview. Chats show supported user and assistant messages.
* Chats have titles, project paths, and a tag for missing projects. An unknown path is not treated as missing.
* Select chats, then press `d` to review. Hidden selections are included.
* Review separates ready and skipped files. Totals and confirmation keys stay visible while scrolling.
* `y` moves ready transcripts to Trash. If none are ready, the review offers no move action.
* Results list moved, failed, and unconfirmed files. Check Trash before retrying unconfirmed files.
* Finder's Put Back restores transcripts. Assistant indexes, related files, and cloud history remain.
* Linking a skill reviews its source, destination, and instructions. Keep its source folder in place and reload the assistant afterward.
* Editing or restoring a user skill shows a diff and saves a backup after confirmation.
* A failed edit keeps the draft. Use `e` to repair it or `Esc` to discard it.
* Tool settings and bundled skills stay view only. See [AI Tools safety](SAFETY.md#ai-tools).

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

These screens are not built. Keys and layouts are still undecided.

| Screen | Plan |
| --- | --- |
| Space Breakdown | Explain used space outside the home scan. |
| SSH | Inspect hosts and keys, and review permission or known host changes. |
| PATH | Show shell search folders and review edits with backups. |
| Settings | Power and display controls with undo, plus saved choices. |
