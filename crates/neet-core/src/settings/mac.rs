//! Mac settings that help on a Mac used from the terminal, read and written
//! with `defaults`. Each change is reviewed, the value from before neet's
//! first change is saved for undo, and reset deletes the key so macOS's own
//! default applies again.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::run::capture;

/// How long `defaults` and `killall` may take
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Finder,
    Typing,
    Dock,
    Screenshots,
    Saving,
}

impl Group {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Finder => "Finder",
            Self::Typing => "Keyboard & typing",
            Self::Dock => "Dock & Spaces",
            Self::Screenshots => "Screenshots",
            Self::Saving => "Saving & backups",
        }
    }
}

/// How `defaults` stores the value
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    Text,
}

impl Kind {
    fn flag(self) -> &'static str {
        match self {
            Self::Bool => "-bool",
            Self::Int => "-int",
            Self::Float => "-float",
            Self::Text => "-string",
        }
    }
}

/// The app that must restart to use a change
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    Finder,
    Dock,
    /// The menu bar, which handles screenshots
    SystemUiServer,
    /// Apps read it when they open, and some only after you log out
    Apps,
}

impl Restart {
    /// What the review says happens
    #[must_use]
    pub fn note(self) -> &'static str {
        match self {
            Self::Finder => "Finder restarts to use it. Its windows reopen.",
            Self::Dock => "The Dock restarts to use it.",
            Self::SystemUiServer => "The menu bar restarts to use it.",
            Self::Apps => {
                "Apps use it when they next open. Some keyboard settings need you to log out."
            }
        }
    }

    fn process(self) -> Option<&'static str> {
        match self {
            Self::Finder => Some("Finder"),
            Self::Dock => Some("Dock"),
            Self::SystemUiServer => Some("SystemUIServer"),
            Self::Apps => None,
        }
    }
}

/// One Mac setting neet can change
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setting {
    pub group: Group,
    pub name: &'static str,
    pub about: &'static str,
    /// The `defaults` domain, such as `com.apple.finder`
    pub domain: &'static str,
    pub key: &'static str,
    pub kind: Kind,
    /// What macOS uses when the key is not set, when it is known
    pub default: Option<&'static str>,
    /// Each choice's label and value, as `defaults` writes it
    pub choices: &'static [(&'static str, &'static str)],
    pub restart: Restart,
}

const ON_OFF: &[(&str, &str)] = &[("On", "true"), ("Off", "false")];

