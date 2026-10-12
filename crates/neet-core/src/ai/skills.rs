//! Review local skills before linking them, editing them, or restoring a backup.

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use super::{Item, Kind, Locations, Provider, checked_path, read_bounded};
use crate::rewrite::{self, Backups, Opened};

fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

/// Checks the required frontmatter fields without running any skill code.
///
/// # Errors
/// Returns an error when the skill has no name, description, or Markdown body.
pub fn validate(contents: &[u8]) -> io::Result<()> {
    let text = std::str::from_utf8(contents).map_err(|_| invalid("SKILL.md must be UTF-8 text"))?;
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(invalid("SKILL.md must start with --- frontmatter"));
    }
    let mut name = false;
    let mut description = false;
    let mut closed = false;
    for line in lines.by_ref() {
        if line == "---" {
            closed = true;
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = !value.trim().trim_matches(['\'', '"']).is_empty();
        }
        if let Some(value) = line.strip_prefix("description:") {
            description = !value.trim().trim_matches(['\'', '"']).is_empty();
        }
    }
    if !closed || !name || !description || !lines.any(|line| !line.trim().is_empty()) {
        return Err(invalid(
            "SKILL.md needs a name, description, closing ---, and instructions",
        ));
    }
    Ok(())
}

fn open_skill(home: &Path, path: &Path) -> io::Result<Opened> {
    let resolved = checked_path(home, path)?;
    if resolved.file_name().is_none_or(|name| name != "SKILL.md") {
        return Err(invalid("the skill must resolve to a SKILL.md file"));
    }
    let (bytes, truncated) = read_bounded(home, path, rewrite::MAX_BYTES)?;
    if truncated {
        return Err(invalid("SKILL.md exceeds 1 MiB"));
    }
    validate(&bytes)?;
    Opened::open(path)
}

