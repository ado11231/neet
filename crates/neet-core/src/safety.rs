//! The path check every cleanup target must pass. See `docs/SAFETY.md`.
//!
//! Only [`CleanupRoots::validate_deletable`] can make a [`ValidatedPath`], so no
//! other code can skip the check. Rules cannot change anything here.

use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

/// Folders cleanup may remove items from, relative to the home folder. `*`
/// stands for exactly one folder. Keep in step with the table in SAFETY.md.
const ROOTS: &[&str] = &[
    "Library/Caches",
    "Library/Containers/*/Data/Library/Caches",
    "Library/Logs",
    "Library/Saved Application State",
    "Library/Developer/Xcode/DerivedData",
    "Library/Developer/Xcode/iOS DeviceSupport",
    "Library/Developer/CoreSimulator/Caches",
    ".npm/_cacache",
    ".cargo/registry/cache",
    ".cache/pip",
];

/// Folders in the home folder that are never touched, with everything inside.
const PROTECTED_IN_HOME: &[&str] = &[
    "Documents",
    "Desktop",
    "Downloads",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Library/Keychains",
    ".ssh",
    "Library/Developer/Xcode/Archives",
];

/// Folders in the home folder that hold your own files. Cleanup never
/// touches them, but the clutter check may take build folders and
/// installers from them.
const YOUR_FILES: &[&str] = &["Documents", "Desktop", "Downloads"];

/// File name endings of disk images and installers the clutter check may
/// take from Downloads. Keep in step with SAFETY.md.
pub const INSTALLER_ENDINGS: &[&str] = &["dmg", "pkg", "iso", "xip"];

/// Where Docker Desktop keeps its disk image, from the home folder. The
/// clutter check may take this one file, once Docker Desktop has quit.
pub const DOCKER_IMAGE: &[&str] = &[
    "Library",
    "Containers",
    "com.docker.docker",
    "Data",
    "vms",
    "0",
    "data",
    "Docker.raw",
];

/// System folders that are never touched, with everything inside.
const PROTECTED_SYSTEM: &[&str] = &["/System", "/usr", "/Library"];

/// Folders in `~/Library` whose direct children app removal may take. Keep
/// in step with the table in SAFETY.md.
pub const APP_LIBRARY_FOLDERS: &[&str] = &[
    "Caches",
    "Logs",
    "Saved Application State",
    "HTTPStorages",
    "WebKit",
    "Application Support",
    "Containers",
    "Preferences",
    "Group Containers",
    "LaunchAgents",
];

/// Which check made a [`ValidatedPath`], so the check right before the move
/// is the same one
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// Inside a cleanup root
    Cleanup,
    /// An app, or one of its related files
    AppRemoval,
    /// A project build folder, or an installer in Downloads
    Clutter,
}

/// Why a path cannot be cleaned
#[derive(Debug)]
pub enum SafetyError {
    Empty,
    Relative,
    /// The path is not valid UTF-8, or has a part neet does not support
    Unsupported,
    TopOfDisk,
    /// The path could not be read, such as when it no longer exists
    Unreadable(io::Error),
    OutsideRoots,
    /// The path is a cleanup root itself, not something inside one
    IsRoot,
    Protected,
    /// A symbolic link that leads outside its cleanup root, or nowhere
    LinkLeadsOut,
    /// A symbolic link, which app removal never moves
    Link,
    /// Not a `.app` folder in an Applications folder, nor a related file app
    /// removal may take
    NotAppItem,
    /// Not a project build folder, an installer in Downloads, nor Docker's
    /// disk image
    NotClutter,
}

/// An `io::Error` cannot be copied, so a copy keeps its kind and message.
pub(crate) fn copy_error(error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), error.to_string())
}

impl Clone for SafetyError {
    fn clone(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Relative => Self::Relative,
            Self::Unsupported => Self::Unsupported,
            Self::TopOfDisk => Self::TopOfDisk,
            Self::Unreadable(error) => Self::Unreadable(copy_error(error)),
            Self::OutsideRoots => Self::OutsideRoots,
            Self::IsRoot => Self::IsRoot,
            Self::Protected => Self::Protected,
            Self::LinkLeadsOut => Self::LinkLeadsOut,
            Self::Link => Self::Link,
            Self::NotAppItem => Self::NotAppItem,
            Self::NotClutter => Self::NotClutter,
        }
    }
}

