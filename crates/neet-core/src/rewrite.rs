//! Changes a settings file in place: a backup first, then a new copy
//! swapped in, and only when the file did not change since it was opened.
//! See how a change runs, and backups, in `docs/SAFETY.md`.

use std::fmt;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The largest file neet opens to change. No settings file should be this
/// big.
pub const MAX_BYTES: u64 = 1024 * 1024;

/// A file as it was when it was opened, to change it later only if it is
/// still the same
#[derive(Debug, Clone)]
pub struct Opened {
    /// Where it really is. For a link, where the link leads, so the link
    /// itself stays.
    path: PathBuf,
    device: u64,
    inode: u64,
    modified: Option<SystemTime>,
    mode: u32,
    contents: Vec<u8>,
}

impl Opened {
    /// Reads the file at `path`, following a link to where it leads.
    ///
    /// # Errors
    ///
    /// Returns an error if it cannot be read, is not a plain file, or is
    /// larger than [`MAX_BYTES`].
    pub fn open(path: &Path) -> io::Result<Self> {
        let path = fs::canonicalize(path)?;
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "it is not a plain file",
            ));
        }
        if metadata.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "it is too large for a settings file",
            ));
        }
        let contents = fs::read(&path)?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            modified: metadata.modified().ok(),
            mode: metadata.permissions().mode() & 0o7777,
            path,
            contents,
        })
    }

    /// Where the file really is
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the file held when it was opened
    #[must_use]
    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    /// Whether the file is still the same one, unchanged: the same identity
    /// on disk, the same last change time, and the same contents
    fn is_unchanged(&self) -> io::Result<bool> {
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.is_file()
            || metadata.dev() != self.device
            || metadata.ino() != self.inode
            || metadata.modified().ok() != self.modified
            || metadata.len() != u64::try_from(self.contents.len()).unwrap_or(u64::MAX)
        {
            return Ok(false);
        }
        Ok(fs::read(&self.path)? == self.contents)
    }
}

/// Why a change was not written
#[derive(Debug)]
pub enum WriteError {
    /// The file changed after it was opened, so nothing was written.
    Changed,
    /// The backup or the write failed. The file is as it was.
    Io(io::Error),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed => write!(
                f,
                "it changed since you opened it, so nothing was written. Open it again"
            ),
            Self::Io(error) => write!(f, "{error}. The file is as it was"),
        }
    }
}

impl std::error::Error for WriteError {}

impl From<io::Error> for WriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// One saved copy of a file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub path: PathBuf,
    /// When it was saved, from its name
    pub saved: String,
    pub size: u64,
}

/// Where backups are kept, one folder per file, then one copy per change.
/// neet never removes a backup.
#[derive(Debug, Clone)]
pub struct Backups {
    root: PathBuf,
}

impl Backups {
    /// The backups of dotfiles, in `~/.local/state/neet/backups/dotfiles`
    #[must_use]
    pub fn dotfiles(home: &Path) -> Self {
        Self::at(home.join(".local/state/neet/backups/dotfiles"))
    }

    #[must_use]
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder holding the backups of `name`, a path such as
    /// `.config/kitty/kitty.conf`
    fn folder(&self, name: &str) -> io::Result<PathBuf> {
        let relative = Path::new(name);
        let plain = !name.is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !plain {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name:?} cannot name a backup"),
            ));
        }
        Ok(self.root.join(relative))
    }

    /// Saves `contents` as a new backup of `name`, with permissions `mode`,
    /// named by the time `now`. Only you can open the folders it makes.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is not a plain relative path, or the
    /// backup could not be written.
    pub fn save(
        &self,
        name: &str,
        contents: &[u8],
        mode: u32,
        now: SystemTime,
    ) -> io::Result<PathBuf> {
        let folder = self.folder(name)?;
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&folder)?;
        let stamp = stamp(now);
        let mut number = 1;
        loop {
            let path = if number == 1 {
                folder.join(&stamp)
            } else {
                folder.join(format!("{stamp}-{number}"))
            };
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(contents)?;
                    file.set_permissions(fs::Permissions::from_mode(mode))?;
                    file.sync_all()?;
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => number += 1,
                Err(error) => return Err(error),
            }
        }
    }

    /// The backups of `name`, newest first
    ///
    /// # Errors
    ///
    /// Returns an error if the name is not a plain relative path, or its
    /// folder could not be read. No folder means no backups.
    pub fn list(&self, name: &str) -> io::Result<Vec<Backup>> {
        let folder = self.folder(name)?;
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut backups: Vec<Backup> = entries
            .flatten()
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then(|| Backup {
                    path: entry.path(),
                    saved: entry.file_name().to_string_lossy().into_owned(),
                    size: metadata.len(),
                })
            })
            .collect();
        backups.sort_by(|a, b| sort_key(&b.saved).cmp(&sort_key(&a.saved)));
        Ok(backups)
    }
}

