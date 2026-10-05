//! The settings files Dotfiles lists, and how each stands with chezmoi.
//! Everything here only reads. See Dotfiles in `docs/SAFETY.md`.
//!
//! chezmoi's templates and hooks can run any program, so neet never runs
//! chezmoi to learn about its files. It reads chezmoi's config and source
//! folder itself.

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::safety::CleanupRoots;

pub mod change;

/// Which program a file belongs to, in the order the screen shows them
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Shell,
    Git,
    Ssh,
    Editors,
    Terminal,
    Tools,
}

impl Group {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Shell => "Shell",
            Self::Git => "Git",
            Self::Ssh => "SSH",
            Self::Editors => "Editors",
            Self::Terminal => "Terminal",
            Self::Tools => "Tools",
        }
    }
}

/// How a file's syntax is checked before it is written. A check only reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Zsh,
    Bash,
    Fish,
    Git,
    Toml,
    None,
}

impl Syntax {
    /// The check, as the screen names it
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh -n",
            Self::Bash => "bash -n",
            Self::Fish => "fish --no-execute",
            Self::Git => "git config --list",
            Self::Toml => "TOML reader",
            Self::None => "none",
        }
    }
}

/// One file Dotfiles may list
#[derive(Debug, PartialEq, Eq)]
pub struct Known {
    /// Where it is, from the home folder
    pub path: &'static str,
    pub group: Group,
    pub syntax: Syntax,
}

const fn known(path: &'static str, group: Group, syntax: Syntax) -> Known {
    Known {
        path,
        group,
        syntax,
    }
}

/// Every file Dotfiles may list. Keep in step with the table in SAFETY.md.
pub const FILES: &[Known] = &[
    known(".zshrc", Group::Shell, Syntax::Zsh),
    known(".zprofile", Group::Shell, Syntax::Zsh),
    known(".zshenv", Group::Shell, Syntax::Zsh),
    known(".zlogin", Group::Shell, Syntax::Zsh),
    known(".zlogout", Group::Shell, Syntax::Zsh),
    known(".bashrc", Group::Shell, Syntax::Bash),
    known(".bash_profile", Group::Shell, Syntax::Bash),
    known(".profile", Group::Shell, Syntax::Bash),
    known(".inputrc", Group::Shell, Syntax::None),
    known(".config/fish/config.fish", Group::Shell, Syntax::Fish),
    known(".gitconfig", Group::Git, Syntax::Git),
    known(".config/git/config", Group::Git, Syntax::Git),
    known(".config/git/ignore", Group::Git, Syntax::None),
    known(".gitignore_global", Group::Git, Syntax::None),
    known(".ssh/config", Group::Ssh, Syntax::None),
    known(".vimrc", Group::Editors, Syntax::None),
    known(".config/nvim/init.lua", Group::Editors, Syntax::None),
    known(".config/nvim/init.vim", Group::Editors, Syntax::None),
    known(".nanorc", Group::Editors, Syntax::None),
    known(".editorconfig", Group::Editors, Syntax::None),
    known(".tmux.conf", Group::Terminal, Syntax::None),
    known(".config/tmux/tmux.conf", Group::Terminal, Syntax::None),
    known(".config/kitty/kitty.conf", Group::Terminal, Syntax::None),
    known(".config/ghostty/config", Group::Terminal, Syntax::None),
    known(
        ".config/alacritty/alacritty.toml",
        Group::Terminal,
        Syntax::Toml,
    ),
    known(".config/starship.toml", Group::Terminal, Syntax::Toml),
    known(".wezterm.lua", Group::Terminal, Syntax::None),
    known(".npmrc", Group::Tools, Syntax::None),
    known(".config/mise/config.toml", Group::Tools, Syntax::Toml),
    known(".config/gh/config.yml", Group::Tools, Syntax::None),
    known(".Brewfile", Group::Tools, Syntax::None),
];

/// Files that can hold a token. They start left out of exports, and their
/// preview is hidden.
const MAY_HOLD_SECRETS: &[&str] = &[".npmrc"];

/// The most of a file the preview reads
const PREVIEW_BYTES: u64 = 16 * 1024;

/// The largest file compared with its source file. Larger ones count as
/// differing, since no settings file should be this big.
const COMPARE_BYTES: u64 = 1024 * 1024;

/// A listed file that exists
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Its size, or the size of what its link leads to
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Its permission bits, such as `0o644`
    pub mode: u32,
    /// Where it really is, when it is a link
    pub link: Option<PathBuf>,
}

