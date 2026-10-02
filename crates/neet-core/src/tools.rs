//! Space only its own tool can free: simulator runtimes and devices,
//! through `xcrun simctl`, and Docker's images, containers, and volumes,
//! through `docker`. None of it can go to the Trash, so removing it is
//! permanent. Resetting Docker is the one exception: its disk image goes to
//! the Trash. See Tools neet Runs in `docs/SAFETY.md`.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::safety::{CleanupRoots, DOCKER_IMAGE};
use crate::size::allocated_size;
use crate::trash;

/// One simulator runtime Xcode downloaded
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    /// The ID `simctl` knows it by
    pub identifier: String,
    /// The ID its simulators name it by, such as
    /// `com.apple.CoreSimulator.SimRuntime.iOS-18-6`
    pub runtime_identifier: String,
    /// The platform and version, such as `iOS 18.6`
    pub name: String,
    pub build: String,
    pub size: u64,
    /// When a simulator last used it, as `simctl` gives it
    pub last_used: Option<String>,
    /// Whether `simctl` lets it be deleted
    pub deletable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRuntime {
    identifier: String,
    #[serde(default)]
    runtime_identifier: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    build: String,
    #[serde(default)]
    size_bytes: u64,
    #[serde(default)]
    last_used_at: Option<String>,
    #[serde(default)]
    deletable: bool,
    #[serde(default)]
    platform_identifier: String,
}

/// The platform's name from its identifier, such as `iOS`
fn platform(identifier: &str) -> &'static str {
    match identifier {
        "com.apple.platform.iphonesimulator" => "iOS",
        "com.apple.platform.appletvsimulator" => "tvOS",
        "com.apple.platform.watchsimulator" => "watchOS",
        "com.apple.platform.xrsimulator" => "visionOS",
        _ => "Simulator",
    }
}

fn parse_runtimes(json: &[u8]) -> io::Result<Vec<Runtime>> {
    let raw: HashMap<String, RawRuntime> = serde_json::from_slice(json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut runtimes: Vec<Runtime> = raw
        .into_values()
        .map(|runtime| Runtime {
            name: format!(
                "{} {}",
                platform(&runtime.platform_identifier),
                runtime.version
            ),
            identifier: runtime.identifier,
            runtime_identifier: runtime.runtime_identifier,
            build: runtime.build,
            size: runtime.size_bytes,
            last_used: runtime.last_used_at,
            deletable: runtime.deletable,
        })
        .collect();
    runtimes.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    Ok(runtimes)
}

/// An error from a tool's output, with what it printed
fn failed(output: &Output) -> io::Error {
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.is_empty() {
        io::Error::other(format!("the tool stopped with {}", output.status))
    } else {
        io::Error::other(message)
    }
}

/// Every simulator runtime, largest first. Empty when there are no
/// simulators on this Mac, so `xcrun` is never run where it might ask to
/// install developer tools.
///
/// # Errors
///
/// Returns an error if `simctl` could not run or gave output neet cannot read.
pub fn runtimes() -> io::Result<Vec<Runtime>> {
    if !Path::new("/Library/Developer/CoreSimulator").exists() {
        return Ok(Vec::new());
    }
    let output = Command::new("/usr/bin/xcrun")
        .args(["simctl", "runtime", "list", "-j"])
        .output()?;
    if !output.status.success() {
        return Err(failed(&output));
    }
    parse_runtimes(&output.stdout)
}

/// Whether `identifier` looks like a runtime ID: letters, digits, and
/// hyphens only, so it can never be `all` or an option.
fn is_runtime_id(identifier: &str) -> bool {
    identifier.len() >= 8
        && identifier
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Asks `simctl` to delete one runtime, permanently. Simulators that are
/// running on it are shut down first.
///
/// # Errors
///
/// Returns an error if the ID is not a runtime ID, or `simctl` failed.
pub fn delete_runtime(identifier: &str) -> io::Result<()> {
    if !is_runtime_id(identifier) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a simulator runtime ID",
        ));
    }
    let output = Command::new("/usr/bin/xcrun")
        .args(["simctl", "runtime", "delete", identifier])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(&output))
    }
}

