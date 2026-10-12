//! Reviewed chat file removal through Finder, with identity and activity checks.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use super::{Item, Kind, Locations, Provider, checked_path};

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    device: u64,
    inode: u64,
    size: u64,
    modified: SystemTime,
    changed: (i64, i64),
}

impl Snapshot {
    fn read(path: &Path) -> io::Result<Self> {
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() {
            return Err(io::Error::other("Only regular chat files can be removed."));
        }
        Ok(Self {
            device: meta.dev(),
            inode: meta.ino(),
            size: meta.len(),
            modified: meta.modified()?,
            changed: (meta.ctime(), meta.ctime_nsec()),
        })
    }
}

pub struct Removal {
    pub item: Item,
    path: PathBuf,
    snapshot: Snapshot,
    locations: Locations,
}

impl Removal {
    /// Captures the file shown in a removal review without changing it.
    ///
    /// # Errors
    /// Refuses unknown paths, links, and files changed since discovery.
    pub fn prepare(locations: &Locations, item: &Item) -> io::Result<Self> {
        let path = validate(locations, item)?;
        let snapshot = Snapshot::read(&path)?;
        if snapshot.size != item.size || Some(snapshot.modified) != item.modified {
            return Err(io::Error::other(
                "Chat changed. Refresh and review it again.",
            ));
        }
        Ok(Self {
            item: item.clone(),
            path,
            snapshot,
            locations: locations.clone(),
        })
    }

    /// Moves this reviewed file to Trash. Finder's Put Back can restore it.
    ///
    /// # Errors
    /// Refuses changed, open, or recently written files and reports Finder errors.
    pub fn apply(&self) -> io::Result<()> {
        self.apply_with(&ensure_closed, &crate::trash::move_to_trash)
    }

    fn apply_with(
        &self,
        ensure_closed: &impl Fn(&Path) -> io::Result<()>,
        move_file: &impl Fn(&Path) -> io::Result<()>,
    ) -> io::Result<()> {
        ensure_closed(&self.path)?;
        if validate(&self.locations, &self.item)? != self.path
            || Snapshot::read(&self.path)? != self.snapshot
        {
            return Err(io::Error::other(
                "Chat changed after review. Refresh and try again.",
            ));
        }
        move_file(&self.path)
    }
}

fn validate(locations: &Locations, item: &Item) -> io::Result<PathBuf> {
    let allowed = match item.provider {
        Provider::Claude => inside(&item.path, &locations.claude.join("projects"), 2, 2),
        Provider::Codex => {
            inside(&item.path, &locations.codex.join("sessions"), 1, 4)
                || inside(&item.path, &locations.codex.join("archived_sessions"), 1, 1)
        }
        Provider::Gemini => {
            inside(&item.path, &locations.gemini.join("tmp"), 3, 3)
                && item
                    .path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == "chats")
        }
        Provider::Shared => false,
    };
    let extension = item.path.extension().and_then(|part| part.to_str());
    let extension_ok = if item.provider == Provider::Gemini {
        matches!(extension, Some("json" | "jsonl"))
    } else {
        extension == Some("jsonl")
    };
    let name_ok = item.provider != Provider::Codex
        || item
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"));
    if item.kind != Kind::Chat || !allowed || !extension_ok || !name_ok {
        return Err(io::Error::other("This is not a supported chat file."));
    }
    let resolved = checked_path(&locations.home, &item.path)?;
    // Refuse linked parents too, even when their target is still inside home.
    for path in item
        .path
        .ancestors()
        .take_while(|path| *path != locations.home)
    {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(io::Error::other("Chat removal does not follow links."));
        }
    }
    Ok(resolved)
}

fn inside(path: &Path, root: &Path, min: usize, max: usize) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|relative| (min..=max).contains(&relative.components().count()))
}

fn ensure_closed(path: &Path) -> io::Result<()> {
    let age = fs::metadata(path)?
        .modified()?
        .elapsed()
        .unwrap_or_default();
    if age < Duration::from_secs(60) {
        return Err(io::Error::other(
            "Recently updated. Close the chat and wait a minute before removing it.",
        ));
    }
    let mut command = Command::new("/usr/sbin/lsof");
    command.args(["-n", "-P", "-F", "p", "--"]).arg(path);
    match crate::run::capture(&mut command, Duration::from_secs(5))? {
        Some(output)
            if !output.success
                && output.stderr.trim().is_empty()
                && output.stdout.trim().is_empty() =>
        {
            Ok(())
        }
        Some(output) if output.success => Err(io::Error::other(
            "Chat file is open. Close the assistant before removing it.",
        )),
        _ => Err(io::Error::other(
            "Could not check whether this chat is open. Nothing was moved.",
        )),
    }
}

#[cfg(test)]
mod tests;
