//! Space only its own tool can free: simulator runtimes, through
//! `xcrun simctl`, and Docker's images and containers, through `docker`.
//! Neither can go to the Trash, so removing them is permanent. See Tools
//! neet Runs in `docs/SAFETY.md`.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

use serde::Deserialize;

/// One simulator runtime Xcode downloaded
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    /// The ID `simctl` knows it by
    pub identifier: String,
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
        if message.contains("Cannot connect to the Docker daemon") {
            return Ok(Docker::NotRunning);
        }
        return Err(failed(&output));
    }
    parse_docker_usage(&output.stdout).map(Docker::Usage)
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
    use super::{is_runtime_id, parse_docker_usage, parse_runtimes};

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
}