/// Every Mac setting neet offers, in the order shown
pub const SETTINGS: &[Setting] = &[
    Setting {
        group: Group::Finder,
        name: "Show hidden files",
        about: "Finder shows files and folders whose names start with a dot, such as .zshrc. Cmd-Shift-. switches this too.",
        domain: "com.apple.finder",
        key: "AppleShowAllFiles",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Show all file extensions",
        about: "Every file name ends with its extension, such as .sh or .json, so you can tell files apart.",
        domain: "NSGlobalDomain",
        key: "AppleShowAllExtensions",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Show path bar",
        about: "A bar at the bottom of Finder windows shows the folder path. Double click a folder in it to go there.",
        domain: "com.apple.finder",
        key: "ShowPathbar",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Show status bar",
        about: "A bar at the bottom of Finder windows shows item counts and free space.",
        domain: "com.apple.finder",
        key: "ShowStatusBar",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Full path in window title",
        about: "Finder window titles show the whole path, such as /Users/you/code, not just the folder name.",
        domain: "com.apple.finder",
        key: "_FXShowPosixPathInTitle",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Folders first",
        about: "When sorting by name, folders come before files, as in ls --group-directories-first.",
        domain: "com.apple.finder",
        key: "_FXSortFoldersFirst",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Search this folder",
        about: "Finder searches the current folder by default, rather than the whole Mac.",
        domain: "com.apple.finder",
        key: "FXDefaultSearchScope",
        kind: Kind::Text,
        default: Some("SCev"),
        choices: &[("This Mac", "SCev"), ("This folder", "SCcf")],
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Warn when changing an extension",
        about: "Finder asks before you change a file's extension.",
        domain: "com.apple.finder",
        key: "FXEnableExtensionChangeWarning",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "No .DS_Store on network drives",
        about: "Finder stops leaving .DS_Store files in shared folders, where they show up in other people's repos.",
        domain: "com.apple.desktopservices",
        key: "DSDontWriteNetworkStores",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "No .DS_Store on USB drives",
        about: "Finder stops leaving .DS_Store files on USB drives and SD cards.",
        domain: "com.apple.desktopservices",
        key: "DSDontWriteUSBStores",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Finder,
        name: "Show desktop icons",
        about: "Off hides files on the desktop. They stay in ~/Desktop.",
        domain: "com.apple.finder",
        key: "CreateDesktop",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Finder,
    },
    Setting {
        group: Group::Typing,
        name: "Hold a key to repeat it",
        about: "Holding a key repeats it, as vim and other terminal editors expect. On shows the accent menu instead.",
        domain: "NSGlobalDomain",
        key: "ApplePressAndHoldEnabled",
        kind: Kind::Bool,
        default: Some("true"),
        choices: &[("Accent menu", "true"), ("Repeat", "false")],
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Key repeat rate",
        about: "How fast a held key repeats. Lower is faster; System Settings goes down to 2.",
        domain: "NSGlobalDomain",
        key: "KeyRepeat",
        kind: Kind::Int,
        default: None,
        choices: &[
            ("Fastest", "2"),
            ("Fast", "6"),
            ("Medium", "12"),
            ("Slow", "30"),
        ],
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Delay until repeat",
        about: "How long you hold a key before it repeats. Lower is shorter.",
        domain: "NSGlobalDomain",
        key: "InitialKeyRepeat",
        kind: Kind::Int,
        default: None,
        choices: &[
            ("Shortest", "15"),
            ("Short", "25"),
            ("Medium", "35"),
            ("Long", "68"),
        ],
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Smart quotes",
        about: "Turns straight quotes into curly ones, which breaks code and commands pasted from notes.",
        domain: "NSGlobalDomain",
        key: "NSAutomaticQuoteSubstitutionEnabled",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Smart dashes",
        about: "Turns -- into an em dash, which breaks command options typed in apps.",
        domain: "NSGlobalDomain",
        key: "NSAutomaticDashSubstitutionEnabled",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Auto-correct",
        about: "Corrects spelling as you type, including in file names and code.",
        domain: "NSGlobalDomain",
        key: "NSAutomaticSpellingCorrectionEnabled",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Auto-capitalize",
        about: "Capitalizes the first word of a sentence.",
        domain: "NSGlobalDomain",
        key: "NSAutomaticCapitalizationEnabled",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Typing,
        name: "Period with double space",
        about: "Two spaces type a period and a space.",
        domain: "NSGlobalDomain",
        key: "NSAutomaticPeriodSubstitutionEnabled",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Dock,
        name: "Hide the Dock",
        about: "The Dock hides until you move the pointer to it, leaving more room for windows.",
        domain: "com.apple.dock",
        key: "autohide",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Dock,
    },
    Setting {
        group: Group::Dock,
        name: "Dock show delay",
        about: "Seconds before a hidden Dock appears.",
        domain: "com.apple.dock",
        key: "autohide-delay",
        kind: Kind::Float,
        default: None,
        choices: &[("None", "0"), ("Short", "0.2"), ("Normal", "0.5")],
        restart: Restart::Dock,
    },
    Setting {
        group: Group::Dock,
        name: "Recent apps in Dock",
        about: "The Dock shows apps you opened recently that are not kept in it.",
        domain: "com.apple.dock",
        key: "show-recents",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Dock,
    },
    Setting {
        group: Group::Dock,
        name: "Minimize into app icon",
        about: "Minimized windows go into their app's icon instead of filling the Dock.",
        domain: "com.apple.dock",
        key: "minimize-to-application",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Dock,
    },
    Setting {
        group: Group::Dock,
        name: "Rearrange Spaces",
        about: "Spaces reorder by recent use. Off keeps each desktop where you put it.",
        domain: "com.apple.dock",
        key: "mru-spaces",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Dock,
    },
    Setting {
        group: Group::Screenshots,
        name: "Save screenshots to",
        about: "Where Cmd-Shift-3 and Cmd-Shift-4 save. neet makes the folder if it is missing.",
        domain: "com.apple.screencapture",
        key: "location",
        kind: Kind::Text,
        default: Some("~/Desktop"),
        choices: &[
            ("Desktop", "~/Desktop"),
            ("Downloads", "~/Downloads"),
            ("Pictures/Screenshots", "~/Pictures/Screenshots"),
        ],
        restart: Restart::SystemUiServer,
    },
    Setting {
        group: Group::Screenshots,
        name: "Screenshot format",
        about: "The file type screenshots save as. JPG files are smaller.",
        domain: "com.apple.screencapture",
        key: "type",
        kind: Kind::Text,
        default: Some("png"),
        choices: &[("PNG", "png"), ("JPG", "jpg"), ("HEIC", "heic")],
        restart: Restart::SystemUiServer,
    },
    Setting {
        group: Group::Screenshots,
        name: "Window shadow",
        about: "Window screenshots include the shadow around the window.",
        domain: "com.apple.screencapture",
        key: "disable-shadow",
        kind: Kind::Bool,
        default: Some("false"),
        choices: &[("Shadow", "false"), ("No shadow", "true")],
        restart: Restart::SystemUiServer,
    },
    Setting {
        group: Group::Screenshots,
        name: "Floating thumbnail",
        about: "A preview shows in the corner after a screenshot, and the file saves a few seconds later.",
        domain: "com.apple.screencapture",
        key: "show-thumbnail",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::SystemUiServer,
    },
    Setting {
        group: Group::Saving,
        name: "Expanded save dialogs",
        about: "Save dialogs open with the full folder browser instead of the short form.",
        domain: "NSGlobalDomain",
        key: "NSNavPanelExpandedStateForSaveMode",
        kind: Kind::Bool,
        default: Some("false"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Saving,
        name: "Save new files to iCloud",
        about: "New documents save to iCloud Drive by default. Off saves them on this Mac.",
        domain: "NSGlobalDomain",
        key: "NSDocumentSaveNewDocumentsToCloud",
        kind: Kind::Bool,
        default: Some("true"),
        choices: ON_OFF,
        restart: Restart::Apps,
    },
    Setting {
        group: Group::Saving,
        name: "Ask to back up new disks",
        about: "Time Machine asks whether to use each new disk you plug in for backups.",
        domain: "com.apple.TimeMachine",
        key: "DoNotOfferNewDisksForBackup",
        kind: Kind::Bool,
        default: Some("false"),
        choices: &[("Ask", "false"), ("Don't ask", "true")],
        restart: Restart::Apps,
    },
];

/// A setting's value now
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Current {
    /// The key is not set, so macOS uses its default
    Unset,
    /// The value as `defaults` writes it, such as `true` or `0.5`
    Set(String),
}

impl Setting {
    /// The value macOS uses now, if known
    #[must_use]
    pub fn effective<'a>(&'a self, current: &'a Current) -> Option<&'a str> {
        match current {
            Current::Set(value) => Some(value),
            Current::Unset => self.default,
        }
    }

    /// The choice `value` matches, comparing numbers as numbers
    #[must_use]
    pub fn choice(&self, value: &str) -> Option<usize> {
        self.choices.iter().position(|(_, choice)| match self.kind {
            Kind::Float | Kind::Int => match (choice.parse::<f64>(), value.parse::<f64>()) {
                (Ok(a), Ok(b)) => (a - b).abs() < 1e-9,
                _ => false,
            },
            Kind::Bool | Kind::Text => *choice == value,
        })
    }

    /// The label for `current`, such as `On`, `Off (default)`, or the raw
    /// value when it is not one of the choices
    #[must_use]
    pub fn label(&self, current: &Current) -> String {
        let Some(value) = self.effective(current) else {
            return "macOS default".into();
        };
        let label = self.choice(value).map_or_else(
            || value.to_string(),
            |index| self.choices[index].0.to_string(),
        );
        if *current == Current::Unset {
            format!("{label} (default)")
        } else {
            label
        }
    }

    /// The choice after the current one, to change to with Enter
    #[must_use]
    pub fn next(&self, current: &Current) -> &'static str {
        let index = self
            .effective(current)
            .and_then(|value| self.choice(value))
            .map_or(0, |index| (index + 1) % self.choices.len());
        self.choices[index].1
    }
}