/// One simulator, such as an iPhone 16 Pro, and the runtime it runs on
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// The ID `simctl` knows it by
    pub udid: String,
    pub name: String,
    /// The runtime it runs on, such as
    /// `com.apple.CoreSimulator.SimRuntime.iOS-18-6`
    pub runtime: String,
    /// False once its runtime is gone, when it can never start again
    pub available: bool,
    /// Its apps, data, and logs
    pub size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDevice {
    udid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    data_path_size: u64,
    #[serde(default)]
    log_path_size: u64,
}

#[derive(Deserialize)]
struct RawDevices {
    devices: HashMap<String, Vec<RawDevice>>,
}

fn parse_devices(json: &[u8]) -> io::Result<Vec<Device>> {
    let raw: RawDevices = serde_json::from_slice(json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut devices: Vec<Device> = raw
        .devices
        .into_iter()
        .flat_map(|(runtime, devices)| {
            devices.into_iter().map(move |device| Device {
                udid: device.udid,
                name: device.name,
                runtime: runtime.clone(),
                available: device.is_available,
                size: device.data_path_size + device.log_path_size,
            })
        })
        .collect();
    devices.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    Ok(devices)
}

/// Every simulator device, largest first. Empty when there are no
/// simulators on this Mac.
///
/// # Errors
///
/// Returns an error if `simctl` could not run or gave output neet cannot read.
pub fn devices() -> io::Result<Vec<Device>> {
    if !Path::new("/Library/Developer/CoreSimulator").exists() {
        return Ok(Vec::new());
    }
    let output = Command::new("/usr/bin/xcrun")
        .args(["simctl", "list", "devices", "-j"])
        .output()?;
    if !output.status.success() {
        return Err(failed(&output));
    }
    parse_devices(&output.stdout)
}

/// Asks `simctl` to delete one simulator device and its data, permanently.
///
/// # Errors
///
/// Returns an error if the ID is not a device ID, or `simctl` failed.
pub fn delete_device(udid: &str) -> io::Result<()> {
    if !is_runtime_id(udid) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a simulator device ID",
        ));
    }
    let output = Command::new("/usr/bin/xcrun")
        .args(["simctl", "delete", udid])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(&output))
    }
}

/// One line of `docker system df`, such as images or the build cache.
/// Docker gives the sizes as text, such as `1.808GB`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DockerUsage {
    #[serde(rename = "Type")]
    pub kind: String,
    pub total_count: String,
    pub active: String,
    pub size: String,
    pub reclaimable: String,
}

/// What Docker could say about its space
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Docker {
    NotInstalled,
    /// Installed, but Docker Desktop is not running
    NotRunning,
    Usage(Vec<DockerUsage>),
}

/// A Docker volume no container uses, which `docker system prune` keeps
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerVolume {
    pub name: String,
    /// As Docker gives it, such as `2.812GB`
    pub size: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawVolume {
    name: String,
    #[serde(default)]
    links: String,
    #[serde(default)]
    size: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawSpace {
    #[serde(default)]
    volumes: Vec<RawVolume>,
}

fn parse_unused_volumes(json: &[u8]) -> io::Result<Vec<DockerVolume>> {
    let raw: RawSpace = serde_json::from_slice(json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(raw
        .volumes
        .into_iter()
        .filter(|volume| volume.links == "0")
        .map(|volume| DockerVolume {
            name: volume.name,
            size: volume.size,
        })
        .collect())
}

/// Whether `name` looks like a Docker volume name, so it can never be an
/// option
fn is_volume_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// The `docker` program, found where Docker Desktop and Homebrew put it
fn docker_program() -> Option<&'static str> {
    [
        "/usr/local/bin/docker",
        "/opt/homebrew/bin/docker",
        "/Applications/Docker.app/Contents/Resources/bin/docker",
    ]
    .into_iter()
    .find(|path| Path::new(path).exists())
}

fn parse_docker_usage(text: &[u8]) -> io::Result<Vec<DockerUsage>> {
    String::from_utf8_lossy(text)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        })
        .collect()
}

/// Whether `docker` failed only because Docker Desktop is not running. Older
/// versions say they cannot connect to the daemon, newer ones to the API.
fn is_not_running(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("cannot connect to the docker daemon")
        || message.contains("failed to connect to the docker api")
}

/// How much space Docker's images, containers, volumes, and build cache
/// take, asked of `docker system df`. Nothing changes.
///
/// # Errors
///
/// Returns an error if Docker gave output neet cannot read.
pub fn docker_usage() -> io::Result<Docker> {
    let Some(docker) = docker_program() else {
        return Ok(Docker::NotInstalled);
    };
    let output = Command::new(docker)
        .args(["system", "df", "--format", "json"])
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if is_not_running(&message) {
            return Ok(Docker::NotRunning);
        }
        return Err(failed(&output));
    }
    parse_docker_usage(&output.stdout).map(Docker::Usage)
}

