<h1 align="center">neet</h1>

<p align="center">See what is filling your Mac's disk, and safely clear files your apps can make again. All from one terminal program.</p>

<br>

* **Status:** early development. Nothing below works yet.
* The library can walk folders, measure size on disk, count hard links once, and build a folder tree. These are not yet joined into a full scan.
* The program opens a Home menu, but the Disk and Clean screens are not built yet. Progress is in the [roadmap](docs/ROADMAP.md).

**1. Install neet (Planned)**

```sh
brew install neet
```

or:

```sh
cargo install neet
```

**2. Open It**

```sh
neet
```

**3. Pick From The Menu**

* neet opens on a Home screen with a menu. Move with the arrow keys and press `Enter` to open a screen. `Esc` goes back.
* Home also shows how full the disk is, and how the scan is going.

| Screen | What It Does |
| --- | --- |
| Disk | Browses your folders by size. |
| Clean | Moves files your apps can make again to the Trash, after you review every path. |

* Everything happens inside the program. The only options are `--version` and `--help`.
* Later screens are proposed for startup items, SSH, dotfiles, AI coding tool files, and power settings. See the [feature list](docs/FEATURES.md).

<br>

* Needs macOS 13 or later, on Apple silicon or Intel.
* Some folders need Full Disk Access, a macOS permission. Without it, neet lists the folders it skipped.
* Cleanup will never delete files for good. Everything goes to the Trash.
* Cleanup will never touch your own folders, such as Documents, Desktop, Downloads, iCloud Drive, Keychains, and `~/.ssh`.
* Licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your choice.

<p align="center">
  <a href="docs/README.md">Docs guide</a> ·
  <a href="docs/FEATURES.md">Feature catalog</a> ·
  <a href="docs/INTERFACE.md">Interface</a> ·
  <a href="docs/SAFETY.md">Safety</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="docs/ROADMAP.md">Roadmap</a>
</p>
