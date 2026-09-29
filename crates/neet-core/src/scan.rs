use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::size::{HardLinkTracker, allocated_size};
use crate::tree::{NodeId, NodeKind, Tree};

/// How many entries to scan between progress reports
const PROGRESS_EVERY: u64 = 1024;

/// How far a scan has got
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub entries: u64,
    pub bytes: u64,
}

/// A path the scan could not read
#[derive(Debug)]
pub struct ScanError {
    pub path: Option<PathBuf>,
    pub message: String,
}

/// The result of scanning a folder
#[derive(Debug)]
pub struct Scan {
    pub tree: Tree,
    pub errors: Vec<ScanError>,
    /// Folders on another disk, listed but not entered
    pub other_disks: Vec<PathBuf>,
}

impl Scan {
    /// A scan is complete when every folder could be read
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Walks a directory without following links or crossing filesystem boundaries
pub fn walk_directory(root: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    WalkDir::new(root)
        .follow_links(false)
        .same_file_system(true)
        .into_iter()
}

/// Scans a folder into a tree, counting each hard linked file once
///
/// Unreadable paths are recorded in `errors` and the scan carries on.
/// `on_progress` is called every so often, and once more at the end.
///
/// # Errors
///
/// Returns an error if the root cannot be read or is not a folder.
pub fn scan(root: &Path, mut on_progress: impl FnMut(Progress)) -> io::Result<Scan> {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a folder", root.display()),
        ));
    }
    let root_device = root_metadata.dev();

    let mut tree = Tree::new(root.as_os_str());
    let mut errors = Vec::new();
    let mut other_disks = Vec::new();
    let mut tracker = HardLinkTracker::new();
    let mut progress = Progress::default();
    // The folder at each depth on the way down to the current entry
    let mut ancestors: Vec<NodeId> = vec![tree.root()];

    for result in walk_directory(root) {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(ScanError {
                    path: error.path().map(Path::to_path_buf),
                    message: error.to_string(),
                });
                continue;
            }
        };

        let depth = entry.depth();
        if depth == 0 {
            continue;
        }

        let file_type = entry.file_type();
        let kind = if file_type.is_dir() {
            NodeKind::Directory
        } else if file_type.is_file() {
            NodeKind::File
        } else if file_type.is_symlink() {
            NodeKind::Symlink
        } else {
            NodeKind::Other
        };

        let size = match entry.metadata() {
            Ok(metadata) if kind == NodeKind::Directory && metadata.dev() != root_device => {
                other_disks.push(entry.path().to_path_buf());
                0
            }
            Ok(metadata) if tracker.first_sighting(&metadata) => allocated_size(&metadata),
            Ok(_) => 0,
            Err(error) => {
                errors.push(ScanError {
                    path: Some(entry.path().to_path_buf()),
                    message: error.to_string(),
                });
                0
            }
        };

        let id = tree.add(ancestors[depth - 1], entry.file_name(), kind, size);
        if kind == NodeKind::Directory {
            ancestors.truncate(depth);
            ancestors.push(id);
        }

        progress.entries += 1;
        progress.bytes += size;
        if progress.entries % PROGRESS_EVERY == 0 {
            on_progress(progress);
        }
    }

    on_progress(progress);
    Ok(Scan {
        tree,
        errors,
        other_disks,
    })
}

#[cfg(test)]
mod tests {
    use super::{Progress, scan, walk_directory};
    use crate::tree::{NodeKind, Tree};
    use std::fs::{self, File};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;
    use tempfile::tempdir;

    fn find(tree: &Tree, path: &Path) -> Option<crate::tree::NodeId> {
        tree.iter()
            .map(|(id, _)| id)
            .find(|&id| tree.path(id) == path)
    }

    #[test]
    fn walks_a_directory_and_files() {
        let root = tempdir().expect("temporary directory should be created");
        let file_path = root.path().join("example.txt");

        fs::write(&file_path, "example").expect("test file should be written");

        let paths = walk_directory(root.path())
            .map(|result| {
                result
                    .expect("directory entry should be readable")
                    .into_path()
            })
            .collect::<Vec<_>>();

        assert!(paths.contains(&root.path().to_path_buf()));
        assert!(paths.contains(&file_path));
    }

    #[test]
    fn reports_a_symlink_without_following_it() {
        let root = tempdir().expect("scan directory should be created");
        let target = tempdir().expect("target directory should be created");

        let target_file = target.path().join("inside.txt");
        fs::write(&target_file, "example").expect("target file should be written");

        let link_path = root.path().join("linked-directory");
        symlink(target.path(), &link_path).expect("symbolic link should be created");

        let paths = walk_directory(root.path())
            .map(|result| {
                result
                    .expect("directory entry should be readable")
                    .into_path()
            })
            .collect::<Vec<_>>();

        assert!(paths.contains(&link_path));
        assert!(!paths.contains(&link_path.join("inside.txt")));
    }
    #[test]
    fn reports_the_path_for_a_missing_root() {
        let directory = tempdir().expect("temporary directory should be created");
        let missing_path = directory.path().join("missing");

        let result = walk_directory(&missing_path)
            .next()
            .expect("walker should return one result");

        let error = result.expect_err("missing root should produce an error");

        assert_eq!(error.path(), Some(missing_path.as_path()));
    }