impl fmt::Display for SafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the path is empty"),
            Self::Relative => write!(f, "the path is not a full path"),
            Self::Unsupported => write!(f, "the path has a name neet does not support"),
            Self::TopOfDisk => write!(f, "the path is the top folder of a disk"),
            Self::Unreadable(error) => write!(f, "the path could not be read: {error}"),
            Self::OutsideRoots => write!(f, "the path is outside the folders cleanup may touch"),
            Self::IsRoot => write!(f, "the path is a cleanup folder itself"),
            Self::Protected => write!(f, "the path is protected"),
            Self::LinkLeadsOut => write!(f, "the path is a link that leads out of its folder"),
            Self::Link => write!(f, "the path is a link"),
            Self::NotAppItem => write!(f, "the path is not an app or a file app removal may take"),
            Self::NotClutter => write!(
                f,
                "the path is not a project build folder, an installer in Downloads, or Docker's disk image"
            ),
        }
    }
}

impl std::error::Error for SafetyError {}

/// A path that passed the path check, with what identifies it on disk
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPath {
    path: PathBuf,
    root: PathBuf,
    device: u64,
    inode: u64,
    check: Check,
}

impl ValidatedPath {
    /// The real path, after following every link above the item itself
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The cleanup root the path is inside
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn device(&self) -> u64 {
        self.device
    }

    #[must_use]
    pub fn inode(&self) -> u64 {
        self.inode
    }

    /// Which check made it
    #[must_use]
    pub fn check(&self) -> Check {
        self.check
    }
}

/// Whether neet is running as root, such as under `sudo`. neet refuses to:
/// it would find root's home folder instead of yours, and cleanup never needs
/// admin rights.
#[must_use]
pub fn running_as_root() -> bool {
    rustix::process::geteuid().is_root()
}

/// The cleanup roots and protected paths for one home folder
#[derive(Debug, Clone)]
pub struct CleanupRoots {
    home: PathBuf,
    /// The shared Applications folder, `/Applications` outside tests
    applications: PathBuf,
}

/// The names in a path, or `None` if any part is not a plain UTF-8 name.
fn names(path: &Path) -> Option<Vec<&str>> {
    path.components()
        .filter(|component| !matches!(component, Component::RootDir))
        .map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect()
}

/// Whether `path` starts with `prefix`, where `*` in `prefix` matches one name.
fn starts_with_pattern(path: &[&str], prefix: &[&str]) -> bool {
    path.len() >= prefix.len()
        && prefix
            .iter()
            .zip(path)
            .all(|(want, got)| *want == "*" || want == got)
}

impl CleanupRoots {
    /// The roots for the home folder at `home`. The home folder must exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the home folder cannot be read.
    pub fn new(home: &Path) -> io::Result<Self> {
        Self::with_applications(home, Path::new("/Applications"))
    }

    /// The roots for `home`, with `applications` in place of `/Applications`.
    /// Both folders must exist.
    ///
    /// # Errors
    ///
    /// Returns an error if either folder cannot be read.
    pub fn with_applications(home: &Path, applications: &Path) -> io::Result<Self> {
        Ok(Self {
            home: fs::canonicalize(home)?,
            applications: fs::canonicalize(applications)?,
        })
    }

    /// The folders apps may be removed from: the shared Applications folder,
    /// and your own
    #[must_use]
    pub fn application_folders(&self) -> [PathBuf; 2] {
        [self.applications.clone(), self.home.join("Applications")]
    }

    /// The real home folder
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Whether `path`, a real path with no links in it, is a protected
    /// folder or inside one. See the protected folders in SAFETY.md.
    #[must_use]
    pub fn protects(&self, path: &Path) -> bool {
        names(path).is_none_or(|names| self.is_protected(&names))
    }

    /// The names of the real home folder
    fn home_names(&self) -> Vec<&str> {
        names(&self.home).unwrap_or_default()
    }

