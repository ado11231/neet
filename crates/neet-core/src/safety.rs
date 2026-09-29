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

/// System folders that are never touched, with everything inside.
const PROTECTED_SYSTEM: &[&str] = &["/System", "/usr", "/Library"];

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
}

/// The cleanup roots and protected paths for one home folder
#[derive(Debug, Clone)]
pub struct CleanupRoots {
    home: PathBuf,
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
        Ok(Self {
            home: fs::canonicalize(home)?,
        })
    }

    /// The real home folder
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
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
        let home = self.home_names();
        let in_home = starts_with_pattern(path, &home);
        let protected_in_home = in_home
            && PROTECTED_IN_HOME.iter().any(|protected| {
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
        if self.is_protected(&real_names) {
            return Err(SafetyError::Protected);
        }
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
        })
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

/// `/`, or the top folder of a disk under `/Volumes`.
fn is_top_of_disk(path: &[&str]) -> bool {
    path.is_empty() || (path.first() == Some(&"Volumes") && path.len() <= 2)
}

#[cfg(test)]
mod tests {
    use super::{CleanupRoots, SafetyError};
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
}