/// How chezmoi makes a file other than by copying its source file. These are
/// view only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Built {
    Template,
    Encrypted,
    Modify,
    Create,
    Symlink,
}

impl Built {
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Template => "chezmoi builds it from a template",
            Self::Encrypted => "chezmoi keeps it encrypted",
            Self::Modify => "chezmoi changes it with a script",
            Self::Create => "chezmoi only creates it when it is missing",
            Self::Symlink => "chezmoi makes it as a link",
        }
    }
}

/// Why a listed file can only be viewed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewOnly {
    /// `.ssh/config` waits for the SSH feature.
    SshConfig,
    /// A link that leads nowhere
    BrokenLink,
    /// A link that leads outside the home folder
    LinkLeadsOut,
    /// A link into a protected folder
    Protected,
    /// A folder or something else that is not a plain file
    NotAFile,
    /// neet cannot write it as you.
    NotWritable,
    Built(Built),
}

impl ViewOnly {
    #[must_use]
    pub fn describe(&self) -> &'static str {
        match self {
            Self::SshConfig => "Changing it comes with the SSH screen",
            Self::BrokenLink => "It is a link that leads nowhere",
            Self::LinkLeadsOut => "It is a link that leads outside your home folder",
            Self::Protected => "It is a link into a protected folder",
            Self::NotAFile => "It is not a plain file",
            Self::NotWritable => "neet cannot write it as you",
            Self::Built(built) => built.describe(),
        }
    }
}

/// How a listed file stands with chezmoi
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Managed {
    /// chezmoi has no source file for it.
    No,
    /// `.chezmoiignore` names it.
    Ignored,
    /// Its source file has the same contents.
    InSync(PathBuf),
    /// Its source file has other contents, or it is missing in the home folder.
    Differs(PathBuf),
    /// chezmoi makes it some other way.
    Built(PathBuf, Built),
}

impl Managed {
    #[must_use]
    pub fn source(&self) -> Option<&Path> {
        match self {
            Self::InSync(source) | Self::Differs(source) | Self::Built(source, _) => Some(source),
            Self::No | Self::Ignored => None,
        }
    }
}

/// One listed file, as it is now
#[derive(Debug, Clone)]
pub struct Dotfile {
    pub known: &'static Known,
    /// Its full path in the home folder
    pub path: PathBuf,
    /// `None` when it does not exist
    pub found: Option<Found>,
    pub view_only: Option<ViewOnly>,
    /// `None` when chezmoi is not in use
    pub managed: Option<Managed>,
    pub may_hold_secrets: bool,
}

/// chezmoi, when its source folder exists
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chezmoi {
    /// Where chezmoi keeps the source files
    pub source: PathBuf,
    /// Why neet may not run chezmoi, if it may not. neet still reads its
    /// folder.
    pub may_not_run: Option<String>,
}

/// Everything Dotfiles shows when it opens
#[derive(Debug, Clone)]
pub struct Listing {
    /// Every listed file, found or not, in the order of [`FILES`]
    pub files: Vec<Dotfile>,
    pub chezmoi: Option<Chezmoi>,
}

/// Lists every known file in `home`, and reads chezmoi's folder when there
/// is one. Changes nothing.
#[must_use]
pub fn list(home: &Path) -> Listing {
    let chezmoi = find_chezmoi(home, &config_folder(home), is_installed("chezmoi"));
    let ignored = chezmoi
        .as_ref()
        .map(|chezmoi| ignore_patterns(&chezmoi.source))
        .unwrap_or_default();
    let roots = CleanupRoots::new(home).ok();
    let real_home = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    let files = FILES
        .iter()
        .map(|known| {
            let path = home.join(known.path);
            let found = found(&path);
            let managed = chezmoi
                .as_ref()
                .map(|chezmoi| managed(&chezmoi.source, known.path, &path, &ignored));
            let view_only = found.as_ref().and_then(|_| {
                view_only(known, &path, &real_home, roots.as_ref(), managed.as_ref())
            });
            Dotfile {
                known,
                path,
                found,
                view_only,
                managed,
                may_hold_secrets: MAY_HOLD_SECRETS.contains(&known.path),
            }
        })
        .collect();
    Listing { files, chezmoi }
}

