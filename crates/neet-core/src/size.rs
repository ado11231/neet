use std::collections::HashSet;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;

/// Returns space a file uses on disk, can be less than its length
#[must_use]
pub fn allocated_size(metadata: &Metadata) -> u64 {
    metadata.blocks().saturating_mul(512)
}

/// Remembers files already counted, a file with several names is counted as one
#[derive(Debug, Default)]
pub struct HardLinkTracker {
    seen: HashSet<(u64, u64)>,
}

impl HardLinkTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Returns true the first time a file is seen; size should be counted
    /// Returns false if file was already seen under another name; skip counting
    pub fn first_sighting(&mut self, metadata: &Metadata) -> bool {
        // Folders are not considered hardlinks
        if metadata.is_dir() {
            return true;
        }

        // File with only one name cant show up again
        if metadata.nlink() == 1 {
            return true;
        }

        // Saves disk number and file number; inserts key into set
        // Returns true if new, false if saved previously
        let key = (metadata.dev(), metadata.ino());
        self.seen.insert(key)
    }
}

#[cfg(test)]
mod tests {
    use super::{HardLinkTracker, allocated_size};
    use std::fs::{self, File, Metadata};
    use std::path::Path;
    use tempfile::tempdir;

    fn metadata(path: &Path) -> Metadata {
        fs::symlink_metadata(path).expect("metadata should be readable")
    }

    #[test]
    fn counts_a_file_with_one_name_every_time() {
        let root = tempdir().expect("temporary directory should be created");
        let file = root.path().join("single.txt");
        fs::write(&file, "example").expect("test file should be written");

        let mut tracker = HardLinkTracker::new();

        assert!(tracker.first_sighting(&metadata(&file)));
        // Files with one name are never stored, so asking again still says yes.
        assert!(tracker.first_sighting(&metadata(&file)));
    }

    #[test]
    fn counts_a_hard_link_only_once() {
        let root = tempdir().expect("temporary directory should be created");
        let original = root.path().join("original.txt");
        let link = root.path().join("link.txt");
        fs::write(&original, "example").expect("test file should be written");
        fs::hard_link(&original, &link).expect("hard link should be created");

        let mut tracker = HardLinkTracker::new();

        assert!(tracker.first_sighting(&metadata(&original)));
        assert!(!tracker.first_sighting(&metadata(&link)));
        assert!(!tracker.first_sighting(&metadata(&original)));
    }

    #[test]
    fn counts_different_linked_files_separately() {
        let root = tempdir().expect("temporary directory should be created");
        let first = root.path().join("first.txt");
        let second = root.path().join("second.txt");
        fs::write(&first, "first").expect("first file should be written");
        fs::write(&second, "second").expect("second file should be written");
        fs::hard_link(&first, root.path().join("first-link.txt"))
            .expect("first link should be created");
        fs::hard_link(&second, root.path().join("second-link.txt"))
            .expect("second link should be created");

        let mut tracker = HardLinkTracker::new();

        assert!(tracker.first_sighting(&metadata(&first)));
        assert!(tracker.first_sighting(&metadata(&second)));
    }

    #[test]
    fn counts_a_directory_every_time() {
        let root = tempdir().expect("temporary directory should be created");
        fs::create_dir(root.path().join("child")).expect("child directory should be created");

        let mut tracker = HardLinkTracker::new();

        // Directories report more than one link, but they are never hard links.
        assert!(tracker.first_sighting(&metadata(root.path())));
        assert!(tracker.first_sighting(&metadata(root.path())));
    }

    #[test]
    fn a_file_with_empty_parts_uses_less_space_than_its_length() {
        let root = tempdir().expect("temporary directory should be created");
        let path = root.path().join("sparse.bin");
        let length = 64 * 1024 * 1024;

        let file = File::create(&path).expect("sparse file should be created");
        file.set_len(length)
            .expect("sparse file should be extended");
        drop(file);

        let metadata = metadata(&path);

        assert_eq!(metadata.len(), length);
        assert!(allocated_size(&metadata) < length);
    }

    #[test]
    fn a_written_file_uses_space_on_disk() {
        let root = tempdir().expect("temporary directory should be created");
        let path = root.path().join("full.bin");
        fs::write(&path, vec![1_u8; 64 * 1024]).expect("test file should be written");

        assert!(allocated_size(&metadata(&path)) >= 64 * 1024);
    }
}
