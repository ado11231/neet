//! Where the disk's used space goes, past the home folder: the APFS volumes
//! `diskutil` lists, and the folders on the Data volume, measured without
//! admin rights. Looking never changes a file.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::run::capture;
use crate::size::{HardLinkTracker, allocated_size};

/// Where the Data volume is mounted, with every folder of it inside
pub const DATA: &str = "/System/Volumes/Data";

/// How long `diskutil` and `tmutil` may take
const TIMEOUT: Duration = Duration::from_secs(20);

/// What a volume is for, from the role APFS gives it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// macOS itself, read only
    System,
    /// Your files, apps, and everything else that changes
    Data,
    /// What the Mac needs to start up
    Preboot,
    /// The recovery system
    Recovery,
    /// Swap, memory macOS moved to the disk
    Vm,
    /// Room kept for the next macOS update
    Update,
    /// Any other volume in the same container
    Other,
}

impl Role {
    fn from_roles(roles: &[String]) -> Self {
        match roles.first().map(String::as_str) {
            Some("System") => Self::System,
            Some("Data") => Self::Data,
            Some("Preboot") => Self::Preboot,
            Some("Recovery") => Self::Recovery,
            Some("VM") => Self::Vm,
            Some("Update") => Self::Update,
            _ => Self::Other,
        }
    }
}

/// One APFS volume sharing the startup disk's space
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub role: Role,
    /// Space it uses now
    pub used: u64,
}

/// The startup disk's container: its size, free space, and volumes
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub total: u64,
    pub free: u64,
    /// Largest first
    pub volumes: Vec<Volume>,
}

impl Container {
    #[must_use]
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }

    /// Space the Data volume uses, if it was listed
    #[must_use]
    pub fn data(&self) -> Option<u64> {
        self.volumes
            .iter()
            .find(|volume| volume.role == Role::Data)
            .map(|volume| volume.used)
    }
}

#[derive(Deserialize)]
struct RawInfo {
    #[serde(rename = "APFSContainerReference")]
    container: String,
}

#[derive(Deserialize)]
struct RawList {
    #[serde(rename = "Containers")]
    containers: Vec<RawContainer>,
}

#[derive(Deserialize)]
struct RawContainer {
    #[serde(rename = "ContainerReference")]
    reference: String,
    #[serde(rename = "CapacityCeiling")]
    total: u64,
    #[serde(rename = "CapacityFree")]
    free: u64,
    #[serde(rename = "Volumes")]
    volumes: Vec<RawVolume>,
}

#[derive(Deserialize)]
struct RawVolume {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Roles", default)]
    roles: Vec<String>,
    #[serde(rename = "CapacityInUse")]
    used: u64,
}

/// The container named `reference`, from `diskutil apfs list -plist` as JSON
fn parse_container(json: &str, reference: &str) -> io::Result<Container> {
    let list: RawList = serde_json::from_str(json).map_err(io::Error::other)?;
    let raw = list
        .containers
        .into_iter()
        .find(|container| container.reference == reference)
        .ok_or_else(|| io::Error::other(format!("diskutil did not list {reference}")))?;
    let mut volumes: Vec<Volume> = raw
        .volumes
        .into_iter()
        .map(|volume| Volume {
            role: Role::from_roles(&volume.roles),
            name: volume.name,
            used: volume.used,
        })
        .collect();
    volumes.sort_by(|a, b| b.used.cmp(&a.used).then_with(|| a.name.cmp(&b.name)));
    Ok(Container {
        total: raw.total,
        free: raw.free,
        volumes,
    })
}

/// Runs `diskutil` with `args`, and gives its plist output as JSON
fn diskutil_json(args: &[&str]) -> io::Result<String> {
    let captured = capture(Command::new("/usr/sbin/diskutil").args(args), TIMEOUT)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "diskutil took too long"))?;
    if !captured.success {
        return Err(io::Error::other(captured.stderr.trim().to_string()));
    }
    let mut file = tempfile::NamedTempFile::new()?;
    io::Write::write_all(&mut file, captured.stdout.as_bytes())?;
    let converted = capture(
        Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-", "--"])
            .arg(file.path()),
        TIMEOUT,
    )?
    .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "plutil took too long"))?;
    if converted.success {
        Ok(converted.stdout)
    } else {
        Err(io::Error::other(converted.stderr.trim().to_string()))
    }
}