fn found(path: &Path) -> Option<Found> {
    let own = fs::symlink_metadata(path).ok()?;
    let link = own
        .file_type()
        .is_symlink()
        .then(|| fs::canonicalize(path).ok())
        .flatten();
    let metadata = if own.file_type().is_symlink() {
        fs::metadata(path).unwrap_or(own)
    } else {
        own
    };
    Some(Found {
        size: metadata.len(),
        modified: metadata.modified().ok(),
        mode: metadata.permissions().mode() & 0o7777,
        link,
    })
}

fn view_only(
    known: &Known,
    path: &Path,
    real_home: &Path,
    roots: Option<&CleanupRoots>,
    managed: Option<&Managed>,
) -> Option<ViewOnly> {
    if known.path == ".ssh/config" {
        return Some(ViewOnly::SshConfig);
    }
    let own = fs::symlink_metadata(path).ok()?;
    if own.file_type().is_symlink() {
        let Ok(target) = fs::canonicalize(path) else {
            return Some(ViewOnly::BrokenLink);
        };
        if !target.starts_with(real_home) {
            return Some(ViewOnly::LinkLeadsOut);
        }
        if roots.is_none_or(|roots| roots.protects(&target)) {
            return Some(ViewOnly::Protected);
        }
    }
    if !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Some(ViewOnly::NotAFile);
    }
    if rustix::fs::access(path, rustix::fs::Access::WRITE_OK).is_err() {
        return Some(ViewOnly::NotWritable);
    }
    if let Some(Managed::Built(_, built)) = managed {
        return Some(ViewOnly::Built(*built));
    }
    None
}

/// The start of a file, as text that is safe to draw. Hidden for files that
/// may hold a token.
///
/// # Errors
///
/// Returns an error if the file could not be read.
pub fn preview(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(PREVIEW_BYTES)
        .read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes)
        .replace('\t', "    ")
        .chars()
        .filter(|&character| character == '\n' || !character.is_control())
        .collect())
}

/// Whether a program by this name is in a folder on `PATH`
fn is_installed(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|folder| {
            fs::metadata(folder.join(program))
                .is_ok_and(|metadata| metadata.is_file() && metadata.mode() & 0o111 != 0)
        })
    })
}

/// Where chezmoi looks for its config file
fn config_folder(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
        .join("chezmoi")
}

/// chezmoi's config, as far as neet needs it
#[derive(Debug, Default, PartialEq, Eq)]
struct Config {
    source_dir: Option<String>,
    /// Why neet may not run chezmoi under this config
    refused: Option<String>,
}

fn read_config(folder: &Path) -> Config {
    let names: Vec<&str> = [
        "chezmoi.toml",
        "chezmoi.json",
        "chezmoi.yaml",
        "chezmoi.yml",
        "chezmoi.jsonc",
    ]
    .into_iter()
    .filter(|name| folder.join(name).exists())
    .collect();
    let unknown = |why: &str| Config {
        source_dir: None,
        refused: Some(why.to_string()),
    };
    let [name] = names.as_slice() else {
        return if names.is_empty() {
            Config::default()
        } else {
            unknown("chezmoi has more than one config file")
        };
    };
    let Ok(text) = fs::read_to_string(folder.join(name)) else {
        return unknown("chezmoi's config file could not be read");
    };
    let (source_dir, hooks) = match *name {
        "chezmoi.toml" => match text.parse::<toml::Table>() {
            Ok(table) => (
                table
                    .get("sourceDir")
                    .and_then(toml::Value::as_str)
                    .map(str::to_string),
                table.contains_key("hooks"),
            ),
            Err(_) => return unknown("chezmoi's config file could not be read"),
        },
        "chezmoi.json" => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => (
                value
                    .get("sourceDir")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                value.get("hooks").is_some(),
            ),
            Err(_) => return unknown("chezmoi's config file could not be read"),
        },
        _ => return unknown("neet cannot read chezmoi's config file in this format"),
    };
    Config {
        source_dir,
        refused: hooks.then(|| "chezmoi's config has hooks, which run programs".to_string()),
    }
}

/// chezmoi's source folder and whether neet may run chezmoi, from its config
/// in `config_folder`, or `None` when there is no source folder.
fn find_chezmoi(home: &Path, config_folder: &Path, installed: bool) -> Option<Chezmoi> {
    let config = read_config(config_folder);
    let folder = match &config.source_dir {
        Some(dir) => match dir.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => home.join(dir),
        },
        None => home.join(".local/share/chezmoi"),
    };
    if !folder.is_dir() {
        return None;
    }
    let source = match fs::read_to_string(folder.join(".chezmoiroot")) {
        Ok(root) if !root.trim().is_empty() => folder.join(root.trim()),
        _ => folder,
    };
    let may_not_run = if installed {
        config.refused.or_else(|| unsafe_special_file(&source))
    } else {
        Some("chezmoi is not installed".to_string())
    };
    Some(Chezmoi {
        source,
        may_not_run,
    })
}

