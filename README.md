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
* Download the latest release with the installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ado11231/neet/releases/latest/download/neet-installer.sh | sh
```

Or install the published crate with [Rust](https://rustup.rs):

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

* Scans your home folder in the background.
* Arrow keys move, `Enter` opens, `Esc` goes back, `?` shows help, and `q` quits.
* The table below describes the current code. Published releases may have fewer features.

| Screen | Purpose |
| --- | --- |
| Disk | Browse folders by size. |
| Quick Clean | Review build folders, installers, Trash, Docker, and simulators. |
| Deep Clean | Review cache and log cleanup rules. |
| Remove App | Review an app and its leftover files. |
| Large Files | Find large or old files. |
| Startup | List startup items and review changes to your own launch agents. |
| Dotfiles | Edit settings with backups, or review a Git export. |
| AI Tools | Browse chats and skills, review transcript cleanup, and edit local skills. |

## License

[MIT](https://github.com/ado11231/neet/blob/master/LICENSE-MIT) or [Apache 2.0](https://github.com/ado11231/neet/blob/master/LICENSE-APACHE), your choice.
