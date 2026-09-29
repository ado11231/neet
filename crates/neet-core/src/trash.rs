//! Moves items to the Trash through Finder, so Finder's Put Back can restore
//! them. See how a cleanup runs in `docs/SAFETY.md`.

use std::io;
use std::path::Path;
use std::process::Command;

/// The path is passed as an argument, never written into the script, so no
/// name can change what the script does.
const SCRIPT: [&str; 3] = [
    "on run argv",
    "tell application \"Finder\" to delete (POSIX file (item 1 of argv))",
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