/// The first special file chezmoi renders on almost every command whose
/// template could run a program, as a reason
fn unsafe_special_file(source: &Path) -> Option<String> {
    let mut folders = vec![source.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        let in_externals = folder
            .file_name()
            .is_some_and(|name| name == ".chezmoiexternals");
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if name != ".git" {
                    folders.push(entry.path());
                }
                continue;
            }
            let special = in_externals
                || [".chezmoiignore", ".chezmoiremove", ".chezmoiexternal"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix));
            if special
                && !fs::read_to_string(entry.path()).is_ok_and(|text| only_safe_actions(&text))
            {
                let shown = entry.path();
                let shown = shown.strip_prefix(source).unwrap_or(&shown);
                return Some(format!(
                    "{} uses a template that could run a program",
                    shown.display()
                ));
            }
        }
    }
    None
}

/// Words a template action may use without being able to run a program
const SAFE_WORDS: &[&str] = &["if", "else", "end", "eq", "ne", "and", "or", "not"];

/// Whether every template action in `text` only compares values, so
/// rendering it cannot run a program or ask a password manager
fn only_safe_actions(text: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return false;
        };
        let action = after[..end]
            .trim_start_matches('-')
            .trim_end_matches('-')
            .trim();
        let comment = action.starts_with("/*") && action.ends_with("*/");
        if !comment && !safe_action(action) {
            return false;
        }
        rest = &after[end + 2..];
    }
    true
}

fn safe_action(action: &str) -> bool {
    let mut characters = action.chars().peekable();
    while let Some(&character) = characters.peek() {
        if character.is_whitespace() || character == '(' || character == ')' {
            characters.next();
        } else if character == '"' {
            characters.next();
            let mut closed = false;
            while let Some(inside) = characters.next() {
                match inside {
                    '\\' => {
                        characters.next();
                    }
                    '"' => {
                        closed = true;
                        break;
                    }
                    _ => {}
                }
            }
            if !closed {
                return false;
            }
        } else {
            let mut word = String::new();
            while let Some(&inside) = characters.peek() {
                if inside.is_whitespace() || "()\"".contains(inside) {
                    break;
                }
                word.push(inside);
                characters.next();
            }
            let field = word.starts_with('.')
                && word[1..]
                    .chars()
                    .all(|letter| letter.is_ascii_alphanumeric() || letter == '_' || letter == '.');
            let number = word.chars().all(|letter| letter.is_ascii_digit());
            if !(field || number || SAFE_WORDS.contains(&word.as_str())) {
                return false;
            }
        }
    }
    true
}

/// How a name in chezmoi's source folder maps to the name it makes
#[derive(Debug, PartialEq, Eq)]
struct SourceName {
    target: String,
    built: Option<Built>,
}

/// The target name of a file in chezmoi's source folder, or `None` for
/// chezmoi's own files and scripts.
fn file_target(name: &str) -> Option<SourceName> {
    if name.starts_with('.') {
        return None;
    }
    let mut rest = name;
    let mut built = None;
    for (prefix, kind) in [
        ("create_", Some(Built::Create)),
        ("modify_", Some(Built::Modify)),
        ("symlink_", Some(Built::Symlink)),
        ("remove_", None),
        ("run_", None),
    ] {
        if let Some(after) = rest.strip_prefix(prefix) {
            built = Some(kind?);
            rest = after;
            break;
        }
    }
    let encrypted = rest.starts_with("encrypted_");
    while let Some(after) = [
        "encrypted_",
        "private_",
        "readonly_",
        "empty_",
        "executable_",
    ]
    .iter()
    .find_map(|prefix| rest.strip_prefix(prefix))
    {
        rest = after;
    }
    let mut target = if let Some(after) = rest.strip_prefix("literal_") {
        after.to_string()
    } else if let Some(after) = rest.strip_prefix("dot_") {
        format!(".{after}")
    } else {
        rest.to_string()
    };
    if let Some(before) = target.strip_suffix(".literal") {
        target = before.to_string();
    } else {
        if encrypted {
            for ending in [".age", ".asc"] {
                if let Some(before) = target.strip_suffix(ending) {
                    target = before.to_string();
                }
            }
            built = built.or(Some(Built::Encrypted));
        }
        if let Some(before) = target.strip_suffix(".tmpl") {
            target = before.to_string();
            built = built.or(Some(Built::Template));
        }
    }
    Some(SourceName { target, built })
}

