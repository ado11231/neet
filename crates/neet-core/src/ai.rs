//! Local assistant files, conversation summaries, and skill changes.

pub mod actions;
mod chat;
mod plugins;
pub mod remove;
pub mod skills;
pub mod summary;

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

const MAX_ENTRIES: usize = 20_000;
const PREVIEW_BYTES: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
    Shared,
}

impl Provider {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::Shared => "Shared",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Instructions,
    Settings,
    Skill,
    Chat,
}

impl Kind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Instructions => "Instructions",
            Self::Settings => "Settings",
            Self::Skill => "Skill",
            Self::Chat => "Chat",
        }
    }
}

/// An installed plugin that brings skills with it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    /// Where it was installed from, such as `claude-plugins-official`
    pub marketplace: String,
    pub version: String,
    /// Whether the tool has it turned on, when the tool records that
    pub enabled: Option<bool>,
}

/// Where a skill comes from
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A folder of its own in the skill folder: your copy
    Copy,
    /// A link to a folder elsewhere, which holds the skill. Removing the
    /// link leaves that folder alone.
    Link(PathBuf),
    /// Shipped with the tool
    Bundled,
    /// Part of an installed plugin, which the tool manages
    Plugin(Plugin),
}

impl Source {
    /// A word or two for lists
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Copy => "your copy".into(),
            Self::Link(_) => "link".into(),
            Self::Bundled => "bundled".into(),
            Self::Plugin(plugin) => format!("plugin {}", plugin.name),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub provider: Provider,
    pub kind: Kind,
    pub path: PathBuf,
    pub scope: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// View only: shipped with the tool, or part of a plugin
    pub bundled: bool,
    pub chat: Option<summary::Summary>,
    /// Where a skill comes from. `None` for other kinds.
    pub source: Option<Source>,
}

#[derive(Default)]
pub struct Listing {
    pub items: Vec<Item>,
    pub warnings: Vec<String>,
}

/// Explicit roots keep discovery away from credentials and databases.
#[derive(Clone)]
pub struct Locations {
    pub home: PathBuf,
    pub project: Option<PathBuf>,
    pub claude: PathBuf,
    pub codex: PathBuf,
    pub gemini: PathBuf,
}

impl Locations {
    #[must_use]
    pub fn new(home: PathBuf, project: Option<PathBuf>) -> Self {
        Self {
            claude: home.join(".claude"),
            codex: home.join(".codex"),
            gemini: home.join(".gemini"),
            home,
            project,
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut locations = Self::new(home, std::env::current_dir().ok());
        if let Some(path) = std::env::var_os("CODEX_HOME") {
            locations.codex = PathBuf::from(path);
        }
        if let Some(path) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            locations.claude = PathBuf::from(path);
        }
        locations
    }

    #[must_use]
    pub fn skill_root(&self, provider: Provider) -> PathBuf {
        match provider {
            Provider::Claude => self.claude.join("skills"),
            Provider::Codex | Provider::Shared => self.home.join(".agents/skills"),
            Provider::Gemini => self.gemini.join("skills"),
        }
    }