/// The volumes no container uses, asked of `docker system df -v`. Nothing
/// changes.
///
/// # Errors
///
/// Returns an error if Docker is not installed or running, or gave output
/// neet cannot read.
pub fn unused_volumes() -> io::Result<Vec<DockerVolume>> {
    let docker = docker_program()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Docker is not installed"))?;
    let output = Command::new(docker)
        .args(["system", "df", "--verbose", "--format", "json"])
        .output()?;
    if !output.status.success() {
        return Err(failed(&output));
    }
    parse_unused_volumes(&output.stdout)
}

/// Asks Docker to remove one volume and the data in it, permanently. Docker
/// refuses if a container uses it.
///
/// # Errors
///
/// Returns an error if the name is not a volume name, or Docker failed.
pub fn remove_volume(name: &str) -> io::Result<()> {
    if !is_volume_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a Docker volume name",
        ));
    }
    let docker = docker_program()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Docker is not installed"))?;
    let output = Command::new(docker)
        .args(["volume", "rm", "--", name])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(&output))
    }
}

/// Where Docker Desktop keeps its disk image, and the space it takes. `None`
/// when there is none.
#[must_use]
pub fn docker_image(roots: &CleanupRoots) -> Option<(std::path::PathBuf, u64)> {
    let path = DOCKER_IMAGE
        .iter()
        .fold(roots.home().to_path_buf(), |path, name| path.join(name));
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    metadata
        .is_file()
        .then(|| (path, allocated_size(&metadata)))
}

/// Whether any part of Docker Desktop is still running
fn docker_is_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-f", "/Applications/Docker.app/Contents/"])
        .output()
        .map_or(true, |output| output.status.success())
}

/// How long Docker Desktop gets to stop after it says it has
const QUIT_WAIT: Duration = Duration::from_secs(60);

/// Asks Docker Desktop to stop, with `docker desktop stop`, which waits until
/// it has. Docker Desktop ignores a plain quit from `osascript`, since its
/// main process runs in the background.
fn stop_docker() -> io::Result<()> {
    let docker = docker_program()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Docker is not installed"))?;
    let output = Command::new(docker)
        .args(["desktop", "stop", "--timeout", "60"])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Docker Desktop could not be stopped ({}). Quit it from the whale menu, then try again.",
            failed(&output)
        )))
    }
}

/// Stops Docker Desktop, then moves its disk image to the Trash, where Put
/// Back can restore it while Docker Desktop is quit. Docker Desktop makes a
/// new, empty one when it opens. Every image, container, and volume goes
/// with it. Returns the space it took.
///
/// # Errors
///
/// Returns an error if Docker Desktop did not stop in time, the image fails
/// the path check, or Finder could not move it.
pub fn reset_docker(roots: &CleanupRoots) -> io::Result<u64> {
    let (path, size) = docker_image(roots)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Docker has no disk image"))?;
    if docker_is_running() {
        stop_docker()?;
        let started = Instant::now();
        while docker_is_running() {
            if started.elapsed() > QUIT_WAIT {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Docker Desktop did not stop within a minute. Quit it from the whale menu, then try again.",
                ));
            }
            thread::sleep(Duration::from_millis(500));
        }
    }
    let validated = roots.validate_clutter(&path).map_err(io::Error::other)?;
    trash::move_to_trash(validated.path())?;
    Ok(size)
}

/// Opens Docker Desktop, which starts Docker in the background.
///
/// # Errors
///
/// Returns an error if macOS could not open it.
pub fn start_docker() -> io::Result<()> {
    let output = Command::new("/usr/bin/open")
        .args(["-a", "Docker"])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(&output))
    }
}