    /// The root that `path` is inside, as its names, and whether `path` is
    /// the root itself.
    fn root_of<'a>(&self, path: &[&'a str]) -> Option<(Vec<&'a str>, bool)> {
        let home = self.home_names();
        if !starts_with_pattern(path, &home) {
            return None;
        }
        let rest = &path[home.len()..];
        ROOTS.iter().find_map(|root| {
            let root: Vec<&str> = root.split('/').collect();
            starts_with_pattern(rest, &root).then(|| {
                let full = path[..home.len() + root.len()].to_vec();
                (full, rest.len() == root.len())
            })
        })
    }

    fn is_protected(&self, path: &[&str]) -> bool {
        self.is_protected_for(path, Check::Cleanup)
    }

    /// Whether `path` is protected for `check`. Only the clutter check may
    /// reach into Documents, Desktop, and Downloads.
    fn is_protected_for(&self, path: &[&str], check: Check) -> bool {
        let home = self.home_names();
        let in_home = starts_with_pattern(path, &home);
        let protected_in_home = in_home
            && PROTECTED_IN_HOME.iter().any(|protected| {
                if check == Check::Clutter && YOUR_FILES.contains(protected) {
                    return false;
                }
                let protected: Vec<&str> = protected.split('/').collect();
                starts_with_pattern(&path[home.len()..], &protected)
            });
        let protected_system = PROTECTED_SYSTEM.iter().any(|protected| {
            let protected: Vec<&str> = protected.trim_start_matches('/').split('/').collect();
            starts_with_pattern(path, &protected)
        });
        let git = path.contains(&".git");
        protected_in_home || protected_system || git || path == home.as_slice()
    }

    /// Checks that a path may be moved to the Trash. See the path check in
    /// SAFETY.md.
    ///
    /// # Errors
    ///
    /// Returns why the path is refused.
    pub fn validate_deletable(&self, path: &Path) -> Result<ValidatedPath, SafetyError> {
        let (real, metadata) = self.resolve(path)?;
        let real_names = names(&real).ok_or(SafetyError::Unsupported)?;
        let (root, is_root) = self.root_of(&real_names).ok_or(SafetyError::OutsideRoots)?;
        if is_root {
            return Err(SafetyError::IsRoot);
        }

        if metadata.file_type().is_symlink() {
            self.check_link_target(&real, &root)?;
        }

        Ok(ValidatedPath {
            root: Path::new("/").join(root.join("/")),
            path: real,
            device: metadata.dev(),
            inode: metadata.ino(),
            check: Check::Cleanup,
        })
    }

    /// The first steps every check shares: refuses empty, relative, and top
    /// of disk paths, follows every link above the item but not the item
    /// itself, and refuses protected paths. Returns the real path and its
    /// details.
    fn resolve(&self, path: &Path) -> Result<(PathBuf, fs::Metadata), SafetyError> {
        self.resolve_for(path, Check::Cleanup)
    }

    /// [`Self::resolve`], with the protected folders of `check`
    fn resolve_for(
        &self,
        path: &Path,
        check: Check,
    ) -> Result<(PathBuf, fs::Metadata), SafetyError> {
        if path.as_os_str().is_empty() {
            return Err(SafetyError::Empty);
        }
        if !path.is_absolute() {
            return Err(SafetyError::Relative);
        }
        let given = names(path).ok_or(SafetyError::Unsupported)?;
        if given.is_empty() || is_top_of_disk(&given) {
            return Err(SafetyError::TopOfDisk);
        }
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(SafetyError::Unsupported);
        };

        // Follow every link above the item on disk, but not the item itself,
        // so a link is judged as the link.
        let real_parent = fs::canonicalize(parent).map_err(SafetyError::Unreadable)?;
        let real = real_parent.join(name);
        let metadata = fs::symlink_metadata(&real).map_err(SafetyError::Unreadable)?;

        let real_names = names(&real).ok_or(SafetyError::Unsupported)?;
        if is_top_of_disk(&real_names) {
            return Err(SafetyError::TopOfDisk);
        }
        if self.is_protected_for(&real_names, check) {
            return Err(SafetyError::Protected);
        }
        Ok((real, metadata))
    }

    /// Checks that an app, or one of its related files, may be moved to the
    /// Trash. See App Removal in SAFETY.md. Which related files belong to an
    /// app is up to the caller; this only allows the folders they may be in.
    ///
    /// # Errors
    ///
    /// Returns why the path is refused.
    pub fn validate_app_removal(&self, path: &Path) -> Result<ValidatedPath, SafetyError> {
        let (real, metadata) = self.resolve(path)?;
        if metadata.file_type().is_symlink() {
            return Err(SafetyError::Link);
        }
        let parent = real.parent().ok_or(SafetyError::Unsupported)?;
        let is_app = real.extension().is_some_and(|ext| ext == "app") && metadata.is_dir();
        let allowed = if self
            .application_folders()
            .iter()
            .any(|folder| parent == folder)
        {
            is_app
        } else {
            let library = self.home.join("Library");
            APP_LIBRARY_FOLDERS
                .iter()
                .any(|folder| parent == library.join(folder))
        };
        if !allowed {
            return Err(SafetyError::NotAppItem);
        }
        Ok(ValidatedPath {
            root: parent.to_path_buf(),
            device: metadata.dev(),
            inode: metadata.ino(),
            path: real,
            check: Check::AppRemoval,
        })
    }

    /// Checks that a project build folder, an installer in Downloads, or
    /// Docker's disk image may be moved to the Trash. See Clutter Removal in SAFETY.md.
    ///
    /// # Errors
    ///
    /// Returns why the path is refused.
    pub fn validate_clutter(&self, path: &Path) -> Result<ValidatedPath, SafetyError> {
        let (real, metadata) = self.resolve_for(path, Check::Clutter)?;
        if metadata.file_type().is_symlink() {
            return Err(SafetyError::Link);
        }
        let real_names = names(&real).ok_or(SafetyError::Unsupported)?;
        let home = self.home_names();
        if !starts_with_pattern(&real_names, &home) {
            return Err(SafetyError::NotClutter);
        }
        let rest = &real_names[home.len()..];
        let (Some(first), Some(name)) = (rest.first(), rest.last()) else {
            return Err(SafetyError::NotClutter);
        };
        let parent = real.parent().ok_or(SafetyError::Unsupported)?;
        let allowed = if *first == "Downloads" && rest.len() > 1 && is_installer(name) {
            true
        } else if rest == DOCKER_IMAGE {
            metadata.is_file()
        } else {
            // Tools keep their own copies in ~/Library and hidden folders,
            // and a build folder inside another is part of it.
            let inside_build = rest[..rest.len() - 1]
                .iter()
                .any(|part| *part == "node_modules" || *part == "target");
            let is_build = metadata.is_dir()
                && match *name {
                    "node_modules" => true,
                    "target" => parent.join("Cargo.toml").is_file(),
                    _ => false,
                };
            is_build
                && rest.len() > 1
                && *first != "Library"
                && !first.starts_with('.')
                && !inside_build
        };
        if !allowed {
            return Err(SafetyError::NotClutter);
        }
        Ok(ValidatedPath {
            root: parent.to_path_buf(),
            device: metadata.dev(),
            inode: metadata.ino(),
            path: real,
            check: Check::Clutter,
        })
    }

    /// Runs the same check that made `validated` again, on its path.
    ///
    /// # Errors
    ///
    /// Returns why the path is now refused.
    pub fn validate_again(&self, validated: &ValidatedPath) -> Result<ValidatedPath, SafetyError> {
        match validated.check {
            Check::Cleanup => self.validate_deletable(validated.path()),
            Check::AppRemoval => self.validate_app_removal(validated.path()),
            Check::Clutter => self.validate_clutter(validated.path()),
        }
    }

    /// A link may only lead somewhere inside its own root, and not to a
    /// protected path.
    fn check_link_target(&self, link: &Path, root: &[&str]) -> Result<(), SafetyError> {
        let target = fs::canonicalize(link).map_err(|_| SafetyError::LinkLeadsOut)?;
        let target_names = names(&target).ok_or(SafetyError::LinkLeadsOut)?;
        let inside_root = target_names.len() > root.len() && target_names.starts_with(root);
        if !inside_root || self.is_protected(&target_names) {
            return Err(SafetyError::LinkLeadsOut);
        }
        Ok(())
    }

    /// Whether a rule path pattern stays inside a cleanup root. The fixed part
    /// must cover a whole root before any `*`, apart from a `*` the root
    /// itself has, and the pattern must reach below the root.
    #[must_use]
    pub fn covers_pattern(&self, pattern: &Path) -> bool {
        let Some(pattern) = names(pattern) else {
            return false;
        };
        let home = self.home_names();
        if pattern.len() <= home.len() || pattern[..home.len()] != home[..] {
            return false;
        }
        let rest = &pattern[home.len()..];
        ROOTS.iter().any(|root| {
            let root: Vec<&str> = root.split('/').collect();
            rest.len() > root.len()
                && root
                    .iter()
                    .zip(rest)
                    .all(|(want, got)| *want == "*" || (want == got && *got != "*"))
        }) && !self.is_protected(&pattern)
    }
}

