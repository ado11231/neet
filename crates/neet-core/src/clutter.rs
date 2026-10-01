//! Things that take space but neet does not clean itself, so you can remove
//! them with the right tool. Measured from the scan or asked of macOS.
//! Nothing here changes a file.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::scan;
use crate::tree::{NodeId, NodeKind, Tree};

/// What kind of clutter a finding is
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Items in the Trash, which take space until it is emptied
    Trash,
    /// Disk images and installer packages in Downloads
    Installers,
    /// `node_modules` folders, and Rust `target` folders next to a
    /// `Cargo.toml`, in your projects
    BuildFolders,
    /// The disk image Docker Desktop keeps its containers and images in
    DockerImage,
    /// iOS and other simulator runtimes Xcode downloaded
    SimulatorRuntimes,
    /// Your temporary and cache folders under `/private/var/folders`
    TempFiles,
}

/// One kind of clutter and how much space it takes
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub kind: Kind,
    pub size: u64,
    /// How many items, such as installers or build folders
    pub count: usize,
    /// The largest item, when it is in the scan, to show in Disk
    pub node: Option<NodeId>,
}

/// File name endings of disk images and installers
const INSTALLERS: &[&str] = &["dmg", "pkg", "iso", "xip"];

/// Where Docker Desktop keeps its disk image, from the home folder
const DOCKER_IMAGE: &[&str] = &[
    "Library",
    "Containers",
    "com.docker.docker",
    "Data",
    "vms",
    "0",
    "data",
    "Docker.raw",
];

/// The findings that come from the scan of the home folder. Kinds with
/// nothing found are left out.
#[must_use]
pub fn from_tree(tree: &Tree) -> Vec<Finding> {
    let root = tree.root();
    let mut found = Vec::new();
    if let Some(trash) = child_path(tree, root, &[".Trash"]) {
        found.push(single(
            tree,
            Kind::Trash,
            trash,
            tree.get(trash).total_items,
        ));
    }
    if let Some(image) = child_path(tree, root, DOCKER_IMAGE) {
        found.push(single(tree, Kind::DockerImage, image, 1));
    }
    if let Some(downloads) = child_path(tree, root, &["Downloads"]) {
        let installers: Vec<NodeId> = tree
            .iter()
            .filter(|(_, node)| node.kind == NodeKind::File && is_installer(&node.name))
            .map(|(id, _)| id)
            .filter(|&id| is_inside(tree, id, downloads))
            .collect();
        found.push(group(tree, Kind::Installers, &installers));
    }
    let builds: Vec<NodeId> = tree
        .iter()
        .filter(|(id, node)| node.kind == NodeKind::Directory && is_build_folder(tree, *id))
        .map(|(id, _)| id)
        .collect();
    found.push(group(tree, Kind::BuildFolders, &builds));
    found.retain(|finding| finding.size > 0);
    found
}

fn single(tree: &Tree, kind: Kind, id: NodeId, count: u64) -> Finding {
    Finding {
        kind,
        size: tree.get(id).total_size,
        count: usize::try_from(count).unwrap_or(usize::MAX),
        node: Some(id),
    }
}

fn group(tree: &Tree, kind: Kind, ids: &[NodeId]) -> Finding {
    Finding {
        kind,
        size: ids.iter().map(|&id| tree.get(id).total_size).sum(),
        count: ids.len(),
        node: ids
            .iter()
            .copied()
            .max_by_key(|&id| tree.get(id).total_size),
    }
}

/// The node at `names` below `from`, if the scan has it
fn child_path(tree: &Tree, from: NodeId, names: &[&str]) -> Option<NodeId> {
    names.iter().try_fold(from, |current, name| {
        tree.get(current)
            .children
            .iter()
            .copied()
            .find(|&child| tree.get(child).name == OsStr::new(name))
    })
}

fn is_installer(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            INSTALLERS
                .iter()
                .any(|installer| extension.eq_ignore_ascii_case(installer))
        })
}

/// Whether `id` is somewhere below `folder`
fn is_inside(tree: &Tree, id: NodeId, folder: NodeId) -> bool {
    let mut current = tree.get(id).parent;
    while let Some(parent) = current {
        if parent == folder {
            return true;
        }
        current = tree.get(parent).parent;
    }
    false
}

/// A project build folder: `node_modules`, or `target` beside a
/// `Cargo.toml`. Not one inside another, nor in `~/Library` or a hidden
/// folder, where tools keep their own copies that are not yours to remove.
fn is_build_folder(tree: &Tree, id: NodeId) -> bool {
    let node = tree.get(id);
    let Some(parent) = node.parent else {
        return false;
    };
    let is_build = if node.name == "node_modules" {
        true
    } else if node.name == "target" {
        tree.get(parent)
            .children
            .iter()
            .any(|&sibling| tree.get(sibling).name == "Cargo.toml")
    } else {
        false
    };
    if !is_build {
        return false;
    }
    // Walk up to the folder just below the home folder.
    let mut current = parent;
    loop {
        let above = tree.get(current);
        if above.name == "node_modules" || above.name == "target" {
            return false;
        }
        match above.parent {
            Some(top) if top == tree.root() => {
                let name = above.name.to_string_lossy();
                return name != "Library" && !name.starts_with('.');
            }
            Some(next) => current = next,
            None => return false,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Runtime {
    #[serde(default)]
    size_bytes: u64,
}

/// The simulator runtimes Xcode downloaded, asked of `xcrun simctl`, which
/// takes a few seconds. `None` when there are no simulators on this Mac, so
/// `xcrun` is never run where it might ask to install developer tools.
///
/// # Errors
///
/// Returns an error if `simctl` could not run or gave output neet cannot read.
pub fn simulator_runtimes() -> io::Result<Option<Finding>> {
    if !Path::new("/Library/Developer/CoreSimulator").exists() {
        return Ok(None);
    }
    let output = Command::new("/usr/bin/xcrun")
        .args(["simctl", "runtime", "list", "-j"])
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(io::Error::other(message));
    }
    let runtimes: HashMap<String, Runtime> = serde_json::from_slice(&output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(Finding {
        kind: Kind::SimulatorRuntimes,
        size: runtimes.values().map(|runtime| runtime.size_bytes).sum(),
        count: runtimes.len(),
        node: None,
    }))
}

/// Your own folder under `/private/var/folders`, which holds your temporary
/// files (`T`) and caches (`C`), asked of `getconf`
///
/// # Errors
///
/// Returns an error if `getconf` gave no folder.
pub fn temp_folder() -> io::Result<PathBuf> {
    let output = Command::new("/usr/bin/getconf")
        .arg("DARWIN_USER_TEMP_DIR")
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let temp = Path::new(text.trim());
    match temp.parent() {
        Some(parent) if output.status.success() && temp.is_absolute() => Ok(parent.to_path_buf()),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "macOS gave no temporary folder",
        )),
    }
}

