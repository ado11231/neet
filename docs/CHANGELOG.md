# Changelog

* Release notes for the neet app.
* Library changes are in [CHANGELOG-CORE.md](CHANGELOG-CORE.md).

## Unreleased

## [0.3.0](https://github.com/ado11231/neet/compare/neet-v0.2.0...neet-v0.3.0)

### Added


* add a Shell PATH tab to Settings

* show folder sizes as areas

* add the Settings screen

* read and change power, Mac, and neet settings

* continue, archive, and fully clean up chats

* show skill sources and review removing a skill link

* explain leftover runtime downloads

* explain space outside the home scan

### Fixed


* show the same size for other volumes everywhere

### Other


* add Settings and the new Disk and AI Tools features

* compare the empty layout directly

* wait for the refresh the cleanup starts

* install with cargo and tidy the Use section

Released 2026-10-16

## [0.2.0](https://github.com/ado11231/neet/compare/neet-v0.1.0...neet-v0.2.0)

### Added


* [**breaking**] remove SSH support

* show images held by running containers

* review and trash selected chat files

* select chats individually or by filter

* show searchable chat titles and project tags

* add local tools and skill manager

* polish the Startup layout and fix the legend

* t turns your own launch agents off and back on

* Startup lists programs that start on their own

* x starts a repository when chezmoi's folder is missing

* Configure zsh settings in a block neet owns

* Configure Ghostty settings

* Disk first on Home, SSH last

* Disk shows the whole disk, <1% shares & its name in the title

* Disk in the same boxes & words as the other screens

* Large Files in the same boxes & words as the other screens

* Remove App fits its lists & fills the space below with boxes

* Review steps wrap under their text, plainer confirm

* Deep Clean in plainer words, with values that wrap under themselves

* Docker in small boxes, and & for and on the cleanup screens

* a Trash screen that empties the Trash after a red question

* Review's summary is small boxes, with a bar beside each path

* build folders and Simulators lay their boxes out in two columns

* sections are separate boxes, with plainer words

* Deep Clean's detail boxes keep their size whichever rule is selected

* shorter section titles, a steady Location box, and no keys while loading

* each section's heading sits right under its rule

* fill Deep Clean, the build folder list, and Simulators with sections

* sections sit in even bands, each just under its rule

* Quick Clean says where each row is and what clearing it means, with rules between sections

* Configure kitty and mise settings

* Configure steps through choices and checks numbers as you type

* Large Files clarity: aligned ages, a fitted name column, and files by type

* Dotfiles reviews and questions are a centred card

* Dotfiles list keeps a sensible width, and the box under it is filled

* Quick Clean keeps its full height boxes, filled with sections

* Quick Clean is one centred rectangle on a full screen

* Quick Clean boxes fit their lines, centred, with After cleanup

* Home's boxes stand a row apart, with stars around them

* stars cover the whole Home screen evenly, in visible night colors

* give Home's selected row box a blank row above and below

* Home menu says only soon; Disk and Home folder in their own boxes

* spread Home's stars across the whole art column

* simpler Dotfiles screen: plain statuses, Enter opens what you can do

* add a listed file to chezmoi with a, after showing it whole

* export dotfiles with x: review for secrets, commit the chosen files, then push on y

* configure Git settings one at a time with c, then review them like an edit

* list a dotfile's backups and restore one, and show its diff with d

* edit dotfiles with a check and a diff, and keep or put back a file that differs from chezmoi

* leave the screen to open a file in your editor, then come back

* open Dotfiles from Home, view only, with chezmoi status, details, and a preview

* pick unused Docker volumes, reset Docker with x, and remove simulators with their runtimes

* list simulator devices and unused Docker volumes, and reset Docker to the Trash

* give Large Files a filter box, size bars, and a side with the selected file and where files are

* keep row colors when selected, say permanently, and center Quick Clean's empty list

* make Disk and Large Files view only, without d

* remove simulator runtimes and prune Docker from Quick Clean