fn defaults(args: &[&str]) -> io::Result<crate::run::Captured> {
    capture(Command::new("/usr/bin/defaults").args(args), TIMEOUT)?
        .ok_or_else(|| io::Error::other("defaults took too long"))
}

/// Reads `key` in `domain`. A key that is not set is [`Current::Unset`].
fn read_at(domain: &str, key: &str, kind: Kind) -> io::Result<Current> {
    let captured = defaults(&["read", domain, key])?;
    if !captured.success {
        if captured.stderr.contains("does not exist") {
            return Ok(Current::Unset);
        }
        return Err(io::Error::other(captured.stderr.trim().to_string()));
    }
    let value = captured.stdout.trim().to_string();
    Ok(Current::Set(match (kind, value.as_str()) {
        (Kind::Bool, "1") => "true".into(),
        (Kind::Bool, "0") => "false".into(),
        _ => value,
    }))
}

/// Reads one setting with `defaults read`, which needs no admin rights.
///
/// # Errors
/// Returns an error if `defaults` failed for a reason other than an unset key.
pub fn read(setting: &Setting) -> io::Result<Current> {
    read_at(setting.domain, setting.key, setting.kind)
}

/// Reads every setting, keeping errors per setting
#[must_use]
pub fn read_all() -> Vec<io::Result<Current>> {
    SETTINGS.iter().map(read).collect()
}