/// Runs `docker system prune -a -f`: removes stopped containers, networks
/// no container uses, every image no container uses, and the build cache,
/// permanently. Volumes are kept. Returns the space Docker says it reclaimed.
///
/// # Errors
///
/// Returns an error if Docker is not installed, or the prune failed.
pub fn docker_prune() -> io::Result<String> {
    let docker = docker_program()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Docker is not installed"))?;
    let output = Command::new(docker)
        .args(["system", "prune", "--all", "--force"])
        .output()?;
    if !output.status.success() {
        return Err(failed(&output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .find_map(|line| line.strip_prefix("Total reclaimed space:"))
        .map_or_else(|| "0B".to_string(), |space| space.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        is_not_running, is_runtime_id, is_volume_name, parse_devices, parse_docker_usage,
        parse_runtimes, parse_unused_volumes,
    };

    #[test]
    fn reads_runtimes_largest_first() {
        let json = br#"{
            "A": {"identifier": "AAAA-1111", "version": "26.5", "build": "23F77",
                  "sizeBytes": 100, "deletable": true,
                  "platformIdentifier": "com.apple.platform.iphonesimulator"},
            "B": {"identifier": "BBBB-2222", "version": "18.6", "build": "22G86",
                  "sizeBytes": 200, "deletable": true, "lastUsedAt": "2026-09-17T22:26:41Z",
                  "platformIdentifier": "com.apple.platform.iphonesimulator"}
        }"#;

        let runtimes = parse_runtimes(json).expect("runtimes should parse");

        assert_eq!(runtimes[0].name, "iOS 18.6");
        assert_eq!(runtimes[0].size, 200);
        assert_eq!(
            runtimes[0].last_used.as_deref(),
            Some("2026-09-17T22:26:41Z")
        );
        assert_eq!(runtimes[1].identifier, "AAAA-1111");
    }

    #[test]
    fn only_runtime_ids_are_passed_to_simctl() {
        assert!(is_runtime_id("DE9F86B8-4B9B-4325-96E9-277119F7A187"));
        assert!(!is_runtime_id("all"));
        assert!(!is_runtime_id("--notUsedSinceDays"));
        assert!(!is_runtime_id("DE9F86B8 4B9B"));
    }

    #[test]
    fn reads_docker_usage() {
        let text = br#"{"Active":"2","Reclaimable":"1.105GB (61%)","Size":"1.808GB","TotalCount":"9","Type":"Images"}
{"Active":"0","Reclaimable":"0B","Size":"0B","TotalCount":"0","Type":"Build Cache"}
"#;

        let usage = parse_docker_usage(text).expect("usage should parse");

        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].kind, "Images");
        assert_eq!(usage[0].reclaimable, "1.105GB (61%)");
    }

    #[test]
    fn reads_devices_with_their_runtime() {
        let json = br#"{"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-18-6": [
                {"udid": "89B174CA-DC9E-4D90", "name": "iPhone 16 Pro", "isAvailable": false,
                 "dataPathSize": 2000, "logPathSize": 100}
            ],
            "com.apple.CoreSimulator.SimRuntime.iOS-26-5": [
                {"udid": "E6AA1A70-FBA6-4BB5", "name": "iPhone 17 Pro", "isAvailable": true,
                 "dataPathSize": 500}
            ]
        }}"#;

        let devices = parse_devices(json).expect("devices should parse");

        assert_eq!(devices[0].name, "iPhone 16 Pro");
        assert_eq!(devices[0].size, 2100);
        assert!(!devices[0].available);
        assert_eq!(
            devices[0].runtime,
            "com.apple.CoreSimulator.SimRuntime.iOS-18-6"
        );
        assert!(devices[1].available);
    }

    #[test]
    fn reads_only_volumes_no_container_uses() {
        let json = br#"{"Images": [], "Volumes": [
            {"Name": "influxdb-storage", "Links": "0", "Size": "2.812GB"},
            {"Name": "supabase_db_JrnymanApp", "Links": "1", "Size": "167.8MB"}
        ]}"#;

        let volumes = parse_unused_volumes(json).expect("volumes should parse");

        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].name, "influxdb-storage");
        assert_eq!(volumes[0].size, "2.812GB");
    }

    #[test]
    fn only_volume_names_are_passed_to_docker() {
        assert!(is_volume_name("influxdb-storage"));
        assert!(is_volume_name("supabase_edge_runtime_"));
        assert!(!is_volume_name("--all"));
        assert!(!is_volume_name("a b"));
        assert!(!is_volume_name(""));
    }

    #[test]
    fn knows_docker_is_not_running_in_old_and_new_words() {
        assert!(is_not_running(
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
        ));
        assert!(is_not_running(
            "failed to connect to the docker API at unix:///Users/me/.docker/run/docker.sock; check if the path is correct and if the daemon is running"
        ));
        assert!(!is_not_running("permission denied"));
    }
}
