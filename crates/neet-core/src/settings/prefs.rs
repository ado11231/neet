//! neet's own preferences: the choices each screen starts with, saved in
//! `~/.config/neet/settings.toml`. They never skip a review or a check.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The settings file's format version
const VERSION: i64 = 1;

/// Disk's sort orders
pub const DISK_SORTS: [&str; 3] = ["size", "name", "items"];

/// AI Tools' tool filters
pub const AI_TOOLS: [&str; 5] = ["all", "claude", "codex", "gemini", "shared"];

/// Large Files' smallest sizes, in bytes
pub const LARGE_SIZES: [u64; 6] = [
    10_000_000,
    50_000_000,
    100_000_000,
    500_000_000,
    1_000_000_000,
    5_000_000_000,
];

/// Large Files' unchanged for filters, in days. `0` means any age.
pub const LARGE_AGES: [u32; 6] = [0, 30, 90, 180, 365, 730];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// One of [`DISK_SORTS`]
    pub disk_sort: String,
    /// One of [`LARGE_SIZES`]
    pub large_min_size: u64,
    /// One of [`LARGE_AGES`]
    pub large_unchanged_days: u32,
    /// One of [`AI_TOOLS`]
    pub ai_tool: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            disk_sort: "size".into(),
            large_min_size: 100_000_000,
            large_unchanged_days: 0,
            ai_tool: "all".into(),
        }
    }
}

impl Prefs {
    /// `~/.config/neet/settings.toml`
    #[must_use]
    pub fn path(home: &Path) -> PathBuf {
        home.join(".config/neet/settings.toml")
    }

    /// The saved preferences, from `HOME`. Problems fall back to defaults.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var_os("HOME").map_or_else(Self::default, |home| {
            Self::load(&Self::path(Path::new(&home))).0
        })
    }

    /// Reads the file at `path`. A missing file gives the defaults. Values
    /// neet does not know, or a broken file, fall back to the defaults with
    /// a warning, and keys from newer versions are ignored.
    #[must_use]
    pub fn load(path: &Path) -> (Self, Vec<String>) {
        let mut prefs = Self::default();
        let mut warnings = Vec::new();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return (prefs, warnings),
            Err(error) => {
                warnings.push(format!("{} could not be read: {error}", path.display()));
                return (prefs, warnings);
            }
        };
        let table: toml::Table = match text.parse() {
            Ok(table) => table,
            Err(error) => {
                warnings.push(format!(
                    "{} is not valid TOML, so defaults are used: {}",
                    path.display(),
                    error.message()
                ));
                return (prefs, warnings);
            }
        };
        if let Some(version) = table.get("version").and_then(toml::Value::as_integer)
            && version > VERSION
        {
            warnings.push(format!(
                "settings.toml is from a newer neet (version {version}). Known values are used."
            ));
        }
        let mut text_value = |section: &str, key: &str, allowed: &[&str], into: &mut String| {
            let Some(value) = table.get(section).and_then(|s| s.get(key)) else {
                return;
            };
            match value.as_str().filter(|value| allowed.contains(value)) {
                Some(value) => *into = value.to_string(),
                None => warnings.push(format!(
                    "{section}.{key} = {value} is not one of {}, so {into} is used",
                    allowed.join(", ")
                )),
            }
        };
        text_value("disk", "sort", &DISK_SORTS, &mut prefs.disk_sort);
        text_value("ai", "tool", &AI_TOOLS, &mut prefs.ai_tool);
        let large = table.get("large");
        if let Some(value) = large.and_then(|large| large.get("min_size")) {
            match value
                .as_integer()
                .and_then(|size| u64::try_from(size).ok())
                .filter(|size| LARGE_SIZES.contains(size))
            {
                Some(size) => prefs.large_min_size = size,
                None => warnings.push(format!(
                    "large.min_size = {value} is not one of neet's sizes, so the default is used"
                )),
            }
        }
        if let Some(value) = large.and_then(|large| large.get("unchanged_days")) {
            match value
                .as_integer()
                .and_then(|days| u32::try_from(days).ok())
                .filter(|days| LARGE_AGES.contains(days))
            {
                Some(days) => prefs.large_unchanged_days = days,
                None => warnings.push(format!(
                    "large.unchanged_days = {value} is not one of neet's ages, so the default is used"
                )),
            }
        }
        (prefs, warnings)
    }

    /// The file's text
    #[must_use]
    pub fn to_toml(&self) -> String {
        format!(
            "# neet preferences. Edit them in neet, Settings, Preferences.\nversion = {VERSION}\n\n[disk]\nsort = \"{}\"\n\n[large]\nmin_size = {}\nunchanged_days = {}\n\n[ai]\ntool = \"{}\"\n",
            self.disk_sort, self.large_min_size, self.large_unchanged_days, self.ai_tool
        )
    }

    /// Writes the file at `path`: a new copy beside it, then swapped in.
    ///
    /// # Errors
    /// Returns an error if the folder or file cannot be written.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        super::write_atomic(path, self.to_toml().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_give_defaults_and_saved_ones_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = Prefs::path(dir.path());
        assert_eq!(Prefs::load(&path), (Prefs::default(), Vec::new()));

        let prefs = Prefs {
            disk_sort: "name".into(),
            large_min_size: 1_000_000_000,
            large_unchanged_days: 365,
            ai_tool: "codex".into(),
        };
        prefs.save(&path).unwrap();

        assert_eq!(Prefs::load(&path), (prefs, Vec::new()));
    }

    #[test]
    fn invalid_values_and_files_fall_back_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "[disk]\nsort = \"color\"\n[large]\nmin_size = 7\nunchanged_days = 90\n[ai]\ntool = 3\n",
        )
        .unwrap();
        let (prefs, warnings) = Prefs::load(&path);
        assert_eq!(prefs.disk_sort, "size");
        assert_eq!(prefs.large_min_size, 100_000_000);
        assert_eq!(prefs.large_unchanged_days, 90);
        assert_eq!(prefs.ai_tool, "all");
        assert_eq!(warnings.len(), 3, "{warnings:?}");

        fs::write(&path, "not = [valid").unwrap();
        let (prefs, warnings) = Prefs::load(&path);
        assert_eq!(prefs, Prefs::default());
        assert!(warnings[0].contains("not valid TOML"));
    }

    #[test]
    fn newer_and_older_files_keep_what_neet_knows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "version = 9\ntheme = \"dark\"\n[disk]\nsort = \"items\"\nfuture = true\n",
        )
        .unwrap();
        let (prefs, warnings) = Prefs::load(&path);
        assert_eq!(prefs.disk_sort, "items");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("newer neet"));

        // An old file with no version and only some keys
        fs::write(&path, "[ai]\ntool = \"gemini\"\n").unwrap();
        let (prefs, warnings) = Prefs::load(&path);
        assert_eq!(prefs.ai_tool, "gemini");
        assert_eq!(prefs.disk_sort, "size");
        assert_eq!(warnings, Vec::<String>::new());
    }
}