    #[must_use]
    pub fn list(&self) -> Listing {
        let mut scanner = Scanner {
            locations: self,
            listing: Listing::default(),
            visited: 0,
        };
        for (provider, root, instruction, settings) in [
            (Provider::Claude, &self.claude, "CLAUDE.md", "settings.json"),
            (Provider::Codex, &self.codex, "AGENTS.md", "config.toml"),
            (Provider::Gemini, &self.gemini, "GEMINI.md", "settings.json"),
        ] {
            scanner.add(
                root.join(instruction),
                provider,
                Kind::Instructions,
                "User",
                false,
            );
            scanner.add(root.join(settings), provider, Kind::Settings, "User", false);
            scanner.walk_skills(&root.join("skills"), provider, "User");
        }
        scanner.add(
            self.codex.join("AGENTS.override.md"),
            Provider::Codex,
            Kind::Instructions,
            "User",
            false,
        );
        scanner.walk_skills(&self.home.join(".agents/skills"), Provider::Shared, "User");
        scanner.plugins();
        scanner.walk_chats(&self.claude.join("projects"), Provider::Claude, 2);
        scanner.walk_chats(&self.codex.join("sessions"), Provider::Codex, 4);
        scanner.walk_chats(&self.codex.join("archived_sessions"), Provider::Codex, 1);
        scanner.walk_chats(&self.gemini.join("tmp"), Provider::Gemini, 3);
        if let Some(project) = &self.project {
            for folder in project.ancestors() {
                if folder == self.home || !folder.starts_with(&self.home) {
                    break;
                }
                scanner.project(folder);
                if folder.join(".git").exists() {
                    break;
                }
            }
        }
        scanner.listing.items.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| {
                    if a.kind == Kind::Chat {
                        b.modified.cmp(&a.modified)
                    } else {
                        a.provider.cmp(&b.provider)
                    }
                })
                // Your own skills first, then the many a plugin can bring
                .then_with(|| {
                    let plugin = |item: &Item| matches!(item.source, Some(Source::Plugin(_)));
                    plugin(a).cmp(&plugin(b))
                })
                .then_with(|| a.path.cmp(&b.path))
        });
        scanner.listing.items.dedup_by(|a, b| a.path == b.path);
        if scanner.visited >= MAX_ENTRIES {
            scanner
                .listing
                .warnings
                .push("Discovery stopped at 20,000 entries. The list is incomplete.".into());
        }
        scanner.listing
    }

    /// Reads a bounded preview only after a file is opened in the UI.
    ///
    /// # Errors
    /// Returns an error for links outside home, special files, or unreadable files.
    pub fn preview(&self, item: &Item) -> io::Result<String> {
        let (bytes, truncated) = read_bounded(&self.home, &item.path, PREVIEW_BYTES)?;
        let mut text = if item.kind == Kind::Chat {
            chat::render(item.provider, &bytes)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        if truncated {
            text.push_str("\n\n[Preview limited to the first 512 KiB.]");
        }
        Ok(display_text(&text))
    }
}

struct Scanner<'a> {
    locations: &'a Locations,
    listing: Listing,
    visited: usize,
}

