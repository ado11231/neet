//! Moves items to the Trash through Finder, so Finder's Put Back can restore
//! them. See how a cleanup runs in `docs/SAFETY.md`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The path is passed as an argument, never written into the script, so no
/// name can change what the script does. It becomes a file reference before
/// Finder sees it, since Finder cannot read `POSIX file` itself.
const SCRIPT: [&str; 4] = [
    "on run argv",
    "set picked to (POSIX file (item 1 of argv)) as alias",
    "tell application \"Finder\" to delete picked",
    "end run",
];

/// The error Apple events give when macOS has not let neet control Finder
const NOT_ALLOWED: &str = "-1743";

/// Asks Finder to move one item to the Trash. The first time, macOS asks
/// whether neet may control Finder.
///
/// # Errors
///
/// Returns an error if Finder could not be asked, or refused.
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    let output = Command::new("/usr/bin/osascript")
        .args(SCRIPT.iter().flat_map(|line| ["-e", line]))
        .arg(path)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.contains(NOT_ALLOWED) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "macOS did not let neet control Finder. Allow it in System Settings, \
             Privacy & Security, Automation.",
        ));
    }
    Err(io::Error::other(message))
}

/// Moves one file into `trash` itself, without Finder, keeping its name, or
/// adding a number when the Trash already has one by that name. For files
/// Finder cannot reach, such as inside another app's container, where it
/// hangs. Finder's Put Back does not know where it came from. Returns where
/// it went.
///
/// # Errors
///
/// Returns an error if the file has no name, or could not be moved, such as
/// when the Trash is on another disk.
pub fn move_into_trash(path: &Path, trash: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no name"))?;
    let mut target = trash.join(name);
    let mut number = 2;
    while fs::symlink_metadata(&target).is_ok() {
        let stem = path.file_stem().unwrap_or(name).to_string_lossy();
        let renamed = match path.extension() {
            Some(extension) => format!("{stem} {number}.{}", extension.to_string_lossy()),
            None => format!("{stem} {number}"),
        };
        target = trash.join(renamed);
        number += 1;
    }
    fs::rename(path, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::move_into_trash;
    use std::fs;

    #[test]
    fn moves_into_the_trash_without_replacing_anything() {
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        let trash = dir.path().join(".Trash");
        let data = dir.path().join("data");
        fs::create_dir_all(&trash).expect("folder should be created");
        fs::create_dir_all(&data).expect("folder should be created");
        fs::write(trash.join("Docker.raw"), "old").expect("file should be written");
        fs::write(data.join("Docker.raw"), "new").expect("file should be written");

        let moved = move_into_trash(&data.join("Docker.raw"), &trash).expect("file should move");

        assert_eq!(moved, trash.join("Docker 2.raw"));
        assert!(!data.join("Docker.raw").exists());
        assert_eq!(fs::read_to_string(trash.join("Docker.raw")).unwrap(), "old");
        assert_eq!(fs::read_to_string(moved).unwrap(), "new");
    }
}