    #[test]
    fn scan_builds_nested_folders_with_totals() {
        let root = tempdir().expect("temporary directory should be created");
        let inner = root.path().join("outer").join("inner");
        fs::create_dir_all(&inner).expect("nested folders should be created");
        fs::write(inner.join("a.bin"), vec![1_u8; 64 * 1024]).expect("file should be written");
        fs::write(
            root.path().join("outer").join("b.bin"),
            vec![1_u8; 32 * 1024],
        )
        .expect("file should be written");

        let result = scan(root.path(), |_| {}).expect("scan should succeed");
        let tree = &result.tree;

        let outer = find(tree, &root.path().join("outer")).expect("outer should be in the tree");
        let inner_id = find(tree, &inner).expect("inner should be in the tree");
        let a = find(tree, &inner.join("a.bin")).expect("a.bin should be in the tree");

        assert!(result.is_complete());
        assert_eq!(tree.get(a).kind, NodeKind::File);
        assert_eq!(tree.get(a).parent, Some(inner_id));
        assert_eq!(tree.get(inner_id).parent, Some(outer));
        assert!(tree.get(outer).total_size >= 96 * 1024);
        assert_eq!(
            tree.get(tree.root()).total_size,
            tree.iter()
                .skip(1)
                .map(|(_, node)| node.own_size)
                .sum::<u64>()
        );
    }

    #[test]
    fn scan_counts_a_hard_link_once() {
        let root = tempdir().expect("temporary directory should be created");
        let original = root.path().join("original.bin");
        fs::write(&original, vec![1_u8; 64 * 1024]).expect("file should be written");

        let single = scan(root.path(), |_| {}).expect("scan should succeed");
        fs::hard_link(&original, root.path().join("link.bin"))
            .expect("hard link should be created");
        let linked = scan(root.path(), |_| {}).expect("scan should succeed");

        assert_eq!(
            linked.tree.get(linked.tree.root()).total_size,
            single.tree.get(single.tree.root()).total_size
        );
    }

    #[test]
    fn scan_measures_a_file_with_empty_parts_by_space_used() {
        let root = tempdir().expect("temporary directory should be created");
        let length = 64 * 1024 * 1024;
        File::create(root.path().join("sparse.bin"))
            .and_then(|file| file.set_len(length))
            .expect("sparse file should be created");

        let result = scan(root.path(), |_| {}).expect("scan should succeed");

        assert!(result.tree.get(result.tree.root()).total_size < length);
    }

    #[test]
    fn scan_lists_a_symlink_without_following_it() {
        let root = tempdir().expect("scan directory should be created");
        let target = tempdir().expect("target directory should be created");
        fs::write(target.path().join("inside.bin"), vec![1_u8; 64 * 1024])
            .expect("target file should be written");
        let link = root.path().join("link");
        symlink(target.path(), &link).expect("symbolic link should be created");

        let result = scan(root.path(), |_| {}).expect("scan should succeed");
        let link_id = find(&result.tree, &link).expect("link should be in the tree");

        assert_eq!(result.tree.get(link_id).kind, NodeKind::Symlink);
        assert!(find(&result.tree, &link.join("inside.bin")).is_none());
        assert!(result.tree.get(result.tree.root()).total_size < 64 * 1024);
    }

    #[test]
    fn scan_records_an_unreadable_folder_and_carries_on() {
        let root = tempdir().expect("temporary directory should be created");
        let locked = root.path().join("locked");
        fs::create_dir(&locked).expect("locked folder should be created");
        fs::write(locked.join("hidden.txt"), "x").expect("file should be written");
        fs::write(root.path().join("visible.txt"), "x").expect("file should be written");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
            .expect("permissions should be set");

        let result = scan(root.path(), |_| {});
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755))
            .expect("permissions should be restored");
        let result = result.expect("scan should succeed");

        assert!(!result.is_complete());
        assert_eq!(result.errors[0].path.as_deref(), Some(locked.as_path()));
        assert!(find(&result.tree, &locked).is_some());
        assert!(find(&result.tree, &root.path().join("visible.txt")).is_some());
    }

    #[test]
    fn scan_fails_for_a_missing_root() {
        let root = tempdir().expect("temporary directory should be created");

        assert!(scan(&root.path().join("missing"), |_| {}).is_err());
    }

    #[test]
    fn scan_fails_when_the_root_is_a_file() {
        let root = tempdir().expect("temporary directory should be created");
        let file = root.path().join("file.txt");
        fs::write(&file, "x").expect("file should be written");

        assert!(scan(&file, |_| {}).is_err());
    }

    #[test]
    fn scan_reports_final_progress() {
        let root = tempdir().expect("temporary directory should be created");
        for index in 0..3 {
            fs::write(root.path().join(format!("{index}.txt")), "x")
                .expect("file should be written");
        }

        let mut last = Progress::default();
        let result = scan(root.path(), |progress| last = progress).expect("scan should succeed");

        assert_eq!(last.entries, 3);
        assert_eq!(last.bytes, result.tree.get(result.tree.root()).total_size);
    }
}
