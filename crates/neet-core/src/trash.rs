//! Moves items to the Trash through Finder, so Finder's Put Back can restore
//! them, and empties the Trash through Finder when you ask. See how a
//! cleanup runs, and Emptying the Trash, in `docs/SAFETY.md`.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

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

/// How long Finder has to answer one move. Long enough to answer macOS's
/// question about controlling Finder the first time.
const FINDER_TIMEOUT: Duration = Duration::from_secs(60);

/// Asks Finder to move one item to the Trash. The first time, macOS asks
/// whether neet may control Finder.
///
/// # Errors
///
/// Returns an error if Finder could not be asked, refused, or did not answer
/// within a minute.
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    let mut command = Command::new("/usr/bin/osascript");
    command
        .args(SCRIPT.iter().flat_map(|line| ["-e", line]))
        .arg(path);
    let Some((success, stderr)) = crate::run::with_timeout(&mut command, FINDER_TIMEOUT)? else {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Finder did not answer within a minute. It may still move the item \
             later, so look in the Trash before trying again.",
        ));
    };
    if success {
        return Ok(());
    }
    let message = stderr.trim().to_string();
    if message.contains(NOT_ALLOWED) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "macOS did not let neet control Finder. Allow it in System Settings, \
             Privacy & Security, Automation.",
        ));
    }
    Err(io::Error::other(message))
}

/// How long Finder has to empty the Trash. A large Trash takes a while.
const EMPTY_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Asks Finder to empty the Trash, as its Empty Trash does: every item in
/// it, on every disk, is deleted for good. Nothing else runs it.
///
/// # Errors
///
/// Returns an error if Finder could not be asked, refused, or did not
/// finish within ten minutes.
pub fn empty() -> io::Result<()> {
    let mut command = Command::new("/usr/bin/osascript");
    command.args(["-e", "tell application \"Finder\" to empty trash"]);
    let Some((success, stderr)) = crate::run::with_timeout(&mut command, EMPTY_TIMEOUT)? else {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Finder did not finish within ten minutes. It may still be emptying \
             the Trash.",
        ));
    };
    if success {
        return Ok(());
    }
    let message = stderr.trim().to_string();
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
    move_into_trash_with(path, trash, &rename_without_replacing)
}

fn rename_without_replacing(path: &Path, target: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    renameat_with(CWD, path, CWD, target, RenameFlags::NOREPLACE).map_err(Into::into)
}

fn move_into_trash_with(
    path: &Path,
    trash: &Path,
    rename: &impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no name"))?;
    let mut target = trash.join(name);
    let mut number = 2;
    loop {
        match rename(path, &target) {
            Ok(()) => return Ok(target),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let stem = path.file_stem().unwrap_or(name).to_string_lossy();
        let renamed = match path.extension() {
            Some(extension) => format!("{stem} {number}.{}", extension.to_string_lossy()),
            None => format!("{stem} {number}"),
        };
        target = trash.join(renamed);
        number += 1;
    }
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

    #[test]
    fn keeps_a_file_that_arrives_just_before_the_move() {
        let dir = tempfile::tempdir().unwrap();
        let trash = dir.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let source = dir.path().join("Docker.raw");
        fs::write(&source, "new").unwrap();
        let occupied = trash.join("Docker.raw");
        let moved = super::move_into_trash_with(&source, &trash, &|from, to| {
            if to == occupied {
                fs::write(to, "arrived during the move")?;
            }
            super::rename_without_replacing(from, to)
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(&occupied).unwrap(),
            "arrived during the move"
        );
        assert_eq!(moved, trash.join("Docker 2.raw"));
        assert_eq!(fs::read_to_string(moved).unwrap(), "new");
        assert!(!source.exists());
    }

    #[test]
    fn skips_broken_links_and_folders_with_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let trash = dir.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let source = dir.path().join("Docker.raw");
        fs::write(&source, "new").unwrap();
        let link = trash.join("Docker.raw");
        let missing = dir.path().join("missing");
        std::os::unix::fs::symlink(&missing, &link).unwrap();
        let folder = trash.join("Docker 2.raw");
        fs::create_dir(&folder).unwrap();

        let moved = move_into_trash(&source, &trash).unwrap();

        assert_eq!(moved, trash.join("Docker 3.raw"));
        assert_eq!(fs::read_link(link).unwrap(), missing);
        assert!(folder.is_dir());
        assert_eq!(fs::read_to_string(moved).unwrap(), "new");
    }

    #[test]
    fn a_failed_move_keeps_the_source_and_does_not_try_other_names() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Docker.raw");
        fs::write(&source, "keep").unwrap();
        let calls = std::cell::Cell::new(0);
        let error = super::move_into_trash_with(&source, dir.path(), &|_, _| {
            calls.set(calls.get() + 1);
            assert_eq!(calls.get(), 1);
            Err(std::io::ErrorKind::PermissionDenied.into())
        })
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_to_string(source).unwrap(), "keep");
    }
}