/// Whether `name` ends in one of the installer endings, in any case
fn is_installer(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            INSTALLER_ENDINGS
                .iter()
                .any(|ending| extension.eq_ignore_ascii_case(ending))
        })
}

/// `/`, or the top folder of a disk under `/Volumes`.
fn is_top_of_disk(path: &[&str]) -> bool {
    path.is_empty() || (path.first() == Some(&"Volumes") && path.len() <= 2)
}

#[cfg(test)]
mod tests {
    use super::{Check, CleanupRoots, SafetyError};
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use tempfile::{TempDir, tempdir};

    /// A fake home folder with a few cleanup roots and protected folders.
    fn home() -> (TempDir, CleanupRoots, PathBuf) {
        let dir = tempdir().expect("temporary directory should be created");
        let home = fs::canonicalize(dir.path()).expect("home should resolve");
        for folder in [
            "Library/Caches/com.example.app",
            "Library/Containers/com.example.sandboxed/Data/Library/Caches/data",
            "Library/Containers/com.example.sandboxed/Data/Documents",
            "Library/Developer/Xcode/DerivedData/Project-abc",
            "Library/Developer/Xcode/Archives/2026-09-28",
            "Documents",
            ".ssh",
            "Library/Caches/repo/.git",
        ] {
            fs::create_dir_all(home.join(folder)).expect("folder should be created");
        }
        let roots = CleanupRoots::new(&home).expect("roots should be made");
        (dir, roots, home)
    }