impl Scanner<'_> {
    fn add(&mut self, path: PathBuf, provider: Provider, kind: Kind, scope: &str, bundled: bool) {
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            self.listing
                .warnings
                .push(format!("{}: file links are not opened", path.display()));
            return;
        }
        let path_exists = fs::symlink_metadata(&path).is_ok();
        match checked_path(&self.locations.home, &path).and_then(fs::metadata) {
            Ok(meta) if meta.is_file() => self.listing.items.push(Item {
                provider,
                kind,
                path,
                scope: scope.into(),
                size: meta.len(),
                modified: meta.modified().ok(),
                bundled,
                chat: None,
                source: None,
            }),
            Err(error) if path_exists || error.kind() != io::ErrorKind::NotFound => {
                self.listing
                    .warnings
                    .push(format!("{}: {error}", path.display()));
            }
            _ => {}
        }
    }

    fn children(&mut self, folder: &Path) -> Vec<PathBuf> {
        if self.visited >= MAX_ENTRIES {
            return Vec::new();
        }
        let entries = match checked_path(&self.locations.home, folder).and_then(fs::read_dir) {
            Ok(entries) => entries,
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    self.listing
                        .warnings
                        .push(format!("{}: {error}", folder.display()));
                }
                return Vec::new();
            }
        };
        let mut paths = Vec::new();
        for entry in entries.take(MAX_ENTRIES.saturating_sub(self.visited)) {
            self.visited += 1;
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(error) => self.listing.warnings.push(error.to_string()),
            }
        }
        paths.sort();
        paths
    }

    fn walk_skills(&mut self, root: &Path, provider: Provider, scope: &str) {
        for folder in self.children(root) {
            let Ok(meta) = fs::symlink_metadata(&folder) else {
                continue;
            };
            if meta.is_file() {
                continue;
            }
            let bundled = folder.file_name().is_some_and(|name| name == ".system");
            if bundled {
                for system in self.children(&folder) {
                    // Loose files, such as the tool's own marker, are not skills.
                    if fs::symlink_metadata(&system).is_ok_and(|meta| meta.is_file()) {
                        continue;
                    }
                    self.add_skill(system.join("SKILL.md"), provider, scope, Source::Bundled);
                }
            } else if meta.file_type().is_symlink() {
                let target = fs::read_link(&folder).map_or_else(
                    |_| folder.clone(),
                    |target| {
                        folder
                            .parent()
                            .map_or(target.clone(), |parent| parent.join(target))
                    },
                );
                self.add_skill(
                    folder.join("SKILL.md"),
                    provider,
                    scope,
                    Source::Link(target),
                );
            } else {
                self.add_skill(folder.join("SKILL.md"), provider, scope, Source::Copy);
            }
        }
    }

    /// Adds a skill, marked with where it comes from. Bundled and plugin
    /// skills are view only.
    fn add_skill(&mut self, path: PathBuf, provider: Provider, scope: &str, source: Source) {
        let view_only = matches!(source, Source::Bundled | Source::Plugin(_));
        let before = self.listing.items.len();
        self.add(path, provider, Kind::Skill, scope, view_only);
        if self.listing.items.len() > before
            && let Some(item) = self.listing.items.last_mut()
        {
            item.source = Some(source);
        }
    }

    fn walk_chats(&mut self, root: &Path, provider: Provider, depth: usize) {
        if depth == 0 {
            return;
        }
        for path in self.children(root) {
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                self.walk_chats(&path, provider, depth - 1);
            } else if meta.is_file() {
                let extension = path.extension().and_then(|part| part.to_str());
                let allowed = match provider {
                    Provider::Claude => extension == Some("jsonl"),
                    Provider::Codex => {
                        extension == Some("jsonl")
                            && path
                                .file_name()
                                .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"))
                    }
                    Provider::Gemini => {
                        matches!(extension, Some("json" | "jsonl"))
                            && path
                                .parent()
                                .and_then(Path::file_name)
                                .is_some_and(|name| name == "chats")
                    }
                    Provider::Shared => false,
                };
                if allowed {
                    self.add(path, provider, Kind::Chat, "History", false);
                }
            }
        }
    }

    fn project(&mut self, root: &Path) {
        let scope = format!("Project: {}", root.display());
        for (provider, instruction, hidden, settings) in [
            (Provider::Claude, "CLAUDE.md", ".claude", "settings.json"),
            (Provider::Codex, "AGENTS.md", ".codex", "config.toml"),
            (Provider::Gemini, "GEMINI.md", ".gemini", "settings.json"),
        ] {
            self.add(
                root.join(instruction),
                provider,
                Kind::Instructions,
                &scope,
                false,
            );
            self.add(
                root.join(hidden).join(settings),
                provider,
                Kind::Settings,
                &scope,
                false,
            );
            self.walk_skills(&root.join(hidden).join("skills"), provider, &scope);
        }
        for name in ["CLAUDE.local.md", ".claude/CLAUDE.md"] {
            self.add(
                root.join(name),
                Provider::Claude,
                Kind::Instructions,
                &scope,
                false,
            );
        }
        self.add(
            root.join(".claude/settings.local.json"),
            Provider::Claude,
            Kind::Settings,
            &scope,
            false,
        );
        self.add(
            root.join("AGENTS.override.md"),
            Provider::Codex,
            Kind::Instructions,
            &scope,
            false,
        );
        self.walk_skills(&root.join(".agents/skills"), Provider::Shared, &scope);
    }
}

pub(super) fn checked_path(home: &Path, path: &Path) -> io::Result<PathBuf> {
    if !home.is_absolute()
        || !path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(io::Error::other(
            "an absolute path inside your home folder is required",
        ));
    }
    let home = fs::canonicalize(home)?;
    let resolved = fs::canonicalize(path)?;
    if !resolved.starts_with(&home) || resolved == home {
        return Err(io::Error::other("the path leads outside your home folder"));
    }
    Ok(resolved)
}

pub(super) fn read_bounded(home: &Path, path: &Path, limit: u64) -> io::Result<(Vec<u8>, bool)> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::other(
            "not a regular file; file links are not opened",
        ));
    }
    let path = checked_path(home, path)?;
    if !fs::metadata(&path)?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > limit;
    bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    Ok((bytes, truncated))
}

/// File contents and names must not inject terminal control sequences.
#[must_use]
pub fn display_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect::<String>()
        .replace('\t', "    ")
}

#[cfg(test)]
mod tests;