/// The target name of a folder in chezmoi's source folder
fn folder_target(name: &str) -> Option<String> {
    if name.starts_with('.') || name.starts_with("remove_") {
        return None;
    }
    let mut rest = name;
    while let Some(after) = ["external_", "exact_", "private_", "readonly_"]
        .iter()
        .find_map(|prefix| rest.strip_prefix(prefix))
    {
        rest = after;
    }
    Some(if let Some(after) = rest.strip_prefix("literal_") {
        after.to_string()
    } else if let Some(after) = rest.strip_prefix("dot_") {
        format!(".{after}")
    } else {
        rest.to_string()
    })
}

/// The source file chezmoi keeps for `target`, a path from the home folder,
/// with how chezmoi makes it
fn source_file(source: &Path, target: &str) -> Option<(PathBuf, Option<Built>)> {
    let parts: Vec<&str> = target.split('/').collect();
    let (file, folders) = parts.split_last()?;
    let mut folder = source.to_path_buf();
    for part in folders {
        folder = fs::read_dir(&folder).ok()?.flatten().find_map(|entry| {
            let name = entry.file_name();
            (entry.file_type().ok()?.is_dir()
                && folder_target(&name.to_string_lossy()).as_deref() == Some(*part))
            .then(|| entry.path())
        })?;
    }
    fs::read_dir(&folder).ok()?.flatten().find_map(|entry| {
        let name = file_target(&entry.file_name().to_string_lossy())?;
        (!entry.file_type().ok()?.is_dir() && name.target == *file)
            .then(|| (entry.path(), name.built))
    })
}

fn managed(source: &Path, target: &str, path: &Path, ignored: &[Pattern]) -> Managed {
    if is_ignored(ignored, target) {
        return Managed::Ignored;
    }
    let Some((file, built)) = source_file(source, target) else {
        return Managed::No;
    };
    if let Some(built) = built {
        return Managed::Built(file, built);
    }
    let read = |path: &Path| -> Option<Vec<u8>> {
        let metadata = fs::metadata(path).ok()?;
        (metadata.len() <= COMPARE_BYTES)
            .then(|| fs::read(path).ok())
            .flatten()
    };
    match (read(&file), read(path)) {
        (Some(want), Some(have)) if want == have => Managed::InSync(file),
        _ => Managed::Differs(file),
    }
}

/// One line of `.chezmoiignore`
#[derive(Debug, PartialEq, Eq)]
struct Pattern {
    glob: String,
    /// A line starting with `!` takes a name back out.
    keep: bool,
}

/// The patterns in `.chezmoiignore`. Template actions are left out, so a
/// pattern in any branch counts. That can only make neet treat a file as not
/// managed, never the other way.
fn ignore_patterns(source: &Path) -> Vec<Pattern> {
    let Ok(text) = fs::read_to_string(source.join(".chezmoiignore")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut plain = String::new();
            let mut rest = line;
            while let Some(start) = rest.find("{{") {
                plain.push_str(&rest[..start]);
                rest = rest[start..]
                    .find("}}")
                    .map_or("", |end| &rest[start + end + 2..]);
            }
            plain.push_str(rest);
            let plain = plain.split(" #").next().unwrap_or_default().trim();
            if plain.is_empty() || plain.starts_with('#') {
                return None;
            }
            Some(match plain.strip_prefix('!') {
                Some(glob) => Pattern {
                    glob: glob.trim().to_string(),
                    keep: true,
                },
                None => Pattern {
                    glob: plain.to_string(),
                    keep: false,
                },
            })
        })
        .collect()
}

/// Whether `.chezmoiignore` leaves `target` out. A `!` line wins.
fn is_ignored(patterns: &[Pattern], target: &str) -> bool {
    let matches = |pattern: &Pattern| {
        let glob: Vec<&str> = pattern.glob.trim_matches('/').split('/').collect();
        let path: Vec<&str> = target.split('/').collect();
        // A folder that is ignored takes everything in it along.
        (1..=path.len()).any(|end| glob_match(&glob, &path[..end]))
    };
    !patterns
        .iter()
        .any(|pattern| pattern.keep && matches(pattern))
        && patterns
            .iter()
            .any(|pattern| !pattern.keep && matches(pattern))
}