    fn refused(roots: &CleanupRoots, path: &Path) -> SafetyError {
        roots
            .validate_deletable(path)
            .expect_err("path should be refused")
    }

    #[test]
    fn accepts_items_inside_a_root() {
        let (_dir, roots, home) = home();

        let cache = roots
            .validate_deletable(&home.join("Library/Caches/com.example.app"))
            .expect("cache folder should be accepted");
        let derived = roots
            .validate_deletable(&home.join("Library/Developer/Xcode/DerivedData/Project-abc"))
            .expect("DerivedData project should be accepted");

        assert_eq!(cache.path(), home.join("Library/Caches/com.example.app"));
        assert_eq!(cache.root(), home.join("Library/Caches"));
        assert_eq!(
            derived.root(),
            home.join("Library/Developer/Xcode/DerivedData")
        );
        assert!(cache.inode() > 0);
    }

    #[test]
    fn accepts_only_the_caches_folder_of_a_container() {
        let (_dir, roots, home) = home();
        let container = home.join("Library/Containers/com.example.sandboxed/Data");

        assert!(
            roots
                .validate_deletable(&container.join("Library/Caches/data"))
                .is_ok()
        );
        assert!(matches!(
            refused(&roots, &container.join("Documents")),
            SafetyError::OutsideRoots
        ));
        assert!(matches!(
            refused(&roots, &container.join("Library/Caches")),
            SafetyError::IsRoot
        ));
    }