/// The container holding the startup disk, read with `diskutil`, which needs
/// no admin rights
///
/// # Errors
///
/// Returns an error if `diskutil` failed, or gave output neet cannot read.
pub fn container() -> io::Result<Container> {
    let info: RawInfo = serde_json::from_str(&diskutil_json(&["info", "-plist", "/"])?)
        .map_err(io::Error::other)?;
    parse_container(
        &diskutil_json(&["apfs", "list", "-plist"])?,
        &info.container,
    )
}

/// How many local Time Machine snapshots the startup disk keeps. Their size
/// cannot be read without admin rights.
///
/// # Errors
///
/// Returns an error if `tmutil` failed.
pub fn snapshots() -> io::Result<usize> {
    let captured = capture(
        Command::new("/usr/bin/tmutil").args(["listlocalsnapshots", "/"]),
        TIMEOUT,
    )?
    .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "tmutil took too long"))?;
    if !captured.success {
        return Err(io::Error::other(captured.stderr.trim().to_string()));
    }
    Ok(count_snapshots(&captured.stdout))
}

fn count_snapshots(text: &str) -> usize {
    text.lines()
        .filter(|line| line.trim_start().starts_with("com.apple."))
        .count()
}

/// A group of folders on the Data volume
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    /// `/Applications`
    Apps,
    /// `/Library`: files apps share between users, such as fonts, plugins,
    /// and simulator runtimes
    Library,
    /// `/System/Library/AssetsV2` and the like: files macOS downloads, such
    /// as voices, language data, and simulator runtime downloads
    SystemDownloads,
    /// `/private`: macOS's own databases, logs, sleep image, and temporary
    /// folders
    SystemData,
    /// `/opt` and `/usr/local`: Homebrew and other command line tools
    Tools,
    /// Other people's home folders and `/Users/Shared`
    OtherUsers,
    /// Every other folder at the top of the Data volume
    Other,
}

impl Area {
    /// Every area, in the order to show them
    pub const ALL: [Self; 7] = [
        Self::Apps,
        Self::Library,
        Self::SystemDownloads,
        Self::SystemData,
        Self::Tools,
        Self::OtherUsers,
        Self::Other,
    ];

    /// The area a folder at the top of the Data volume, or under its
    /// `System` folder, belongs to
    fn of(relative: &Path) -> Self {
        let mut parts = relative.components().map(std::path::Component::as_os_str);
        match parts.next().and_then(|part| part.to_str()) {
            Some("Applications") => Self::Apps,
            Some("Library") => Self::Library,
            Some("System") => Self::SystemDownloads,
            Some("private") => Self::SystemData,
            Some("opt" | "usr") => Self::Tools,
            Some("Users") => Self::OtherUsers,
            _ => Self::Other,
        }
    }
}

/// What one area holds that neet could read
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Measured {
    pub size: u64,
    /// Folders macOS would not let neet read, so the size is a floor
    pub unreadable: u64,
}

/// Measures each area of the Data volume at `data`, leaving out `home`,
/// which the home scan measures. Stays on the Data volume, so other disks
/// and volumes are never counted. Takes several seconds, so run it off the
/// screen's thread.
#[must_use]
pub fn measure(data: &Path, home: &Path) -> HashMap<Area, Measured> {
    let mut areas: HashMap<Area, Measured> = HashMap::new();
    let Ok(top) = fs::symlink_metadata(data) else {
        return areas;
    };
    let mut walk = Walk {
        device: top.dev(),
        skip: home.strip_prefix("/").map(|rest| data.join(rest)).ok(),
        links: HardLinkTracker::new(),
    };
    let Ok(entries) = fs::read_dir(data) else {
        return areas;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(data) else {
            continue;
        };
        let area = Area::of(relative);
        let found = walk.size(&path);
        let measured = areas.entry(area).or_default();
        measured.size += found.size;
        measured.unreadable += found.unreadable;
    }
    areas
}

/// Everything the breakdown shows, read together
#[derive(Clone, Debug)]
pub struct Breakdown {
    pub container: Container,
    /// `None` when `tmutil` could not say
    pub snapshots: Option<usize>,
    pub areas: HashMap<Area, Measured>,
}

/// Reads the volumes, snapshots, and the Data volume's areas, leaving out
/// `home`. Takes several seconds, so run it off the screen's thread.
///
/// # Errors
///
/// Returns an error if the volumes could not be read.
pub fn look(home: &Path) -> io::Result<Breakdown> {
    let container = container()?;
    Ok(Breakdown {
        container,
        snapshots: snapshots().ok(),
        areas: measure(Path::new(DATA), home),
    })
}

struct Walk {
    device: u64,
    skip: Option<PathBuf>,
    links: HardLinkTracker,
}