/// Matches path parts against glob parts, where `**` matches any number of
/// parts, and `*` and `?` match within one part.
fn glob_match(glob: &[&str], path: &[&str]) -> bool {
    match glob.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| glob_match(rest, &path[skip..])),
        Some((first, rest)) => path.split_first().is_some_and(|(name, others)| {
            part_match(first.as_bytes(), name.as_bytes()) && glob_match(rest, others)
        }),
    }
}

fn part_match(glob: &[u8], name: &[u8]) -> bool {
    match glob.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|skip| part_match(rest, &name[skip..])),
        Some((b'?', rest)) => !name.is_empty() && part_match(rest, &name[1..]),
        Some((want, rest)) => name.first() == Some(want) && part_match(rest, &name[1..]),
    }
}

/// The Git repository chezmoi's source folder is in, as last fetched
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repo {
    /// Such as `you/dotfiles` for GitHub, or the remote's address
    pub remote: Option<String>,
    pub branch: Option<String>,
    /// How many files have changes not yet committed
    pub uncommitted: usize,
    /// Commits here and not on the remote, and the other way, as last fetched
    pub ahead_behind: Option<(usize, usize)>,
}

/// Reads the Git repository `folder` is in. Runs only Git commands that
/// read, with nothing that could start another program. `None` when it is
/// not in a repository.
#[must_use]
pub fn repo(folder: &Path) -> Option<Repo> {
    let git = |args: &[&str]| -> Option<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(folder)
            .args(["-c", "core.fsmonitor=false", "--no-optional-locks"])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let status = git(&["status", "--porcelain"])?;
    let ahead_behind = git(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .and_then(|counts| {
            let (ahead, behind) = counts.split_once('\t')?;
            Some((ahead.parse().ok()?, behind.parse().ok()?))
        });
    Some(Repo {
        remote: git(&["config", "--get", "remote.origin.url"]).map(|url| short_remote(&url)),
        branch: git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        uncommitted: status.lines().filter(|line| !line.is_empty()).count(),
        ahead_behind,
    })
}

/// A GitHub address as `owner/name`, anything else as it is
fn short_remote(url: &str) -> String {
    [
        "https://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))
    .map_or_else(
        || url.to_string(),
        |rest| rest.trim_end_matches(".git").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn home() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap().join("home");
        fs::create_dir_all(&home).unwrap();
        (dir, home)
    }

    fn file<'a>(listing: &'a Listing, path: &str) -> &'a Dotfile {
        listing
            .files
            .iter()
            .find(|file| file.known.path == path)
            .unwrap()
    }

    #[test]
    fn source_names_map_to_the_files_they_make() {
        let plain = |target: &str| {
            Some(SourceName {
                target: target.to_string(),
                built: None,
            })
        };
        assert_eq!(file_target("dot_zshrc"), plain(".zshrc"));
        assert_eq!(file_target("private_executable_dot_x"), plain(".x"));
        assert_eq!(file_target("literal_dot_x"), plain("dot_x"));
        assert_eq!(file_target("dot_x.tmpl.literal"), plain(".x.tmpl"));
        assert_eq!(
            file_target("dot_gitconfig.tmpl").unwrap().built,
            Some(Built::Template)
        );
        assert_eq!(
            file_target("encrypted_private_dot_npmrc.age"),
            Some(SourceName {
                target: ".npmrc".to_string(),
                built: Some(Built::Encrypted),
            })
        );
        assert_eq!(
            file_target("modify_dot_x").unwrap().built,
            Some(Built::Modify)
        );
        assert_eq!(file_target("run_onchange_install.sh"), None);
        assert_eq!(file_target(".chezmoiignore"), None);
        assert_eq!(
            folder_target("private_dot_config").as_deref(),
            Some(".config")
        );
        assert_eq!(folder_target("exact_dot_vim").as_deref(), Some(".vim"));
        assert_eq!(folder_target(".git"), None);
    }

    #[test]
    fn templates_that_only_compare_values_are_safe() {
        assert!(only_safe_actions("README.md\n"));
        assert!(only_safe_actions(
            "{{ if ne .chezmoi.os \"darwin\" }}\n.config/x\n{{- end }}\n"
        ));
        assert!(only_safe_actions("{{/* a note */}}"));
        assert!(only_safe_actions(
            "{{ if and (eq .chezmoi.hostname \"mac\") .work }}{{ end }}"
        ));
        assert!(!only_safe_actions("{{ output \"sh\" \"-c\" \"id\" }}"));
        assert!(!only_safe_actions("{{ (onepasswordRead \"op://x\") }}"));
        assert!(!only_safe_actions("{{ include \"x\" }}"));
        assert!(!only_safe_actions("{{ $x := .y }}"));
        assert!(!only_safe_actions("{{ if .x }"));
        assert!(!only_safe_actions("{{ \"unclosed }}"));
    }

    #[test]
    fn ignore_patterns_match_like_chezmoi() {
        let patterns = ignore_patterns_from(
            "README.md\n.config/kitty\n*.txt # notes\n{{ if .x }}\n.vimrc\n{{ end }}\n!.config/kitty/kitty.conf\n",
        );
        assert!(is_ignored(&patterns, "README.md"));
        assert!(is_ignored(&patterns, ".vimrc"));
        assert!(is_ignored(&patterns, ".config/kitty/theme.conf"));
        assert!(!is_ignored(&patterns, ".config/kitty/kitty.conf"));
        assert!(is_ignored(&patterns, "a.txt"));
        assert!(!is_ignored(&patterns, ".zshrc"));
        assert!(glob_match(&["**", "*.conf"], &["a", "b", "c.conf"]));
        assert!(!glob_match(&["?.conf"], &["ab.conf"]));
    }

    fn ignore_patterns_from(text: &str) -> Vec<Pattern> {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".chezmoiignore"), text).unwrap();
        ignore_patterns(dir.path())
    }

    #[test]
    fn finds_the_source_folder_from_the_config() {
        let (_dir, home) = home();
        let config = home.join(".config/chezmoi");
        assert_eq!(find_chezmoi(&home, &config, true), None);

        fs::create_dir_all(home.join(".local/share/chezmoi")).unwrap();
        assert_eq!(
            find_chezmoi(&home, &config, true),
            Some(Chezmoi {
                source: home.join(".local/share/chezmoi"),
                may_not_run: None,
            })
        );

        write(&config.join("chezmoi.toml"), "sourceDir = \"~/dots\"\n");
        write(&home.join("dots/.chezmoiroot"), "mac\n");
        fs::create_dir_all(home.join("dots/mac")).unwrap();
        let found = find_chezmoi(&home, &config, true).unwrap();
        assert_eq!(found.source, home.join("dots/mac"));
        assert_eq!(found.may_not_run, None);

        let found = find_chezmoi(&home, &config, false).unwrap();
        assert_eq!(
            found.may_not_run.as_deref(),
            Some("chezmoi is not installed")
        );
    }

    #[test]
    fn hooks_and_unsafe_templates_stop_neet_running_chezmoi() {
        let (_dir, home) = home();
        let config = home.join(".config/chezmoi");
        let source = home.join(".local/share/chezmoi");
        write(
            &source.join(".chezmoiignore"),
            "{{ if ne .chezmoi.os \"darwin\" }}x{{ end }}\n",
        );
        assert_eq!(
            find_chezmoi(&home, &config, true).unwrap().may_not_run,
            None
        );

        write(
            &source.join("sub/.chezmoiexternal.toml.tmpl"),
            "{{ output \"id\" }}",
        );
        let reason = find_chezmoi(&home, &config, true)
            .unwrap()
            .may_not_run
            .unwrap();
        assert!(
            reason.contains("sub/.chezmoiexternal.toml.tmpl"),
            "{reason}"
        );
        fs::remove_file(source.join("sub/.chezmoiexternal.toml.tmpl")).unwrap();

        write(
            &config.join("chezmoi.toml"),
            "[hooks.read-source-state.pre]\ncommand = \"id\"\n",
        );
        let reason = find_chezmoi(&home, &config, true)
            .unwrap()
            .may_not_run
            .unwrap();
        assert!(reason.contains("hooks"));

        fs::remove_file(config.join("chezmoi.toml")).unwrap();
        write(&config.join("chezmoi.yaml"), "sourceDir: x\n");
        let reason = find_chezmoi(&home, &config, true)
            .unwrap()
            .may_not_run
            .unwrap();
        assert!(reason.contains("format"));
    }

    #[test]
    fn compares_each_file_with_its_source_file() {
        let (_dir, home) = home();
        let source = home.join("src");
        write(&source.join("dot_zshrc"), "same\n");
        write(&home.join(".zshrc"), "same\n");
        write(&source.join("private_dot_config/kitty/kitty.conf"), "new\n");
        write(&home.join(".config/kitty/kitty.conf"), "old\n");
        write(&source.join("dot_gitconfig.tmpl"), "{{ .name }}\n");
        write(&source.join(".chezmoiignore"), ".vimrc\n");
        write(&source.join("dot_vimrc"), "set x\n");

        let ignored = ignore_patterns(&source);
        let check = |target: &str| managed(&source, target, &home.join(target), &ignored);
        assert_eq!(check(".zshrc"), Managed::InSync(source.join("dot_zshrc")));
        assert_eq!(
            check(".config/kitty/kitty.conf"),
            Managed::Differs(source.join("private_dot_config/kitty/kitty.conf"))
        );
        assert_eq!(
            check(".gitconfig"),
            Managed::Built(source.join("dot_gitconfig.tmpl"), Built::Template)
        );
        assert_eq!(check(".vimrc"), Managed::Ignored);
        assert_eq!(check(".bashrc"), Managed::No);
    }

    #[test]
    fn lists_files_and_says_which_are_view_only() {
        let (dir, home) = home();
        write(&home.join(".zshrc"), "export A=1\n");
        write(&home.join(".ssh/config"), "Host x\n");
        write(&home.join(".npmrc"), "//registry.npmjs.org/:_authToken=x\n");
        let outside = dir.path().join("outside");
        write(&outside, "x");
        symlink(&outside, home.join(".vimrc")).unwrap();
        write(&home.join("dotfiles/tmux.conf"), "set -g mouse on\n");
        symlink(home.join("dotfiles/tmux.conf"), home.join(".tmux.conf")).unwrap();
        symlink(home.join("nowhere"), home.join(".nanorc")).unwrap();
        fs::create_dir_all(home.join(".inputrc")).unwrap();

        let listing = list(&home);

        assert_eq!(listing.files.len(), FILES.len());
        let zshrc = file(&listing, ".zshrc");
        assert_eq!(zshrc.found.as_ref().unwrap().size, 11);
        assert_eq!(zshrc.view_only, None);
        assert!(!zshrc.may_hold_secrets);
        assert_eq!(file(&listing, ".bashrc").found, None);
        assert_eq!(
            file(&listing, ".ssh/config").view_only,
            Some(ViewOnly::SshConfig)
        );
        assert!(file(&listing, ".npmrc").may_hold_secrets);
        assert_eq!(
            file(&listing, ".vimrc").view_only,
            Some(ViewOnly::LinkLeadsOut)
        );
        let tmux = file(&listing, ".tmux.conf");
        assert_eq!(tmux.view_only, None);
        assert_eq!(
            tmux.found.as_ref().unwrap().link.as_deref(),
            Some(home.join("dotfiles/tmux.conf").as_path())
        );
        assert_eq!(
            file(&listing, ".nanorc").view_only,
            Some(ViewOnly::BrokenLink)
        );
        assert_eq!(
            file(&listing, ".inputrc").view_only,
            Some(ViewOnly::NotAFile)
        );
    }

    #[test]
    fn previews_only_drawable_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x");
        fs::write(&path, "a\tb\u{1b}[31m\nc\n").unwrap();
        assert_eq!(preview(&path).unwrap(), "a    b[31m\nc\n");
    }

    #[test]
    fn github_remotes_are_shortened() {
        assert_eq!(
            short_remote("https://github.com/you/dotfiles.git"),
            "you/dotfiles"
        );
        assert_eq!(
            short_remote("git@github.com:you/dotfiles.git"),
            "you/dotfiles"
        );
        assert_eq!(
            short_remote("https://example.com/x.git"),
            "https://example.com/x.git"
        );
    }

    #[test]
    fn reads_a_repository_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["remote", "add", "origin", "git@github.com:you/dotfiles.git"]);
        fs::write(dir.path().join("dot_zshrc"), "x").unwrap();

        let found = repo(dir.path()).unwrap();

        assert_eq!(found.remote.as_deref(), Some("you/dotfiles"));
        assert_eq!(found.uncommitted, 1);
        assert_eq!(found.ahead_behind, None);
        assert!(repo(&dir.path().join("missing")).is_none());
    }
}
