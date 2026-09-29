use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

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
    /// macOS refused access, often because Full Disk Access is off
    pub permission_denied: bool,
}

impl ScanError {
    fn new(path: Option<PathBuf>, error: &jwalk::Error) -> Self {
        let io_error = error.io_error();
        Self {
            path,
            message: io_error.map_or_else(|| error.to_string(), ToString::to_string),
            permission_denied: io_error
                .is_some_and(|error| error.kind() == io::ErrorKind::PermissionDenied),
        }
    }
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

/// A walker that keeps each entry's metadata, read on the walker's threads
type Walk = jwalk::WalkDirGeneric<((), Option<jwalk::Result<Metadata>>)>;

/// How many times to retry a lookup that macOS interrupted
const RETRIES: usize = 3;

/// macOS sometimes interrupts a lookup when many run at once. Trying again
/// usually works, so these are not real read failures.
fn interrupted(error: &jwalk::Error) -> bool {
    error
        .io_error()
        .is_some_and(|error| error.kind() == io::ErrorKind::Interrupted)
}

/// Walks a folder without following links or leaving its disk. Entries come
/// out parents first, one folder at a time, even when folders are read on
/// several threads.
fn walk(root: &Path, root_device: u64, parallelism: jwalk::Parallelism) -> Walk {
    Walk::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .parallelism(parallelism)
        .process_read_dir(move |_, _, (), children| {
            for child in children.iter_mut().flatten() {
                let mut metadata = child.metadata();
                for _ in 0..RETRIES {
                    match &metadata {
                        Err(error) if interrupted(error) => metadata = child.metadata(),
                        _ => break,
                    }
                }
                // A folder on another disk is listed, but not entered.
                if let Ok(metadata) = &metadata
                    && metadata.is_dir()
                    && metadata.dev() != root_device
                {
                    child.read_children = None;
                }
                child.client_state = Some(metadata);
            }
        })
}

fn kind_of(file_type: fs::FileType) -> NodeKind {
    if file_type.is_dir() {
        NodeKind::Directory
    } else if file_type.is_file() {
        NodeKind::File
    } else if file_type.is_symlink() {
        NodeKind::Symlink
    } else {
        NodeKind::Other
    }
}

/// Fills the tree from one or more walks, keeping what they share
struct Builder<F> {
    tree: Tree,
    errors: Vec<ScanError>,
    other_disks: Vec<PathBuf>,
    tracker: HardLinkTracker,
    progress: Progress,
    root_device: u64,
    on_progress: F,
    /// Folders whose reading was interrupted, to read again afterwards
    retry: Vec<(NodeId, PathBuf)>,
}

impl<F: FnMut(Progress)> Builder<F> {
    /// Adds everything `walk` finds below `folder`, which is already in the tree.
    fn fill(&mut self, folder: NodeId, walk: Walk, retry_interrupted: bool) {
        // The folder at each depth on the way down to the current entry
        let mut ancestors: Vec<NodeId> = vec![folder];

        for result in walk {
            let entry = match result {
                Ok(entry) => entry,
                Err(error) => {
                    let path = error.path().map(Path::to_path_buf);
                    self.errors.push(ScanError::new(path, &error));
                    continue;
                }
            };

            // A folder that could not be opened carries the error on its own entry.
            let read_error = entry
                .read_children
                .as_ref()
                .and_then(jwalk::ReadChildren::error);

            let depth = entry.depth;
            if depth == 0 {
                if let Some(error) = read_error {
                    self.errors.push(ScanError::new(Some(entry.path()), error));
                }
                continue;
            }

            let kind = kind_of(entry.file_type);
            let size = match &entry.client_state {
                Some(Ok(metadata))
                    if kind == NodeKind::Directory && metadata.dev() != self.root_device =>
                {
                    self.other_disks.push(entry.path());
                    0
                }
                Some(Ok(metadata)) if self.tracker.first_sighting(metadata) => {
                    allocated_size(metadata)
                }
                Some(Ok(_)) | None => 0,
                Some(Err(error)) => {
                    self.errors.push(ScanError::new(Some(entry.path()), error));
                    0
                }
            };

            let path = read_error.map(|_| entry.path());
            let id = self
                .tree
                .add(ancestors[depth - 1], entry.file_name, kind, size);
            if kind == NodeKind::Directory {
                ancestors.truncate(depth);
                ancestors.push(id);
            }
            if let (Some(error), Some(path)) = (read_error, path) {
                if retry_interrupted && interrupted(error) {
                    self.retry.push((id, path));
                } else {
                    self.errors.push(ScanError::new(Some(path), error));
                }
            }

            self.progress.entries += 1;
            self.progress.bytes += size;
            if self.progress.entries.is_multiple_of(PROGRESS_EVERY) {
                (self.on_progress)(self.progress);
            }
        }
    }
}

/// Scans a folder into a tree, counting each hard linked file once
///
/// Folders are read on several threads. Unreadable paths are recorded in
/// `errors` and the scan carries on. `on_progress` is called every so often,
/// and once more at the end.
///
/// # Errors
///
/// Returns an error if the root cannot be read or is not a folder.
pub fn scan(root: &Path, on_progress: impl FnMut(Progress)) -> io::Result<Scan> {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a folder", root.display()),
        ));
    }
    let root_device = root_metadata.dev();

    let mut builder = Builder {
        tree: Tree::new(root.as_os_str()),
        errors: Vec::new(),
        other_disks: Vec::new(),
        tracker: HardLinkTracker::new(),
        progress: Progress::default(),
        root_device,
        on_progress,
        retry: Vec::new(),
    };
    let root_id = builder.tree.root();
    let parallel = jwalk::Parallelism::RayonNewPool(0);
    builder.fill(root_id, walk(root, root_device, parallel), true);

    // Read interrupted folders again, one at a time. A second failure is real.
    for (folder, path) in std::mem::take(&mut builder.retry) {
        let serial = walk(&path, root_device, jwalk::Parallelism::Serial);
        builder.fill(folder, serial, false);
    }

    (builder.on_progress)(builder.progress);
    Ok(Scan {
        tree: builder.tree,
        errors: builder.errors,
        other_disks: builder.other_disks,
    })
}