/// A reviewed change: to a value, or back to macOS's default
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub index: usize,
    pub from: Current,
    /// `None` resets the key, so macOS's default applies
    pub to: Option<String>,
}

impl Change {
    /// A change of setting `index` from `from` to `to`.
    ///
    /// # Errors
    /// Refuses a value the setting does not offer.
    pub fn new(index: usize, from: Current, to: Option<&str>) -> io::Result<Self> {
        let setting = SETTINGS
            .get(index)
            .ok_or_else(|| io::Error::other("no such setting"))?;
        if let Some(to) = to
            && !setting.choices.iter().any(|(_, value)| *value == to)
        {
            return Err(io::Error::other("that value is not one neet offers"));
        }
        Ok(Self {
            index,
            from,
            to: to.map(str::to_string),
        })
    }

    #[must_use]
    pub fn setting(&self) -> &'static Setting {
        &SETTINGS[self.index]
    }

    /// The command, as it will run
    #[must_use]
    pub fn command(&self) -> String {
        let setting = self.setting();
        match &self.to {
            Some(to) => format!(
                "defaults write {} {} {} {to}",
                setting.domain,
                setting.key,
                setting.kind.flag()
            ),
            None => format!("defaults delete {} {}", setting.domain, setting.key),
        }
    }

    /// Writes the change if the setting is still as reviewed, saves the
    /// value from before neet's first change, then restarts the app that
    /// uses it.
    ///
    /// # Errors
    /// Returns an error if the setting changed since review, or `defaults`
    /// failed. Nothing is written then.
    pub fn apply(&self, home: &Path, saved: &mut Saved) -> io::Result<()> {
        let setting = self.setting();
        self.write_at(home, saved, setting.domain)?;
        if let Some(process) = setting.restart.process() {
            // Finder, the Dock, and the menu bar start again on their own.
            let _ = capture(Command::new("/usr/bin/killall").arg(process), TIMEOUT);
        }
        Ok(())
    }

    fn write_at(&self, home: &Path, saved: &mut Saved, domain: &str) -> io::Result<()> {
        let setting = self.setting();
        if read_at(domain, setting.key, setting.kind)? != self.from {
            return Err(io::Error::other(
                "the setting changed since you looked. Press r and try again",
            ));
        }
        if setting.key == "location"
            && let Some(folder) = self.to.as_deref().and_then(|to| to.strip_prefix("~/"))
        {
            std::fs::create_dir_all(home.join(folder))?;
        }
        saved.remember(home, setting, &self.from)?;
        let captured = match &self.to {
            Some(to) => defaults(&["write", domain, setting.key, setting.kind.flag(), to])?,
            None => defaults(&["delete", domain, setting.key])?,
        };
        if captured.success || (self.to.is_none() && captured.stderr.contains("not found")) {
            Ok(())
        } else {
            Err(io::Error::other(captured.stderr.trim().to_string()))
        }
    }
}

