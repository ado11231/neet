//! Settings: power settings through `pmset`, Mac settings through
//! `defaults`, and neet's own preferences. Every change to the Mac is
//! reviewed first and keeps the old value for undo.

pub mod mac;
pub mod power;
pub mod prefs;

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Reads a small JSON record. A missing file is the default record.
fn load_json<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error),
    }
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    write_atomic(path, &bytes)
}

/// Writes a new copy beside `path`, flushes it, then renames it over
/// `path`, so a crash leaves the old file or the new one. Folders are made
/// private. Links are refused.
fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let folder = path
        .parent()
        .ok_or_else(|| io::Error::other("the path has no folder"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::other(
            "settings files are not written through links",
        ));
    }
    let temporary = folder.join(format!(
        ".{}.neet-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}
