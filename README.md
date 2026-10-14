<h1 align="center">neet</h1>

<p align="center">
  <a href="https://github.com/ado11231/neet/actions/workflows/ci.yml"><img src="https://github.com/ado11231/neet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/neet"><img src="https://img.shields.io/crates/v/neet?logo=rust" alt="neet on crates.io"></a>
  <img src="https://img.shields.io/badge/macOS-13%2B-black?logo=apple" alt="macOS 13 or later">
  <img src="https://img.shields.io/badge/Rust-1.88%2B-orange?logo=rust" alt="Rust 1.88 or later">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20or%20Apache%202.0-blue" alt="License: MIT or Apache 2.0"></a>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/ado11231/neet/master/docs/images/home.svg" alt="The neet Home screen, with the feature menu, the This Mac disk gauge, and scan progress" width="860">
</p>

## Install

* macOS 13 or later, on Apple silicon or Intel.
* Install the published crate with [Rust](https://rustup.rs):

```sh
cargo install neet --locked
```

For the newest code on `master`, including changes not yet released:

```sh
cargo install --git https://github.com/ado11231/neet --locked neet
```

* From a checkout, use `cargo run --release` to build and run that branch.
* Install a checkout with `cargo install --path crates/neet --locked --force`.
* Downloads are also on [GitHub Releases](https://github.com/ado11231/neet/releases).

## Use

```sh
neet
```

neet scans your home folder in the background while you pick a screen from Home.

| Key | Does |
| --- | --- |
| `↑` `↓` or `j` `k` | Move |
| `Enter` | Open |
| `1`–`8` | Open a screen from Home |
| `Esc` | Go back |
| `?` | Help for the current screen |
| `q` | Quit |

| # | Screen | What it does |
| --- | --- | --- |
| 1 | Disk | Browse folders by size. |
| 2 | Quick Clean | Find build folders, installers, Trash, Docker, and simulator space. |
| 3 | Deep Clean | Clear caches and logs, after you review every path. |
| 4 | Remove App | Remove an app and its leftover files. |
| 5 | Large Files | Find large or old files. |
| 6 | Startup | See what starts on its own, and turn off your own launch agents. |
| 7 | Dotfiles | Edit settings files with backups, and export them with chezmoi. |
| 8 | AI Tools | Browse assistant chats and skills, clean up transcripts, and edit local skills. |

Every cleanup shows what it will remove and asks first. Files go to the Trash, except Docker, simulator, and Empty Trash actions, which delete for good after a separate warning. See [SAFETY.md](docs/SAFETY.md).

## License

[MIT](https://github.com/ado11231/neet/blob/master/LICENSE-MIT) or [Apache 2.0](https://github.com/ado11231/neet/blob/master/LICENSE-APACHE), your choice.