/// The values from before neet's first change to each Mac setting, saved
/// under `~/.local/state/neet/settings/mac.json`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// `domain key` → the value then
    pub before: BTreeMap<String, Current>,
}

fn saved_key(setting: &Setting) -> String {
    format!("{} {}", setting.domain, setting.key)
}

impl Saved {
    #[must_use]
    pub fn path(home: &Path) -> PathBuf {
        home.join(".local/state/neet/settings/mac.json")
    }

    /// Reads what was saved. A missing file is an empty record.
    ///
    /// # Errors
    /// Returns an error if the file exists but cannot be read.
    pub fn load(home: &Path) -> io::Result<Self> {
        super::load_json(&Self::path(home))
    }

    fn remember(&mut self, home: &Path, setting: &Setting, from: &Current) -> io::Result<()> {
        let key = saved_key(setting);
        if let std::collections::btree_map::Entry::Vacant(entry) = self.before.entry(key) {
            entry.insert(from.clone());
            super::save_json(&Self::path(home), self)?;
        }
        Ok(())
    }

    /// The value from before neet changed `setting`, if it did
    #[must_use]
    pub fn before(&self, setting: &Setting) -> Option<&Current> {
        self.before.get(&saved_key(setting))
    }

    /// The change that puts setting `index` back as it was before neet.
    #[must_use]
    pub fn undo(&self, index: usize, now: Current) -> Option<Change> {
        let setting = SETTINGS.get(index)?;
        let to = match self.before(setting)? {
            Current::Unset => None,
            Current::Set(value) => Some(value.clone()),
        };
        Some(Change {
            index,
            from: now,
            to,
        })
    }