impl Walk {
    /// Space `path` and everything in it uses on this volume
    fn size(&mut self, path: &Path) -> Measured {
        let mut found = Measured::default();
        if self.skip.as_deref() == Some(path) {
            return found;
        }
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return found;
        };
        if metadata.dev() != self.device {
            return found;
        }
        if self.links.first_sighting(&metadata) {
            found.size = allocated_size(&metadata);
        }
        if !metadata.is_dir() {
            return found;
        }
        let Ok(entries) = fs::read_dir(path) else {
            found.unreadable += 1;
            return found;
        };
        for entry in entries.flatten() {
            let inner = self.size(&entry.path());
            found.size += inner.size;
            found.unreadable += inner.unreadable;
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, Role, count_snapshots, measure, parse_container};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    const LIST: &str = r#"{"Containers":[
        {"ContainerReference":"disk1","CapacityCeiling":500,"CapacityFree":400,"Volumes":[
            {"Name":"iSCPreboot","Roles":["Preboot"],"CapacityInUse":6}]},
        {"ContainerReference":"disk3","CapacityCeiling":1000,"CapacityFree":300,"Volumes":[
            {"Name":"Macintosh HD","Roles":["System"],"CapacityInUse":100},
            {"Name":"Data","Roles":["Data"],"CapacityInUse":550},
            {"Name":"VM","Roles":["VM"],"CapacityInUse":20},
            {"Name":"Extra","CapacityInUse":30}]}]}"#;

    #[test]
    fn reads_the_startup_container_and_its_volumes() {
        let container = parse_container(LIST, "disk3").expect("the container should be read");

        assert_eq!(container.total, 1000);
        assert_eq!(container.used(), 700);
        assert_eq!(container.data(), Some(550));
        let roles: Vec<Role> = container.volumes.iter().map(|v| v.role).collect();
        assert_eq!(roles, [Role::Data, Role::System, Role::Other, Role::Vm]);
    }

    #[test]
    fn a_missing_container_is_an_error() {
        assert!(parse_container(LIST, "disk9").is_err());
        assert!(parse_container("not json", "disk3").is_err());
    }

    #[test]
    fn counts_only_snapshot_lines() {
        let text = "Snapshots for disk /:\ncom.apple.TimeMachine.2026-10-16-101010.local\ncom.apple.os.update-ABC\n";

        assert_eq!(count_snapshots(text), 2);
        assert_eq!(count_snapshots("Snapshots for disk /:\n"), 0);
    }

    #[test]
    fn folders_belong_to_plain_areas() {
        assert_eq!(Area::of(Path::new("Applications")), Area::Apps);
        assert_eq!(Area::of(Path::new("System")), Area::SystemDownloads);
        assert_eq!(Area::of(Path::new("private")), Area::SystemData);
        assert_eq!(Area::of(Path::new("opt")), Area::Tools);
        assert_eq!(Area::of(Path::new("Users")), Area::OtherUsers);
        assert_eq!(Area::of(Path::new(".Spotlight-V100")), Area::Other);
    }

    #[test]
    fn measures_areas_without_the_home_folder() {
        let root = tempdir().expect("temporary directory should be created");
        let data = root.path();
        fs::create_dir_all(data.join("Applications/A.app")).expect("folder should be made");
        fs::write(data.join("Applications/A.app/bin"), vec![1; 64 * 1024])
            .expect("file should be written");
        fs::create_dir_all(data.join("Users/me")).expect("folder should be made");
        fs::write(data.join("Users/me/big"), vec![1; 256 * 1024]).expect("file should be written");
        fs::create_dir_all(data.join("Users/Shared")).expect("folder should be made");
        fs::write(data.join("Users/Shared/small"), vec![1; 8 * 1024])
            .expect("file should be written");

        let areas = measure(data, Path::new("/Users/me"));

        let apps = areas[&Area::Apps].size;
        let users = areas[&Area::OtherUsers].size;
        assert!(apps >= 64 * 1024);
        assert!(users >= 8 * 1024, "Shared is counted");
        assert!(users < 256 * 1024, "the home folder is left out");
    }

    #[test]
    fn counts_folders_it_cannot_read() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().expect("temporary directory should be created");
        let locked = root.path().join("private/locked");
        fs::create_dir_all(&locked).expect("folder should be made");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
            .expect("permissions should change");

        let areas = measure(root.path(), Path::new("/Users/me"));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755))
            .expect("permissions should change back");

        assert_eq!(areas[&Area::SystemData].unreadable, 1);
    }
}
