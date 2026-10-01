use std::io;
use std::path::Path;
use std::process::Command;

/// How full the disk holding a path is, from the disk's own totals
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskSpace {
    pub total: u64,
    /// Space you can use now. macOS can also free purgeable space on its own,
    /// so Finder may show more.
    pub available: u64,
    /// Space macOS can clear on its own when it runs low, such as iCloud
    /// files kept offline and old caches. Finder counts it as free. `None`
    /// until it has been read.
    pub purgeable: Option<u64>,
}

impl DiskSpace {
    #[must_use]
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

/// Reads the size and free space of the disk that holds `path`
///
/// # Errors
///
/// Returns an error if the disk details cannot be read, such as when `path`
/// does not exist.
pub fn disk_space(path: &Path) -> io::Result<DiskSpace> {
    let stats = rustix::fs::statvfs(path)?;
    Ok(DiskSpace {
        total: stats.f_blocks.saturating_mul(stats.f_frsize),
        available: stats.f_bavail.saturating_mul(stats.f_frsize),
        purgeable: None,
    })
}

/// The path is passed as an argument, never written into the script. The
/// script prints the free space for important use, the number Finder shows.
const FINDER_FREE_SCRIPT: [&str; 6] = [
    "ObjC.import('Foundation')",
    "function run(argv) {",
    "  const key = 'NSURLVolumeAvailableCapacityForImportantUsageKey'",
    "  const values = $.NSURL.fileURLWithPath(argv[0]).resourceValuesForKeysError($([key]), null)",
    "  return ObjC.unwrap(values.objectForKey(key)).toString()",
    "}",
];

/// The free space Finder shows for the disk that holds `path`: what is free
/// now, plus purgeable space. Asks macOS with `osascript`, which takes a
/// fraction of a second, so call it off the screen's thread.
///
/// # Errors
///
/// Returns an error if `osascript` could not run, or macOS gave no number.
pub fn finder_free(path: &Path) -> io::Result<u64> {
    let output = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript"])
        .args(FINDER_FREE_SCRIPT.iter().flat_map(|line| ["-e", line]))
        .arg(path)
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(io::Error::other(message));
    }
    text.trim().parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("macOS gave no free space for {}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{DiskSpace, disk_space, finder_free};
    use tempfile::tempdir;

    #[test]
    fn reads_the_disk_for_a_folder() {
        let root = tempdir().expect("temporary directory should be created");

        let space = disk_space(root.path()).expect("disk space should be readable");

        assert!(space.total > 0);
        assert!(space.available <= space.total);
    }

    #[test]
    fn fails_for_a_missing_path() {
        let root = tempdir().expect("temporary directory should be created");

        assert!(disk_space(&root.path().join("missing")).is_err());
    }

    #[test]
    fn reads_the_free_space_finder_shows() {
        let root = tempdir().expect("temporary directory should be created");
        let space = disk_space(root.path()).expect("disk space should be readable");

        let free = finder_free(root.path()).expect("Finder's free space should be readable");

        assert!(free > 0);
        assert!(free <= space.total);
    }

    #[test]
    fn finder_free_fails_for_a_missing_path() {
        let root = tempdir().expect("temporary directory should be created");

        assert!(finder_free(&root.path().join("missing")).is_err());
    }

    #[test]
    fn used_is_total_minus_available() {
        let space = DiskSpace {
            total: 100,
            available: 30,
            purgeable: None,
        };

        assert_eq!(space.used(), 70);
    }
}
