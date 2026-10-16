//! Skills that come with installed plugins: Claude Code plugins, Codex
//! plugins, and Gemini CLI extensions. Each tool manages its own plugins, so
//! their skills are view only.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Plugin, Provider, Scanner, Source, read_bounded};

/// The most a plugin record or settings file may be, to read it
const RECORD_BYTES: u64 = 1024 * 1024;

impl Scanner<'_> {
    pub(super) fn plugins(&mut self) {
        self.claude_plugins();
        self.codex_plugins();
        self.gemini_extensions();
    }

    /// A small JSON file inside home, or `None` when it is missing, too
    /// large, or not JSON. Problems other than a missing file are noted.
    fn json(&mut self, path: &Path) -> Option<Value> {
        if !path.exists() {
            return None;
        }
        match read_bounded(&self.locations.home, path, RECORD_BYTES) {
            Ok((bytes, false)) => match serde_json::from_slice(&bytes) {
                Ok(value) => Some(value),
                Err(error) => {
                    self.listing
                        .warnings
                        .push(format!("{}: {error}", path.display()));
                    None
                }
            },
            Ok((_, true)) => {
                self.listing
                    .warnings
                    .push(format!("{}: larger than 1 MiB, not read", path.display()));
                None
            }
            Err(error) => {
                self.listing
                    .warnings
                    .push(format!("{}: {error}", path.display()));
                None
            }
        }
    }

    /// Each skill folder in `root`'s `skills` folder, one level down
    fn plugin_skills(&mut self, root: &Path, provider: Provider, plugin: &Plugin) {
        let scope = format!("Plugin: {}", plugin.name);
        for folder in self.children(&root.join("skills")) {
            if std::fs::symlink_metadata(&folder).is_ok_and(|meta| meta.is_dir()) {
                self.add_skill(
                    folder.join("SKILL.md"),
                    provider,
                    &scope,
                    Source::Plugin(plugin.clone()),
                );
            }
        }
    }

    /// Claude Code lists plugins in `plugins/installed_plugins.json`, and
    /// turns them on and off in `enabledPlugins` in its settings. Only that
    /// key of the settings is used.
    fn claude_plugins(&mut self) {
        let root = self.locations.claude.clone();
        let Some(installed) = self.json(&root.join("plugins/installed_plugins.json")) else {
            return;
        };
        let enabled = self
            .json(&root.join("settings.json"))
            .and_then(|settings| settings.get("enabledPlugins").cloned());
        for (key, plugin, path) in claude_installs(&installed) {
            let plugin = Plugin {
                enabled: enabled
                    .as_ref()
                    .and_then(|enabled| enabled.get(&key))
                    .and_then(Value::as_bool),
                ..plugin
            };
            self.plugin_skills(&path, Provider::Claude, &plugin);
        }
    }

    /// Codex keeps plugins in `plugins/cache/<marketplace>/<plugin>/<version>`.
    fn codex_plugins(&mut self) {
        let cache = self.locations.codex.join("plugins/cache");
        for marketplace in self.visible_folders(&cache) {
            for plugin in self.visible_folders(&marketplace) {
                for version in self.visible_folders(&plugin) {
                    let found = Plugin {
                        name: folder_name(&plugin),
                        marketplace: folder_name(&marketplace),
                        version: folder_name(&version),
                        enabled: None,
                    };
                    self.plugin_skills(&version, Provider::Codex, &found);
                }
            }
        }
    }

    /// Gemini CLI keeps extensions in `extensions/<name>`, with their
    /// version in `gemini-extension.json`.
    fn gemini_extensions(&mut self) {
        let extensions = self.locations.gemini.join("extensions");
        for extension in self.visible_folders(&extensions) {
            let version = self
                .json(&extension.join("gemini-extension.json"))
                .and_then(|manifest| manifest.get("version")?.as_str().map(str::to_owned))
                .unwrap_or_default();
            let found = Plugin {
                name: folder_name(&extension),
                marketplace: String::new(),
                version,
                enabled: None,
            };
            self.plugin_skills(&extension, Provider::Gemini, &found);
        }
    }

    /// Folders in `folder`, leaving out hidden ones and links
    fn visible_folders(&mut self, folder: &Path) -> Vec<PathBuf> {
        self.children(folder)
            .into_iter()
            .filter(|path| {
                !folder_name(path).starts_with('.')
                    && std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
            })
            .collect()
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Each installed Claude Code plugin: its `name@marketplace` key, what it
/// is, and where it was installed
fn claude_installs(installed: &Value) -> Vec<(String, Plugin, PathBuf)> {
    let Some(plugins) = installed.get("plugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (key, installs) in plugins {
        let (name, marketplace) = key.split_once('@').unwrap_or((key, ""));
        for install in installs.as_array().into_iter().flatten() {
            let Some(path) = install.get("installPath").and_then(Value::as_str) else {
                continue;
            };
            found.push((
                key.clone(),
                Plugin {
                    name: name.to_owned(),
                    marketplace: marketplace.to_owned(),
                    version: install
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    enabled: None,
                },
                PathBuf::from(path),
            ));
        }
    }
    found
}