/// Measures your temporary and cache folders with the same scan as the home
/// folder. Folders it cannot read are left out of the size.
///
/// # Errors
///
/// Returns an error if the folder cannot be found or read.
pub fn temp_files() -> io::Result<Finding> {
    let folder = temp_folder()?;
    let scan = scan::scan(&folder, |_| {})?;
    let root = scan.tree.get(scan.tree.root());
    Ok(Finding {
        kind: Kind::TempFiles,
        size: root.total_size,
        count: usize::try_from(root.total_items).unwrap_or(usize::MAX),
        node: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{Kind, from_tree, temp_folder};
    use crate::tree::{NodeKind, Tree};

    fn kinds(tree: &Tree) -> Vec<(Kind, u64, usize)> {
        from_tree(tree)
            .into_iter()
            .map(|finding| (finding.kind, finding.size, finding.count))
            .collect()
    }

    #[test]
    fn finds_trash_installers_and_build_folders() {
        let mut tree = Tree::new("/Users/someone");
        let root = tree.root();
        let trash = tree.add(root, ".Trash", NodeKind::Directory, 0);
        let _ = tree.add(trash, "old.txt", NodeKind::File, 10);
        let downloads = tree.add(root, "Downloads", NodeKind::Directory, 0);
        let _ = tree.add(downloads, "App.dmg", NodeKind::File, 100);
        let _ = tree.add(downloads, "Tool.PKG", NodeKind::File, 50);
        let _ = tree.add(downloads, "notes.pdf", NodeKind::File, 7);
        let code = tree.add(root, "code", NodeKind::Directory, 0);
        let web = tree.add(code, "web", NodeKind::Directory, 0);
        let modules = tree.add(web, "node_modules", NodeKind::Directory, 0);
        let nested = tree.add(modules, "dep", NodeKind::Directory, 0);
        let inner = tree.add(nested, "node_modules", NodeKind::Directory, 0);
        let _ = tree.add(inner, "x.js", NodeKind::File, 30);
        let rust = tree.add(code, "tool", NodeKind::Directory, 0);
        let _ = tree.add(rust, "Cargo.toml", NodeKind::File, 1);
        let target = tree.add(rust, "target", NodeKind::Directory, 0);
        let _ = tree.add(target, "bin", NodeKind::File, 200);

        let found = kinds(&tree);

        assert!(found.contains(&(Kind::Trash, 10, 1)));
        assert!(found.contains(&(Kind::Installers, 150, 2)));
        // The nested node_modules is counted once, inside its outer folder.
        assert!(found.contains(&(Kind::BuildFolders, 230, 2)));
    }

    #[test]
    fn leaves_out_tool_folders_and_lone_targets() {
        let mut tree = Tree::new("/Users/someone");
        let root = tree.root();
        let vscode = tree.add(root, ".vscode", NodeKind::Directory, 0);
        let extension = tree.add(vscode, "extension", NodeKind::Directory, 0);
        let modules = tree.add(extension, "node_modules", NodeKind::Directory, 0);
        let _ = tree.add(modules, "x.js", NodeKind::File, 30);
        let library = tree.add(root, "Library", NodeKind::Directory, 0);
        let app = tree.add(library, "App", NodeKind::Directory, 0);
        let modules = tree.add(app, "node_modules", NodeKind::Directory, 0);
        let _ = tree.add(modules, "y.js", NodeKind::File, 40);
        let project = tree.add(root, "project", NodeKind::Directory, 0);
        let target = tree.add(project, "target", NodeKind::Directory, 0);
        let _ = tree.add(target, "out", NodeKind::File, 50);

        assert!(kinds(&tree).is_empty(), "{:?}", kinds(&tree));
    }

    #[test]
    fn finds_the_docker_image() {
        let mut tree = Tree::new("/Users/someone");
        let mut folder = tree.root();
        for name in &super::DOCKER_IMAGE[..super::DOCKER_IMAGE.len() - 1] {
            folder = tree.add(folder, *name, NodeKind::Directory, 0);
        }
        let _ = tree.add(folder, "Docker.raw", NodeKind::File, 999);

        assert_eq!(kinds(&tree), [(Kind::DockerImage, 999, 1)]);
    }

    #[test]
    fn the_temp_folder_is_under_var_folders() {
        let folder = temp_folder().expect("macOS should give a temporary folder");

        assert!(folder.starts_with("/var/folders") || folder.starts_with("/private/var/folders"));
    }
}