/// A backup's name in time order, so `-10` comes after `-9`
fn sort_key(saved: &str) -> (&str, u32) {
    match saved.rsplit_once('-') {
        Some((stamp, number)) if stamp.ends_with('Z') => (stamp, number.parse().unwrap_or(0)),
        _ => (saved, 1),
    }
}

/// `now` in UTC, as a name that sorts in time order, such as
/// `2026-10-05T16-30-12.123Z`
fn stamp(now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs();
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let rest = seconds % 86_400;
    // Days to a calendar date, from Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}.{:03}Z",
        rest / 3_600,
        rest / 60 % 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// Writes `contents` over the file `opened` came from, keeping its
/// permissions: checks it is unchanged, saves a backup of it as `name`, then
/// writes a new copy beside it and swaps it in. Returns the backup.
///
/// # Errors
///
/// Returns [`WriteError::Changed`] if the file changed since it was opened,
/// or [`WriteError::Io`] if the backup or the write failed. Either way the
/// file is as it was.
pub fn write(
    opened: &Opened,
    contents: &[u8],
    backups: &Backups,
    name: &str,
    now: SystemTime,
) -> Result<PathBuf, WriteError> {
    if !opened.is_unchanged()? {
        return Err(WriteError::Changed);
    }
    let backup = backups.save(name, &opened.contents, opened.mode, now)?;
    replace(&opened.path, contents, opened.mode)?;
    Ok(backup)
}

/// Puts `backup` back over the file at `path`. The file now is backed up
/// first, so the restore can be undone too. Returns that backup.
///
/// # Errors
///
/// As [`write`], and if either file cannot be opened.
pub fn restore(
    path: &Path,
    backup: &Backup,
    backups: &Backups,
    name: &str,
    now: SystemTime,
) -> Result<PathBuf, WriteError> {
    let opened = Opened::open(path)?;
    let contents = fs::read(&backup.path)?;
    write(&opened, &contents, backups, name, now)
}

/// Writes `contents` to a new file beside `path`, flushes it to disk, then
/// renames it over `path`, so a crash leaves the old file or the new one,
/// never half of one.
fn replace(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    let folder = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no folder"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no name"))?;
    let mut number = 0u32;
    let (temporary, mut file) = loop {
        let temporary = folder.join(format!(
            ".{}.neet-{}-{number}",
            name.to_string_lossy(),
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && number < 100 => {
                number += 1;
            }
            Err(error) => return Err(error),
        }
    };
    let written = file
        .write_all(contents)
        .and_then(|()| file.set_permissions(fs::Permissions::from_mode(mode)))
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    // The rename is only on disk once the folder is.
    fs::File::open(folder)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::time::Duration;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn setup() -> (tempfile::TempDir, PathBuf, Backups) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let file = root.join(".zshrc");
        fs::write(&file, "old\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let backups = Backups::at(root.join("state/backups"));
        (dir, file, backups)
    }

    #[test]
    fn names_backups_by_the_time_in_utc() {
        assert_eq!(stamp(at(0)), "1970-01-01T00-00-00.000Z");
        assert_eq!(stamp(at(1_759_681_812)), "2025-10-05T16-30-12.000Z");
        assert_eq!(stamp(at(951_782_400)), "2000-02-29T00-00-00.000Z");
        assert_eq!(
            stamp(UNIX_EPOCH + Duration::from_millis(4_102_444_799_999)),
            "2099-12-31T23-59-59.999Z"
        );
    }

    #[test]
    fn writes_after_a_backup_and_keeps_the_permissions() {
        let (_dir, file, backups) = setup();
        let opened = Opened::open(&file).unwrap();

        let backup = write(&opened, b"new\n", &backups, ".zshrc", at(100)).unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "old\n");
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode(&file), 0o640);
        assert_eq!(mode(&backup), 0o640);
        assert_eq!(mode(backup.parent().unwrap()), 0o700);
        let leftovers: Vec<_> = fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".neet-"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn refuses_a_file_that_changed_after_it_was_opened() {
        let (_dir, file, backups) = setup();
        let opened = Opened::open(&file).unwrap();
        fs::write(&file, "changed\n").unwrap();

        let result = write(&opened, b"new\n", &backups, ".zshrc", at(100));

        assert!(matches!(result, Err(WriteError::Changed)));
        assert_eq!(fs::read_to_string(&file).unwrap(), "changed\n");
        assert_eq!(backups.list(".zshrc").unwrap(), []);
    }

    #[test]
    fn refuses_a_file_replaced_after_it_was_opened() {
        let (_dir, file, backups) = setup();
        let opened = Opened::open(&file).unwrap();
        let other = file.with_file_name("other");
        fs::write(&other, "old\n").unwrap();
        fs::rename(&other, &file).unwrap();

        let result = write(&opened, b"new\n", &backups, ".zshrc", at(100));

        assert!(matches!(result, Err(WriteError::Changed)));
    }

    #[test]
    fn a_failed_write_leaves_the_file_as_it_was() {
        let (dir, _file, backups) = setup();
        let folder = dir.path().join("locked");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("config");
        fs::write(&file, "old\n").unwrap();
        let opened = Opened::open(&file).unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).unwrap();

        let result = write(&opened, b"new\n", &backups, "config", at(100));

        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(result, Err(WriteError::Io(_))));
        assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
    }

    #[test]
    fn writes_where_a_link_leads_and_keeps_the_link() {
        let (dir, _file, backups) = setup();
        let real = dir.path().join("dotfiles/tmux.conf");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "old\n").unwrap();
        let link = dir.path().join(".tmux.conf");
        symlink(&real, &link).unwrap();

        let opened = Opened::open(&link).unwrap();
        write(&opened, b"new\n", &backups, ".tmux.conf", at(100)).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new\n");
    }

    #[test]
    fn opens_only_plain_files_of_a_settings_size() {
        let (dir, _file, _backups) = setup();
        assert!(Opened::open(dir.path()).is_err());
        let large = dir.path().join("large");
        fs::write(&large, vec![b'x'; usize::try_from(MAX_BYTES).unwrap() + 1]).unwrap();
        assert!(Opened::open(&large).is_err());
        assert!(Opened::open(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn restores_a_backup_and_backs_up_the_file_first() {
        let (_dir, file, backups) = setup();
        let opened = Opened::open(&file).unwrap();
        write(&opened, b"new\n", &backups, ".zshrc", at(100)).unwrap();
        let saved = backups.list(".zshrc").unwrap();
        assert_eq!(saved.len(), 1);

        restore(&file, &saved[0], &backups, ".zshrc", at(200)).unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
        let saved = backups.list(".zshrc").unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(fs::read_to_string(&saved[0].path).unwrap(), "new\n");
        assert_eq!(fs::read_to_string(&saved[1].path).unwrap(), "old\n");
    }

    #[test]
    fn keeps_every_backup_newest_first_even_in_the_same_moment() {
        let (_dir, _file, backups) = setup();
        for text in ["a", "b", "c"] {
            backups
                .save(".config/kitty/kitty.conf", text.as_bytes(), 0o644, at(100))
                .unwrap();
        }
        backups
            .save(".config/kitty/kitty.conf", b"d", 0o644, at(200))
            .unwrap();

        let saved = backups.list(".config/kitty/kitty.conf").unwrap();

        let contents: Vec<String> = saved
            .iter()
            .map(|backup| fs::read_to_string(&backup.path).unwrap())
            .collect();
        assert_eq!(contents, ["d", "c", "b", "a"]);
        assert_eq!(backups.list(".bashrc").unwrap(), []);
    }

    #[test]
    fn backup_names_stay_inside_the_backup_folder() {
        let (_dir, _file, backups) = setup();
        for name in ["", "../x", "/etc/hosts", "a/../../x", "./x"] {
            assert!(backups.save(name, b"x", 0o644, at(1)).is_err(), "{name}");
        }
    }
}