    #[test]
    fn refuses_empty_relative_and_top_level_paths() {
        let (_dir, roots, _home) = home();

        assert!(matches!(refused(&roots, Path::new("")), SafetyError::Empty));
        assert!(matches!(
            refused(&roots, Path::new("Library/Caches/x")),
            SafetyError::Relative
        ));
        assert!(matches!(
            refused(&roots, Path::new("/")),
            SafetyError::TopOfDisk
        ));
        assert!(matches!(
            refused(&roots, Path::new("/Volumes/USB")),
            SafetyError::TopOfDisk
        ));
    }

    #[test]
    fn refuses_a_root_itself_and_the_home_folder() {
        let (_dir, roots, home) = home();

        assert!(matches!(
            refused(&roots, &home.join("Library/Caches")),
            SafetyError::IsRoot
        ));
        assert!(matches!(refused(&roots, &home), SafetyError::Protected));
    }

    #[test]
    fn refuses_paths_outside_the_roots() {
        let (_dir, roots, home) = home();
        fs::create_dir_all(home.join("Library/Application Support/app"))
            .expect("folder should be created");

        assert!(matches!(
            refused(&roots, &home.join("Library/Application Support/app")),
            SafetyError::OutsideRoots
        ));
        assert!(matches!(
            refused(
                &roots,
                &home.join("Library/Developer/Xcode/Archives/2026-09-28")
            ),
            SafetyError::Protected
        ));
    }

    #[test]
    fn refuses_dot_and_dot_dot_parts() {
        let (_dir, roots, home) = home();
        let sneaky = home.join("Library/Caches/com.example.app/../../../Documents");
        let dotted = PathBuf::from(format!(
            "{}/Library/Caches/./com.example.app",
            home.display()
        ));

        assert!(matches!(refused(&roots, &sneaky), SafetyError::Unsupported));
        assert!(roots.validate_deletable(&dotted).is_ok());
    }

    #[test]
    fn refuses_protected_paths_written_in_other_ways() {
        let (_dir, roots, home) = home();
        let doubled = PathBuf::from(format!("{}//.ssh", home.display()));

        assert!(matches!(refused(&roots, &doubled), SafetyError::Protected));
        assert!(matches!(
            refused(&roots, &home.join("Library/Caches/repo/.git")),
            SafetyError::Protected
        ));
    }

    #[test]
    fn refuses_a_parent_link_that_leads_into_a_protected_folder() {
        let (_dir, roots, home) = home();
        fs::write(home.join("Documents/essay.txt"), "mine").expect("file should be written");
        symlink(home.join("Documents"), home.join("Library/Caches/docs"))
            .expect("link should be created");

        let through_link = home.join("Library/Caches/docs/essay.txt");

        assert!(matches!(
            refused(&roots, &through_link),
            SafetyError::Protected
        ));
    }

    #[test]
    fn judges_a_link_by_where_it_leads() {
        let (_dir, roots, home) = home();
        let caches = home.join("Library/Caches");
        symlink(home.join("Documents"), caches.join("out")).expect("link should be created");
        symlink(caches.join("com.example.app"), caches.join("in")).expect("link should be created");
        symlink(caches.join("missing"), caches.join("broken")).expect("link should be created");

        assert!(matches!(
            refused(&roots, &caches.join("out")),
            SafetyError::LinkLeadsOut
        ));
        assert!(matches!(
            refused(&roots, &caches.join("broken")),
            SafetyError::LinkLeadsOut
        ));
        assert!(roots.validate_deletable(&caches.join("in")).is_ok());
    }

    #[test]
    fn refuses_names_that_are_not_utf8() {
        let (_dir, roots, home) = home();
        let name = OsStr::from_bytes(b"bad\xff");
        let path = home.join("Library/Caches").join(name);

        assert!(matches!(refused(&roots, &path), SafetyError::Unsupported));
    }

    #[test]
    fn refuses_a_path_that_does_not_exist() {
        let (_dir, roots, home) = home();

        assert!(matches!(
            refused(&roots, &home.join("Library/Caches/missing")),
            SafetyError::Unreadable(_)
        ));
    }