* split Deep Clean's details into boxes that fit, with a chart of every rule

* clear build folders and installers from Quick Clean, and tidy its layout

* show each app's size in Remove App, color the list, and sort by size with s

* order Home by quickest first, rename Clean to Deep Clean, and drop most gray text

* put Quick Clean first on Home, marked start here

* plan Clean once and reuse it, with r to look again

* show the move and its result in a box in the middle

* add Quick Clean, one table of everything that can be cleared

* add a summary beside the review, and leave out rules that found nothing

* explain the selected item in the Disk preview, with colors

* say on Home why Finder shows more free space

### Fixed


* show failed cache estimates as unknown

* clear stuck app measurements and retry failed ones

* retry purgeable space when the lookup is lost

* show unknown sizes when lookups fail

* report missing cleanup plans

* report a missing repo result

* report a missing list result

* handle a lost final result

* report missing removal plans

* stop loading when a plan is lost

* stop loading when a result is lost

* report lost background results

* wait for emptying before leaving the screen

* keep quit blocked while help is open

* [**breaking**] show lookup errors and unknown prune sizes

* explain when a prune frees no space

* stop promising prune savings

* show each chat cleanup result

* clarify chat removal review

* show retry advice for busy app files

* save turn results and label attempts

* refuse plists changed since listing

* keep failed chats selected and correct dates

* preserve drafts and clarify tool browsing

* merge Remove App opens with sizes and bars ready

* Remove App opens with sizes and bars ready

* Configure rows under the header, About wraps, plainer compare note

* list the largest items inside the trash

* show the cache and log item count

* merge checkbox table alignment

* align checkbox table headers

* keep changes and exports tied to their review

* Disk's notes name Deep Clean, and send Docker.raw to Quick Clean

* every bottom edge reads Selected · …

* no gap under the Remove App & Simulators headers, a blue ✓ over checkboxes

* Simulators' table columns fit what they hold

* Quick Clean says no access when macOS blocks the Trash

* give chezmoi the file's home folder path, and never run it for a link

* match the blue accents to the Home art

* line up columns, brighten labels, and shorten screen text

* reset Docker by moving its disk image into the Trash directly, once nothing has it open

* size message boxes by word wrapping, so their last line shows

* say remove permanently for simulators, offer prune only when Docker can, and say 1 item in Disk

* line up Deep Clean's item counts

* fit the Finder line on one row, and space out Home's details

### Other


* bump similar from 2.7.0 to 3.2.0

* shorten install and use steps

* move and shorten changelogs

* explain chat cleanup screens

* track turn failure status

* explain chat selection and cleanup limits

* refresh feature and safety docs

* Dotfiles is done

* README, FEATURES & ROADMAP catch up with the clarity pass

* render the README Home image from a test, with Disk first

* Merge pull request #87 from ado11231/feat/home-order

* Merge pull request #78 from ado11231/feat/home-polish

* read put back's note before it wraps, so it passes where chezmoi is not installed

* hold the plan back so Deep Clean's loading test cannot race

* share the colored size bar through format

* describe the new Home order, Deep Clean, and white selection

* describe Quick Clean

* share the size colors between screens

* move the 0.1.0 date to the end of each changelog entry

* link the wiki and crates.io from the README

Released 2026-10-14

## [0.1.0](https://github.com/ado11231/neet/releases/tag/neet-v0.1.0)

### Added

* Home menu with disk space, scan progress, and the cat artwork.
* Background folder scans, a disk browser, and large file filters.
* Cache and log cleanup with risk levels and open app checks.
* App removal with a review of the app and its related files.
* Refuses to run as root.
* Required path review and confirmation before moving files to Trash.
* Skipped path reports and Full Disk Access help.
* Mac downloads, an installer, and `cargo install neet`.

### Fixed

* Clear reasons for blocked paths and incomplete scans.
* Better review groups, item counts, and narrow screen layout.

Released 2026-10-01