/// Makes private state folders without following directory links.
fn private_folders(home: &Path, folder: &Path) -> io::Result<()> {
    let relative = folder
        .strip_prefix(home)
        .map_err(|_| invalid("folder is outside home"))?;
    let mut path = fs::canonicalize(home)?;
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err(invalid("invalid folder path"));
        };
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(invalid(
                    "a state or install folder is a link or is not a directory",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(&path)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub struct Install {
    home: PathBuf,
    source: PathBuf,
    opened: Opened,
    destination: PathBuf,
}

impl Install {
    /// Prepares a local directory link, keeping scripts and references together.
    ///
    /// # Errors
    /// Rejects invalid skills, sources outside home, and occupied destinations.
    pub fn prepare(locations: &Locations, source: &Path, provider: Provider) -> io::Result<Self> {
        let source = checked_path(&locations.home, source)?;
        let opened = open_skill(&locations.home, &source.join("SKILL.md"))?;
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid skill folder name"))?;
        if name.starts_with('.')
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(invalid(
                "use a skill folder name with letters, numbers, hyphens, or underscores",
            ));
        }
        let destination = locations.skill_root(provider).join(name);
        check_destination(&locations.home, &destination)?;
        Ok(Self {
            home: locations.home.clone(),
            source,
            opened,
            destination,
        })
    }

    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    #[must_use]
    pub fn contents(&self) -> &[u8] {
        self.opened.contents()
    }

    /// Links only after the reviewed instructions and destination are checked again.
    ///
    /// # Errors
    /// Returns an error if the source changed, the destination exists, or linking fails.
    pub fn apply(&self) -> io::Result<()> {
        checked_path(&self.home, &self.source)?;
        if !self.opened.is_unchanged()? {
            return Err(invalid("the skill changed after review; load it again"));
        }
        check_destination(&self.home, &self.destination)?;
        let parent = self
            .destination
            .parent()
            .ok_or_else(|| invalid("missing skill folder"))?;
        private_folders(&self.home, parent)?;
        symlink(&self.source, &self.destination)
    }
}

fn check_destination(home: &Path, destination: &Path) -> io::Result<()> {
    if !home.is_absolute()
        || !destination.starts_with(home)
        || destination
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(invalid("skills must be installed inside home"));
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(invalid(
                "that skill is already installed; no files were replaced",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    for ancestor in destination.ancestors().skip(1) {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.is_dir() => {
                if ancestor != home {
                    checked_path(home, ancestor)?;
                }
                break;
            }
            Ok(_) => {
                return Err(invalid(
                    "the destination folder is a link or is not a directory",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub struct Edit {
    home: PathBuf,
    original: PathBuf,
    opened: Opened,
    name: String,
    draft: tempfile::TempDir,
}

impl Edit {
    #[must_use]
    pub fn path(&self) -> &Path {
        self.opened.path()
    }

    /// Edits a private copy, leaving the installed skill alone until review.
    ///
    /// # Errors
    /// Rejects bundled skills, escaped paths, and invalid or oversized skill files.
    pub fn begin(locations: &Locations, item: &Item) -> io::Result<Self> {
        if item.kind != Kind::Skill || item.bundled {
            return Err(invalid("only your own SKILL.md files can be edited"));
        }
        let opened = open_skill(&locations.home, &item.path)?;
        if opened
            .path()
            .components()
            .any(|part| part.as_os_str() == ".system")
        {
            return Err(invalid("bundled skills are view only"));
        }
        let home = fs::canonicalize(&locations.home)?;
        let name = opened
            .path()
            .strip_prefix(&home)
            .map_err(|_| invalid("skill is outside home"))?
            .to_str()
            .ok_or_else(|| invalid("the skill path is not UTF-8"))?
            .to_owned();
        let draft = tempfile::Builder::new().prefix("neet-skill-").tempdir()?;
        fs::write(draft.path().join("SKILL.md"), opened.contents())?;
        Ok(Self {
            home,
            original: item.path.clone(),
            opened,
            name,
            draft,
        })
    }

    #[must_use]
    pub fn draft(&self) -> PathBuf {
        self.draft.path().join("SKILL.md")
    }

    #[must_use]
    pub fn contents(&self) -> &[u8] {
        self.opened.contents()
    }

    fn backups(&self) -> Backups {
        Backups::at(self.home.join(".local/state/neet/backups/ai"))
    }

    /// Reads the edited copy with a size cap and basic skill checks.
    ///
    /// # Errors
    /// Rejects invalid, oversized, linked, or nonregular drafts.
    pub fn edited(&self) -> io::Result<Vec<u8>> {
        let path = self.draft();
        if !fs::symlink_metadata(&path)?.is_file() {
            return Err(invalid("the draft is not a regular file"));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(rewrite::MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > rewrite::MAX_BYTES {
            return Err(invalid("SKILL.md exceeds 1 MiB"));
        }
        validate(&bytes)?;
        Ok(bytes)
    }

    /// Reads the latest backup for a restore review.
    ///
    /// # Errors
    /// Returns an error if there is no backup or the backup cannot be read.
    pub fn latest_backup(&self) -> io::Result<Vec<u8>> {
        let backup = self
            .backups()
            .list(&self.name)?
            .into_iter()
            .next()
            .ok_or_else(|| invalid("no backup for this skill yet"))?;
        let (bytes, truncated) = read_bounded(&self.home, &backup.path, rewrite::MAX_BYTES)?;
        if truncated {
            return Err(invalid("the backup exceeds 1 MiB"));
        }
        validate(&bytes)?;
        Ok(bytes)
    }

    /// Backs up the original before replacing it, and refuses stale reviews.
    ///
    /// # Errors
    /// Returns an error if the file changed or backup or replacement fails.
    pub fn save(&self, contents: &[u8]) -> io::Result<PathBuf> {
        validate(contents)?;
        if contents.len() as u64 > rewrite::MAX_BYTES {
            return Err(invalid("SKILL.md exceeds 1 MiB"));
        }
        if checked_path(&self.home, &self.original)? != self.opened.path() {
            return Err(invalid("the skill link changed after review"));
        }
        let folder = self
            .home
            .join(".local/state/neet/backups/ai")
            .join(&self.name);
        private_folders(&self.home, &folder)?;
        let backup = rewrite::write(
            &self.opened,
            contents,
            &self.backups(),
            &self.name,
            SystemTime::now(),
        )
        .map_err(|error| invalid(&error.to_string()))?;
        fs::set_permissions(&backup, fs::Permissions::from_mode(0o600))?;
        Ok(backup)
    }
}