    /// Forgets `setting` once it has been undone.
    ///
    /// # Errors
    /// Returns an error if the record cannot be written.
    pub fn forget(&mut self, home: &Path, setting: &Setting) -> io::Result<()> {
        if self.before.remove(&saved_key(setting)).is_some() {
            super::save_json(&Self::path(home), self)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(key: &str) -> usize {
        SETTINGS
            .iter()
            .position(|setting| setting.key == key)
            .unwrap()
    }

    #[test]
    fn every_setting_offers_its_default_and_valid_values() {
        for setting in SETTINGS {
            assert!(setting.choices.len() >= 2, "{}", setting.name);
            if let Some(default) = setting.default {
                assert!(setting.choice(default).is_some(), "{}", setting.name);
            }
            for (_, value) in setting.choices {
                match setting.kind {
                    Kind::Bool => assert!(matches!(*value, "true" | "false")),
                    Kind::Int => assert!(value.parse::<i64>().is_ok()),
                    Kind::Float => assert!(value.parse::<f64>().is_ok()),
                    Kind::Text => assert_ne!(*value, ""),
                }
            }
        }
    }

    #[test]
    fn labels_defaults_numbers_and_next_values() {
        let hidden = &SETTINGS[index("AppleShowAllFiles")];
        assert_eq!(hidden.label(&Current::Unset), "Off (default)");
        assert_eq!(hidden.label(&Current::Set("true".into())), "On");
        assert_eq!(hidden.next(&Current::Unset), "true");
        let delay = &SETTINGS[index("autohide-delay")];
        assert_eq!(delay.label(&Current::Set("0.20000".into())), "Short");
        assert_eq!(delay.label(&Current::Unset), "macOS default");
        assert_eq!(delay.label(&Current::Set("1.5".into())), "1.5");
        assert_eq!(delay.next(&Current::Unset), "0");
    }

    #[test]
    fn changes_name_their_command_and_refuse_other_values() {
        let change = Change::new(index("autohide"), Current::Unset, Some("true")).unwrap();
        assert_eq!(
            change.command(),
            "defaults write com.apple.dock autohide -bool true"
        );
        let reset = Change::new(index("autohide"), Current::Set("true".into()), None).unwrap();
        assert_eq!(reset.command(), "defaults delete com.apple.dock autohide");
        assert!(Change::new(index("autohide"), Current::Unset, Some("maybe")).is_err());
    }

    #[test]
    fn writes_with_defaults_saves_the_old_value_and_undo_restores_it() {
        let home = tempfile::tempdir().unwrap();
        // A plist of its own, so the test never touches real settings
        let domain = home.path().join("test.neet.settings");
        let domain = domain.to_str().unwrap();
        let mut saved = Saved::default();
        let hidden = index("AppleShowAllFiles");
        let setting = &SETTINGS[hidden];

        assert_eq!(
            read_at(domain, setting.key, setting.kind).unwrap(),
            Current::Unset
        );
        let change = Change::new(hidden, Current::Unset, Some("true")).unwrap();
        change.write_at(home.path(), &mut saved, domain).unwrap();
        assert_eq!(
            read_at(domain, setting.key, setting.kind).unwrap(),
            Current::Set("true".into())
        );

        // A second change keeps the value from before the first.
        let again = Change::new(hidden, Current::Set("true".into()), Some("false")).unwrap();
        again.write_at(home.path(), &mut saved, domain).unwrap();
        assert_eq!(
            Saved::load(home.path()).unwrap().before(setting),
            Some(&Current::Unset)
        );

        // A change reviewed against an old value is refused.
        let stale = Change::new(hidden, Current::Unset, Some("true")).unwrap();
        assert!(stale.write_at(home.path(), &mut saved, domain).is_err());

        let undo = saved.undo(hidden, Current::Set("false".into())).unwrap();
        assert_eq!(undo.to, None);
        undo.write_at(home.path(), &mut saved, domain).unwrap();
        assert_eq!(
            read_at(domain, setting.key, setting.kind).unwrap(),
            Current::Unset
        );
        saved.forget(home.path(), setting).unwrap();
        assert!(Saved::load(home.path()).unwrap().before.is_empty());
    }

    #[test]
    fn writes_numbers_and_makes_the_screenshot_folder() {
        let home = tempfile::tempdir().unwrap();
        let domain = home.path().join("test.neet.settings");
        let domain = domain.to_str().unwrap();
        let mut saved = Saved::default();
        let delay = index("autohide-delay");
        Change::new(delay, Current::Unset, Some("0.2"))
            .unwrap()
            .write_at(home.path(), &mut saved, domain)
            .unwrap();
        let Current::Set(value) = read_at(domain, "autohide-delay", Kind::Float).unwrap() else {
            panic!("the delay should be set");
        };
        assert_eq!(SETTINGS[delay].choice(&value), Some(1));

        let location = index("location");
        Change::new(location, Current::Unset, Some("~/Pictures/Screenshots"))
            .unwrap()
            .write_at(home.path(), &mut saved, domain)
            .unwrap();
        assert!(home.path().join("Pictures/Screenshots").is_dir());
    }
}