    #[test]
    fn patterns_must_cover_a_whole_root_before_any_star() {
        let (_dir, roots, home) = home();
        let covers = |pattern: &str| roots.covers_pattern(&home.join(pattern));

        assert!(covers("Library/Caches/com.example.*"));
        assert!(covers("Library/Caches/*/data"));
        assert!(covers("Library/Developer/Xcode/DerivedData/*"));
        assert!(covers("Library/Containers/*/Data/Library/Caches/*"));
        assert!(!covers("Library/Caches"));
        assert!(!covers("Library/*/Caches"));
        assert!(!covers("Library/Application Support/*"));
        assert!(!covers("Library/Developer/Xcode/Archives/*"));
        assert!(!covers("Documents/*"));
        assert!(!roots.covers_pattern(Path::new("/tmp/Library/Caches/x")));
    }

    /// A home folder and a shared Applications folder, each with an app, a
    /// link to an app, an app in a subfolder, and some Library files.
    fn apps() -> (TempDir, TempDir, CleanupRoots) {
        let home = tempdir().expect("home should be created");
        let shared = tempdir().expect("applications should be created");
        for folder in ["Example.app/Contents", "Utilities/Tool.app/Contents"] {
            fs::create_dir_all(shared.path().join(folder)).expect("folder should be created");
        }
        fs::write(shared.path().join("notes.txt"), "x").expect("file should be written");
        symlink(
            shared.path().join("Example.app"),
            shared.path().join("Linked.app"),
        )
        .expect("link should be created");
        for folder in [
            "Applications/Mine.app/Contents",
            "Library/Caches/com.example.app",
            "Library/Application Support/Example/deep",
            "Library/Preferences",
            "Library/Keychains/login",
            "Documents",
        ] {
            fs::create_dir_all(home.path().join(folder)).expect("folder should be created");
        }
        fs::write(
            home.path()
                .join("Library/Preferences/com.example.app.plist"),
            "x",
        )
        .expect("file should be written");
        let roots = CleanupRoots::with_applications(home.path(), shared.path())
            .expect("roots should be made");
        (home, shared, roots)
    }

    #[test]
    fn app_removal_allows_apps_and_library_items_only() {
        let (home, shared, roots) = apps();

        for allowed in [
            shared.path().join("Example.app"),
            home.path().join("Applications/Mine.app"),
            home.path().join("Library/Caches/com.example.app"),
            home.path()
                .join("Library/Preferences/com.example.app.plist"),
            home.path().join("Library/Application Support/Example"),
        ] {
            let validated = roots
                .validate_app_removal(&allowed)
                .unwrap_or_else(|error| panic!("{} should pass: {error}", allowed.display()));
            assert_eq!(validated.check(), Check::AppRemoval);
            assert!(roots.validate_again(&validated).is_ok());
        }
    }

    #[test]
    fn app_removal_refuses_everything_else() {
        let (home, shared, roots) = apps();

        for (path, want) in [
            (shared.path().join("Linked.app"), "link"),
            (shared.path().join("Utilities/Tool.app"), "not an app"),
            (shared.path().join("Utilities"), "not an app"),
            (shared.path().join("notes.txt"), "not an app"),
            (shared.path().join("Example.app/Contents"), "not an app"),
            (
                home.path().join("Library/Application Support/Example/deep"),
                "not an app",
            ),
            (home.path().join("Library/Caches"), "not an app"),
            (home.path().join("Library/Keychains/login"), "protected"),
            (home.path().join("Documents"), "protected"),
            (home.path().to_path_buf(), "protected"),
        ] {
            let error = roots
                .validate_app_removal(&path)
                .expect_err(&format!("{} should be refused", path.display()));
            assert!(
                error.to_string().contains(want),
                "{}: {error}",
                path.display()
            );
        }
    }

    #[test]
    fn a_cleanup_path_is_checked_again_by_the_cleanup_check() {
        let (_dir, roots, home) = home();
        let validated = roots
            .validate_deletable(&home.join("Library/Caches/com.example.app"))
            .expect("cache should pass");

        assert_eq!(validated.check(), Check::Cleanup);
        assert_eq!(
            roots.validate_again(&validated).expect("still passes"),
            validated
        );
    }

    #[test]
    fn tests_do_not_run_as_root() {
        assert!(!super::running_as_root());
    }