#[cfg(test)]
mod tests {
    use super::{Progress, scan};
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
        assert!(result.errors[0].permission_denied);
        assert!(
            !result.errors[0]
                .message
                .contains(&*locked.to_string_lossy())
        );
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

    /// Mounts a small disk image at `mount_point`, and detaches it when dropped.
    struct DiskImage {
        mount_point: std::path::PathBuf,
    }

    impl DiskImage {
        fn attach(image: &Path, mount_point: &Path) -> Option<Self> {
            let run = |args: &[&std::ffi::OsStr]| {
                std::process::Command::new("hdiutil")
                    .args(args)
                    .output()
                    .is_ok_and(|output| output.status.success())
            };
            let created = run(&[
                "create".as_ref(),
                "-size".as_ref(),
                "2m".as_ref(),
                "-fs".as_ref(),
                "HFS+".as_ref(),
                "-volname".as_ref(),
                "neet-test".as_ref(),
                image.as_os_str(),
            ]);
            let attached = created
                && run(&[
                    "attach".as_ref(),
                    "-nobrowse".as_ref(),
                    "-mountpoint".as_ref(),
                    mount_point.as_os_str(),
                    image.as_os_str(),
                ]);
            attached.then(|| Self {
                mount_point: mount_point.to_path_buf(),
            })
        }
    }

    impl Drop for DiskImage {
        fn drop(&mut self) {
            let _ = std::process::Command::new("hdiutil")
                .args([
                    "detach".as_ref(),
                    "-force".as_ref(),
                    self.mount_point.as_os_str(),
                ])
                .output();
        }
    }

    #[test]
    fn scan_lists_another_disk_without_entering_it() {
        let root = tempdir().expect("temporary directory should be created");
        let images = tempdir().expect("image directory should be created");
        let mount_point = root.path().join("usb");
        fs::create_dir(&mount_point).expect("mount point should be created");

        let Some(disk) = DiskImage::attach(&images.path().join("disk.dmg"), &mount_point) else {
            eprintln!("skipping: hdiutil could not attach a disk image");
            return;
        };
        fs::write(mount_point.join("inside.bin"), vec![1_u8; 64 * 1024])
            .expect("file on the other disk should be written");

        let result = scan(root.path(), |_| {}).expect("scan should succeed");
        drop(disk);

        assert_eq!(result.other_disks, vec![mount_point.clone()]);
        assert!(find(&result.tree, &mount_point).is_some());
        assert!(find(&result.tree, &mount_point.join("inside.bin")).is_none());
        assert!(result.tree.get(result.tree.root()).total_size < 64 * 1024);
    }
}
