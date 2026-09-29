use std::io;
use std::path::Path;

/// How full the disk holding a path is, from the disk's own totals
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskSpace {
    pub total: u64,
    /// Space you can use now. macOS can also free purgeable space on its own,
    /// so Finder may show more.
    pub available: u64,
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
    })
}

#[cfg(test)]
mod tests {
    use super::{DiskSpace, disk_space};
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
    fn used_is_total_minus_available() {
        let space = DiskSpace {
            total: 100,
            available: 30,
        };

        assert_eq!(space.used(), 70);
    }
}