    #[test]
    fn clutter_takes_build_folders_and_installers_from_your_folders() {
        let (_dir, roots, home) = home();
        for folder in [
            "Documents/web/node_modules/dep/node_modules",
            "Documents/tool/target/debug",
            "Desktop/loose/target",
            "Downloads/apps/Big.DMG",
            "code/app/node_modules",
        ] {
            fs::create_dir_all(home.join(folder)).expect("folder should be created");
        }
        fs::write(home.join("Documents/tool/Cargo.toml"), "").expect("file should be written");
        fs::write(home.join("Downloads/Tool.pkg"), "").expect("file should be written");
        fs::write(home.join("Downloads/notes.pdf"), "").expect("file should be written");

        for ok in [
            "Documents/web/node_modules",
            "Documents/tool/target",
            "code/app/node_modules",
            "Downloads/Tool.pkg",
            "Downloads/apps/Big.DMG",
        ] {
            let validated = roots
                .validate_clutter(&home.join(ok))
                .unwrap_or_else(|error| panic!("{ok} should be accepted: {error}"));
            assert_eq!(validated.check(), Check::Clutter);
            assert!(roots.validate_again(&validated).is_ok());
        }
        for refused in [
            // A build folder inside another, and a target with no Cargo.toml
            "Documents/web/node_modules/dep/node_modules",
            "Desktop/loose/target",
            // Not an installer, and Downloads itself
            "Downloads/notes.pdf",
            "Downloads",
            "Documents",
        ] {
            assert!(
                matches!(
                    roots.validate_clutter(&home.join(refused)),
                    Err(SafetyError::NotClutter)
                ),
                "{refused} should be refused"
            );
        }
        // Cleanup still refuses your own folders.
        assert!(matches!(
            refused(&roots, &home.join("Documents/web/node_modules")),
            SafetyError::Protected
        ));
    }

    #[test]
    fn clutter_takes_docker_disk_image_and_nothing_beside_it() {
        let (_dir, roots, home) = home();
        let image = super::DOCKER_IMAGE
            .iter()
            .fold(home.clone(), |path, name| path.join(name));
        let data = image.parent().expect("image should have a folder");
        fs::create_dir_all(data).expect("folder should be created");
        fs::write(&image, "").expect("file should be written");
        fs::write(data.join("Docker.qcow2"), "").expect("file should be written");

        let validated = roots
            .validate_clutter(&image)
            .unwrap_or_else(|error| panic!("Docker.raw should be accepted: {error}"));
        assert_eq!(validated.check(), Check::Clutter);
        for refused in [data.join("Docker.qcow2"), data.to_path_buf()] {
            assert!(
                matches!(
                    roots.validate_clutter(&refused),
                    Err(SafetyError::NotClutter)
                ),
                "{} should be refused",
                refused.display()
            );
        }

        // A folder in its place is refused too.
        fs::remove_file(&image).expect("file should be removed");
        fs::create_dir(&image).expect("folder should be created");
        assert!(roots.validate_clutter(&image).is_err());
    }

    #[test]
    fn clutter_refuses_tool_folders_links_git_and_keys() {
        let (_dir, roots, home) = home();
        for folder in [
            "Library/App/node_modules",
            ".vscode/extension/node_modules",
            "code/repo/.git/node_modules",
            ".ssh/node_modules",
            "code/real/node_modules",
        ] {
            fs::create_dir_all(home.join(folder)).expect("folder should be created");
        }
        fs::create_dir_all(home.join("code/linked")).expect("folder should be created");
        symlink(
            home.join("code/real/node_modules"),
            home.join("code/linked/node_modules"),
        )
        .expect("link should be made");

        for path in ["Library/App/node_modules", ".vscode/extension/node_modules"] {
            assert!(matches!(
                roots.validate_clutter(&home.join(path)),
                Err(SafetyError::NotClutter)
            ));
        }
        for path in ["code/repo/.git/node_modules", ".ssh/node_modules"] {
            assert!(matches!(
                roots.validate_clutter(&home.join(path)),
                Err(SafetyError::Protected)
            ));
        }
        assert!(matches!(
            roots.validate_clutter(&home.join("code/linked/node_modules")),
            Err(SafetyError::Link)
        ));
    }
}
