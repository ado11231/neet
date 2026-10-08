//! Programs that start on their own: launch agents and daemons, and the
//! helpers apps keep inside themselves and ask macOS to run in the
//! background. Only reads. See Startup Items in `docs/SAFETY.md`.
//!
//! Plists are read with `plutil`, whether each is turned on and runs with
//! `launchctl`, and who signed each program with `codesign --display`. None
//! of them runs a listed program, and none asks for admin rights. Apps that
//! open at login are not listed: macOS shows that list only to an admin.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// How long one command may take
const TIMEOUT: Duration = Duration::from_secs(10);

/// What kind of startup item
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A helper inside an app, which the app asked macOS to run
    Background,
    /// A plist in `~/Library/LaunchAgents`
    YourAgent,
    /// A plist in `/Library/LaunchAgents`, for every user
    AgentForAll,
    /// A plist in `/Library/LaunchDaemons`, run by the system
    Daemon,
}

impl Kind {
    pub const ALL: [Self; 4] = [
        Self::Background,
        Self::YourAgent,
        Self::AgentForAll,
        Self::Daemon,
    ];

    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Background => "Allowed in the background",
            Self::YourAgent => "Your launch agents",
            Self::AgentForAll => "Launch agents for every user",
            Self::Daemon => "Launch daemons",
        }
    }

    /// What this kind is, in a few plain words
    #[must_use]
    pub fn about(self) -> &'static str {
        match self {
            Self::Background => "Helpers inside apps, which the app asked macOS to run.",
            Self::YourAgent => "Programs set up to run for you, in ~/Library/LaunchAgents.",
            Self::AgentForAll => {
                "Programs an admin set up to run for every user, in /Library/LaunchAgents."
            }
            Self::Daemon => "Programs the system runs for every user, in /Library/LaunchDaemons.",
        }
    }
}

/// Whether an item runs now
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runs {
    /// Running, with this process ID
    Running(u32),
    /// launchd has it, and starts it when it is needed.
    Waiting,
    /// launchd has not loaded it.
    No,
    /// neet could not tell.
    Unknown,
}

/// Who signed the program
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signed {
    /// Apple's own
    Apple,
    /// A developer, by name, such as `Google LLC`
    By(String),
    /// From the App Store, by its team ID
    AppStore(String),
    Unsigned,
    /// The program is missing, or `codesign` could not tell.
    Unknown,
}

/// One program that starts on its own
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    /// The launchd label, or the plist's name when it has none
    pub name: String,
    pub label: Option<String>,
    /// The plist, or for a login item the helper app
    pub file: PathBuf,
    pub program: Option<PathBuf>,
    pub runs: Runs,
    /// Turned off with `launchctl disable`, in System Settings, or by the
    /// plist's `Disabled` key
    pub off: bool,
    /// Starts when you log in or the Mac starts, not only when asked for
    pub at_start: bool,
    pub signed: Signed,
    /// The app it is inside, for background items
    pub app: Option<String>,
    /// Something wrong with it, in plain words
    pub problem: Option<String>,
}

/// Everything Startup shows
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub items: Vec<Item>,
    /// Why some items may be missing, when they may be
    pub incomplete: Option<String>,
    /// Items that belong to macOS, which are counted, not listed
    pub from_macos: usize,
}

/// A helper inside an app
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inside {
    /// `Contents/Library/LoginItems/*.app`
    LoginItem,
    /// `Contents/Library/LaunchAgents/*.plist`
    Agent,
    /// `Contents/Library/LaunchDaemons/*.plist`
    Daemon,
}

/// A helper found inside an app
#[derive(Debug, Clone)]
pub struct Embedded {
    pub app: PathBuf,
    /// The app's bundle ID
    pub app_id: Option<String>,
    pub inside: Inside,
    pub path: PathBuf,
    /// For a login item, its bundle ID. For a plist, its contents as JSON.
    pub contents: Result<String, String>,
}

/// What the commands printed, so the listing can be tested without them
#[derive(Debug, Default)]
pub struct Read {
    /// Each plist, with its kind and its contents as JSON, or why not
    pub plists: Vec<(Kind, PathBuf, Result<String, String>)>,
    pub embedded: Vec<Embedded>,
    /// `launchctl list`
    pub gui_list: Option<String>,
    /// `launchctl print-disabled gui/<uid>`
    pub gui_disabled: Option<String>,
    /// `launchctl print-disabled system`
    pub system_disabled: Option<String>,
    /// `launchctl print system/<label>`, by label
    pub daemons: HashMap<String, String>,
}

fn run(program: &str, args: &[&str]) -> Result<crate::run::Captured, String> {
    let mut command = Command::new(program);
    command.args(args);
    match crate::run::capture(&mut command, TIMEOUT) {
        Ok(Some(captured)) => Ok(captured),
        Ok(None) => Err(format!(
            "{program} did not finish within {} seconds",
            TIMEOUT.as_secs()
        )),
        Err(error) => Err(format!("{program} could not run: {error}")),
    }
}

fn run_ok(program: &str, args: &[&str]) -> Option<String> {
    run(program, args)
        .ok()
        .filter(|captured| captured.success)
        .map(|captured| captured.stdout)
}

/// The entries in `folder` with this extension, sorted by name
fn entries(folder: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    found.sort();
    found
}

/// A plist as JSON, read with `plutil`, which never runs anything
fn plist_json(path: &Path) -> Result<String, String> {
    let captured = run(
        "/usr/bin/plutil",
        &["-convert", "json", "-o", "-", "--", &path.to_string_lossy()],
    )?;
    if captured.success {
        Ok(captured.stdout)
    } else {
        Err(captured.stderr.trim().to_string())
    }
}

/// An app's bundle ID, from its `Info.plist`
fn bundle_id(app: &Path) -> Result<String, String> {
    let captured = run(
        "/usr/bin/plutil",
        &[
            "-extract",
            "CFBundleIdentifier",
            "raw",
            "-o",
            "-",
            "--",
            &app.join("Contents/Info.plist").to_string_lossy(),
        ],
    )?;
    let id = captured.stdout.trim().to_string();
    if captured.success && !id.is_empty() {
        Ok(id)
    } else {
        Err("Its bundle ID could not be read.".to_string())
    }
}

/// The helpers inside the apps in `folder`
fn embedded(folder: &Path) -> Vec<Embedded> {
    let mut found = Vec::new();
    for app in entries(folder, "app") {
        let library = app.join("Contents/Library");
        let helpers: Vec<(Inside, PathBuf)> = [
            (Inside::LoginItem, "LoginItems", "app"),
            (Inside::Agent, "LaunchAgents", "plist"),
            (Inside::Daemon, "LaunchDaemons", "plist"),
        ]
        .into_iter()
        .flat_map(|(inside, sub, ext)| {
            entries(&library.join(sub), ext)
                .into_iter()
                .map(move |path| (inside, path))
        })
        .collect();
        if helpers.is_empty() {
            continue;
        }
        let app_id = bundle_id(&app).ok();
        for (inside, path) in helpers {
            let contents = match inside {
                Inside::LoginItem => bundle_id(&path),
                Inside::Agent | Inside::Daemon => plist_json(&path),
            };
            found.push(Embedded {
                app: app.clone(),
                app_id: app_id.clone(),
                inside,
                path,
                contents,
            });
        }
    }
    found
}

/// Runs the commands that read startup items for the user `uid`
#[must_use]
pub fn read(home: &Path, uid: u32) -> Read {
    let mut read = Read::default();
    for (kind, folder) in [
        (Kind::YourAgent, home.join("Library/LaunchAgents")),
        (Kind::AgentForAll, PathBuf::from("/Library/LaunchAgents")),
        (Kind::Daemon, PathBuf::from("/Library/LaunchDaemons")),
    ] {
        for path in entries(&folder, "plist") {
            let json = plist_json(&path);
            read.plists.push((kind, path, json));
        }
    }
    for folder in [PathBuf::from("/Applications"), home.join("Applications")] {
        read.embedded.extend(embedded(&folder));
    }
    read.gui_list = run_ok("/bin/launchctl", &["list"]);
    read.gui_disabled = run_ok("/bin/launchctl", &["print-disabled", &format!("gui/{uid}")]);
    read.system_disabled = run_ok("/bin/launchctl", &["print-disabled", "system"]);
    let plist_daemons = read
        .plists
        .iter()
        .filter(|(kind, _, _)| *kind == Kind::Daemon)
        .map(|(_, _, json)| json);
    let embedded_daemons = read
        .embedded
        .iter()
        .filter(|helper| helper.inside == Inside::Daemon)
        .map(|helper| &helper.contents);
    let labels: Vec<String> = plist_daemons
        .chain(embedded_daemons)
        .filter_map(|json| json.as_ref().ok().and_then(|json| launchd(json).label))
        .collect();
    for label in labels {
        if let Ok(captured) = run("/bin/launchctl", &["print", &format!("system/{label}")]) {
            read.daemons.insert(label, captured.stdout);
        }
    }
    read
}

/// Lists startup items for `home` and the user `uid`. Changes nothing.
#[must_use]
pub fn list(home: &Path, uid: u32) -> Listing {
    listing(&read(home, uid), &signed)
}

/// What a launchd plist says
#[derive(Debug, Default, PartialEq, Eq)]
struct Launchd {
    label: Option<String>,
    program: Option<PathBuf>,
    /// `BundleProgram`: the program, from the app's folder
    bundle_program: Option<PathBuf>,
    at_start: bool,
    disabled: bool,
    empty: bool,
}

fn launchd(json: &str) -> Launchd {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Launchd::default();
    };
    let string = |key: &str| map.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let program = string("Program").or_else(|| {
        map.get("ProgramArguments")
            .and_then(|v| v.as_array())
            .and_then(|args| args.first())
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });
    // KeepAlive can be a dictionary of conditions; any of them keeps it going.
    let keep_alive = match map.get("KeepAlive") {
        Some(serde_json::Value::Bool(on)) => *on,
        Some(serde_json::Value::Object(_)) => true,
        _ => false,
    };
    Launchd {
        label: string("Label"),
        program: program.map(PathBuf::from),
        bundle_program: string("BundleProgram").map(PathBuf::from),
        at_start: map.get("RunAtLoad").and_then(serde_json::Value::as_bool) == Some(true)
            || keep_alive,
        disabled: map.get("Disabled").and_then(serde_json::Value::as_bool) == Some(true),
        empty: map.is_empty(),
    }
}

/// `launchctl list`: each label with its process ID, or `None` when
/// loaded but not running
fn gui_list(text: &str) -> HashMap<String, Option<u32>> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let pid = parts.next()?.trim().parse().ok();
            let _status = parts.next()?;
            let label = parts.next()?.trim();
            Some((label.to_string(), pid))
        })
        .collect()
}

/// `launchctl print-disabled`: each label it knows, and whether it is
/// turned off
fn services(text: &str) -> HashMap<String, bool> {
    text.lines()
        .filter_map(|line| {
            let (label, state) = line.trim().split_once(" => ")?;
            Some((
                label.trim().trim_matches('"').to_string(),
                state.trim() == "disabled",
            ))
        })
        .collect()
}

/// `launchctl print system/<label>`: whether it runs
fn daemon_runs(text: &str) -> Runs {
    let field = |name: &str| {
        text.lines().find_map(|line| {
            let (key, value) = line.split_once(" = ")?;
            // Only the service's own lines, one tab in, not nested ones.
            (line.starts_with('\t') && !line.starts_with("\t\t") && key.trim() == name)
                .then(|| value.trim().to_string())
        })
    };
    match (field("state").as_deref(), field("pid")) {
        (Some("running"), Some(pid)) => pid.parse().map_or(Runs::Unknown, Runs::Running),
        (Some(_), _) => Runs::Waiting,
        _ => Runs::No,
    }
}

/// Who signed `path`, from `codesign --display`, which only reads it
fn signed(path: &Path) -> Signed {
    if !path.exists() {
        return Signed::Unknown;
    }
    match run(
        "/usr/bin/codesign",
        &["--display", "--verbose=2", "--", &path.to_string_lossy()],
    ) {
        Ok(captured) => codesign(&captured.stderr),
        Err(_) => Signed::Unknown,
    }
}

/// What `codesign --display --verbose=2` printed, as who signed it
fn codesign(text: &str) -> Signed {
    if text.contains("not signed at all") {
        return Signed::Unsigned;
    }
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::to_string)
    };
    let team = field("TeamIdentifier=").filter(|team| team != "not set");
    match field("Authority=").as_deref() {
        Some("Software Signing") => Signed::Apple,
        Some("Apple Mac OS Application Signing") => Signed::AppStore(team.unwrap_or_default()),
        Some(authority) => {
            let name = authority
                .strip_prefix("Developer ID Application: ")
                .unwrap_or(authority);
            // Such as `Google LLC (EQHXZ8M8AV)`: the team ID goes.
            let name = match name.rsplit_once(" (") {
                Some((name, rest)) if rest.ends_with(')') => name,
                _ => name,
            };
            Signed::By(name.to_string())
        }
        // Signed without a certificate, such as by a build on this Mac
        None if text.contains("Signature=adhoc") => Signed::Unsigned,
        None => Signed::Unknown,
    }
}

/// What is wrong with a plist, if anything
fn problem(parsed: &Result<Launchd, &String>, program: Option<&Path>) -> Option<String> {
    match (parsed, program) {
        (Err(error), _) => Some(format!("Its plist could not be read: {error}")),
        (Ok(plist), _) if plist.empty => {
            Some("Its plist is empty, so it does nothing.".to_string())
        }
        (Ok(_), None) => Some("Its plist names no program.".to_string()),
        (Ok(_), Some(program)) if !program.exists() => Some(
            "Its program is missing, likely left behind by an app that was removed.".to_string(),
        ),
        _ => None,
    }
}

/// What launchctl said, for every item
struct Launchctl<'a> {
    gui: Option<HashMap<String, Option<u32>>>,
    gui_services: HashMap<String, bool>,
    system_services: HashMap<String, bool>,
    daemons: &'a HashMap<String, String>,
}

impl Launchctl<'_> {
    fn runs(&self, label: &str, system: bool) -> Runs {
        if system {
            return self
                .daemons
                .get(label)
                .map_or(Runs::Unknown, |text| daemon_runs(text));
        }
        match &self.gui {
            Some(gui) => match gui.get(label) {
                Some(Some(pid)) => Runs::Running(*pid),
                Some(None) => Runs::Waiting,
                None => Runs::No,
            },
            None => Runs::Unknown,
        }
    }

    /// Whether launchd knows the label, and whether it is turned off
    fn service(&self, label: &str, system: bool) -> Option<bool> {
        let services = if system {
            &self.system_services
        } else {
            &self.gui_services
        };
        services.get(label).copied()
    }
}

/// What one plist or helper turned out to be
enum Found {
    Item(Item),
    /// macOS's own, counted only
    MacOs,
    /// A helper the app never asked macOS to run
    NotAsked,
}

fn plist_item(
    launchctl: &Launchctl,
    (kind, path, json): &(Kind, PathBuf, Result<String, String>),
    signer: &dyn Fn(&Path) -> Signed,
) -> Found {
    let parsed = json.as_ref().map(|json| launchd(json));
    let plist = parsed.as_ref().ok();
    let label = plist.and_then(|plist| plist.label.clone());
    let program = plist.and_then(|plist| plist.program.clone());
    let signer_name = program.as_deref().map_or(Signed::Unknown, signer);
    if signer_name == Signed::Apple
        && label
            .as_deref()
            .is_some_and(|label| label.starts_with("com.apple."))
    {
        return Found::MacOs;
    }
    let system = *kind == Kind::Daemon;
    let runs = label
        .as_deref()
        .map_or(Runs::No, |label| launchctl.runs(label, system));
    let is_link = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let problem = problem(&parsed, program.as_deref())
        .or_else(|| is_link.then(|| "Its plist is a link.".to_string()));
    Found::Item(Item {
        kind: *kind,
        name: label.clone().unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        }),
        off: plist.is_some_and(|plist| plist.disabled)
            || label
                .as_deref()
                .is_some_and(|label| launchctl.service(label, system) == Some(true)),
        at_start: plist.is_some_and(|plist| plist.at_start),
        label,
        file: path.clone(),
        program,
        runs,
        signed: signer_name,
        app: None,
        problem,
    })
}

fn embedded_item(
    launchctl: &Launchctl,
    helper: &Embedded,
    signer: &dyn Fn(&Path) -> Signed,
) -> Found {
    if helper
        .app_id
        .as_deref()
        .is_some_and(|id| id.starts_with("com.apple."))
    {
        return Found::MacOs;
    }
    let (label, program, at_start) = match helper.inside {
        Inside::LoginItem => (
            helper.contents.clone().ok(),
            Some(helper.path.clone()),
            true,
        ),
        Inside::Agent | Inside::Daemon => {
            let Ok(plist) = helper.contents.as_deref().map(launchd) else {
                return Found::NotAsked;
            };
            let program = plist
                .bundle_program
                .or(plist.program)
                .map(|program| helper.app.join(program));
            (plist.label, program, plist.at_start)
        }
    };
    let Some(label) = label else {
        return Found::NotAsked;
    };
    let system = helper.inside == Inside::Daemon;
    let runs = launchctl.runs(&label, system);
    let service = launchctl.service(&label, system);
    // Only helpers the app has asked macOS to run are startup items.
    if service.is_none() && !matches!(runs, Runs::Running(_) | Runs::Waiting) {
        return Found::NotAsked;
    }
    // A login item is named by its helper app, such as `DockerHelper`.
    let name = match helper.inside {
        Inside::LoginItem => helper
            .path
            .file_stem()
            .map_or_else(|| label.clone(), |stem| stem.to_string_lossy().into_owned()),
        Inside::Agent | Inside::Daemon => label.clone(),
    };
    Found::Item(Item {
        kind: Kind::Background,
        name,
        off: service == Some(true),
        label: Some(label),
        file: helper.path.clone(),
        program,
        runs,
        at_start,
        signed: signer(&helper.app),
        app: helper
            .app
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned()),
        problem: None,
    })
}

/// Builds the listing from what the commands printed. `signer` is passed
/// in, so tests need no `codesign`.
fn listing(read: &Read, signer: &dyn Fn(&Path) -> Signed) -> Listing {
    let launchctl = Launchctl {
        gui: read.gui_list.as_deref().map(gui_list),
        gui_services: read
            .gui_disabled
            .as_deref()
            .map(services)
            .unwrap_or_default(),
        system_services: read
            .system_disabled
            .as_deref()
            .map(services)
            .unwrap_or_default(),
        daemons: &read.daemons,
    };
    let mut listing = Listing::default();
    if launchctl.gui.is_none() {
        listing.incomplete =
            Some("launchctl could not say what runs, so each shows not known.".to_string());
    }
    let found = read
        .plists
        .iter()
        .map(|plist| plist_item(&launchctl, plist, signer))
        .chain(
            read.embedded
                .iter()
                .map(|helper| embedded_item(&launchctl, helper, signer)),
        );
    for found in found {
        match found {
            Found::Item(item) => listing.items.push(item),
            Found::MacOs => listing.from_macos += 1,
            Found::NotAsked => {}
        }
    }
    listing
        .items
        .sort_by_key(|item| (item.kind, item.name.to_lowercase()));
    listing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_launchd_plists() {
        let plist = launchd(
            r#"{"Label":"a.b","ProgramArguments":["/x/y","--z"],"KeepAlive":{"SuccessfulExit":false}}"#,
        );
        assert_eq!(plist.label.as_deref(), Some("a.b"));
        assert_eq!(plist.program, Some(PathBuf::from("/x/y")));
        assert!(plist.at_start);
        assert!(!plist.disabled);
        let plist = launchd(
            r#"{"Label":"a","Program":"/p","BundleProgram":"Contents/MacOS/x","RunAtLoad":false,"Disabled":true}"#,
        );
        assert_eq!(plist.program, Some(PathBuf::from("/p")));
        assert_eq!(
            plist.bundle_program,
            Some(PathBuf::from("Contents/MacOS/x"))
        );
        assert!(!plist.at_start);
        assert!(plist.disabled);
        assert!(launchd("{}").empty);
        assert!(!launchd("not json").empty);
    }

    #[test]
    fn reads_launchctl() {
        let list = gui_list("PID\tStatus\tLabel\n-\t0\tcom.a\n708\t0\tcom.b\n");
        assert_eq!(list.get("com.a"), Some(&None));
        assert_eq!(list.get("com.b"), Some(&Some(708)));
        let known = services(
            "\tdisabled services = {\n\t\t\"com.a\" => disabled\n\t\t\"com.b\" => enabled\n\t}\n",
        );
        assert_eq!(known.get("com.a"), Some(&true));
        assert_eq!(known.get("com.b"), Some(&false));
        assert_eq!(known.len(), 2);
        let print = "system/com.x = {\n\tstate = running\n\tpid = 525\n\tendpoints = {\n\t\tstate = active\n\t}\n}\n";
        assert_eq!(daemon_runs(print), Runs::Running(525));
        assert_eq!(
            daemon_runs("x = {\n\tstate = not running\n}\n"),
            Runs::Waiting
        );
        assert_eq!(
            daemon_runs("Bad request.\nCould not find service"),
            Runs::No
        );
    }

    #[test]
    fn reads_who_signed_it() {
        assert_eq!(
            codesign(
                "Authority=Developer ID Application: Google LLC (EQHXZ8M8AV)\nAuthority=Apple Root CA\nTeamIdentifier=EQHXZ8M8AV\n"
            ),
            Signed::By("Google LLC".to_string())
        );
        assert_eq!(
            codesign("Authority=Software Signing\nTeamIdentifier=not set\n"),
            Signed::Apple
        );
        assert_eq!(
            codesign("Authority=Apple Mac OS Application Signing\nTeamIdentifier=VJ5N2X84K8\n"),
            Signed::AppStore("VJ5N2X84K8".to_string())
        );
        assert_eq!(
            codesign("/x: code object is not signed at all\n"),
            Signed::Unsigned
        );
        assert_eq!(
            codesign("Signature=adhoc\nTeamIdentifier=not set\n"),
            Signed::Unsigned
        );
    }

    fn plist(kind: Kind, path: &Path, json: &str) -> (Kind, PathBuf, Result<String, String>) {
        (kind, path.to_path_buf(), Ok(json.to_string()))
    }

    /// What the commands print on a Mac with one of each kind
    fn sample(dir: &Path) -> Read {
        let program = dir.join("updater");
        fs::write(&program, "").unwrap();
        let docker = dir.join("Docker.app");
        let ollama = dir.join("Ollama.app");
        Read {
            plists: vec![
                plist(
                    Kind::YourAgent,
                    &dir.join("com.google.wake.plist"),
                    &format!(
                        r#"{{"Label":"com.google.wake","ProgramArguments":["{}"],"RunAtLoad":true}}"#,
                        program.display()
                    ),
                ),
                plist(Kind::YourAgent, &dir.join("keystone.plist"), "{}"),
                plist(
                    Kind::YourAgent,
                    &dir.join("gone.plist"),
                    r#"{"Label":"com.gone","Program":"/nowhere/gone"}"#,
                ),
                (
                    Kind::YourAgent,
                    dir.join("broken.plist"),
                    Err("unexpected character".to_string()),
                ),
                plist(
                    Kind::AgentForAll,
                    Path::new("/Library/LaunchAgents/com.ws1.ws1etlmu.plist"),
                    r#"{"Label":"com.ws1.ws1etlmu","ProgramArguments":["/usr/local/bin/ws1etlmu"],"KeepAlive":true}"#,
                ),
                plist(
                    Kind::Daemon,
                    Path::new("/Library/LaunchDaemons/com.docker.vmnetd.plist"),
                    r#"{"Label":"com.docker.vmnetd","Program":"/Library/PrivilegedHelperTools/com.docker.vmnetd"}"#,
                ),
                plist(
                    Kind::YourAgent,
                    &dir.join("com.apple.mine.plist"),
                    r#"{"Label":"com.apple.mine","Program":"/usr/libexec/mine"}"#,
                ),
            ],
            embedded: vec![
                Embedded {
                    app: docker.clone(),
                    app_id: Some("com.docker.docker".to_string()),
                    inside: Inside::LoginItem,
                    path: docker.join("Contents/Library/LoginItems/DockerHelper.app"),
                    contents: Ok("com.docker.helper".to_string()),
                },
                Embedded {
                    app: ollama.clone(),
                    app_id: Some("com.electron.ollama".to_string()),
                    inside: Inside::Agent,
                    path: ollama.join("Contents/Library/LaunchAgents/com.ollama.ollama.plist"),
                    contents: Ok(r#"{"Label":"com.ollama.ollama","BundleProgram":"Contents/Frameworks/Squirrel","RunAtLoad":true}"#.to_string()),
                },
                // Inside the app, but never asked for: not a startup item
                Embedded {
                    app: dir.join("Tailscale.app"),
                    app_id: Some("io.tailscale".to_string()),
                    inside: Inside::Daemon,
                    path: dir.join("Tailscale.app/Contents/Library/LaunchDaemons/t.plist"),
                    contents: Ok(r#"{"Label":"io.tailscale.sentinel"}"#.to_string()),
                },
                Embedded {
                    app: dir.join("Xcode.app"),
                    app_id: Some("com.apple.dt.Xcode".to_string()),
                    inside: Inside::LoginItem,
                    path: dir.join("Xcode.app/Contents/Library/LoginItems/H.app"),
                    contents: Ok("com.apple.dt.helper".to_string()),
                },
            ],
            gui_list: Some(
                "PID\tStatus\tLabel\n-\t0\tcom.google.wake\n708\t0\tcom.ws1.ws1etlmu\n44\t0\tcom.docker.helper\n"
                    .to_string(),
            ),
            gui_disabled: Some(
                "\t\"com.gone\" => disabled\n\t\"com.ollama.ollama\" => disabled\n\t\"com.docker.helper\" => enabled\n"
                    .to_string(),
            ),
            system_disabled: None,
            daemons: HashMap::from([(
                "com.docker.vmnetd".to_string(),
                "\tstate = running\n\tpid = 529\n".to_string(),
            )]),
        }
    }

    #[test]
    fn lists_every_kind_and_counts_macos_items() {
        let dir = tempfile::tempdir().unwrap();
        let read = sample(dir.path());
        let ollama = dir.path().join("Ollama.app");
        let signer = |path: &Path| {
            if path.starts_with("/usr/libexec") {
                Signed::Apple
            } else {
                Signed::By("Someone".to_string())
            }
        };
        let listing = listing(&read, &signer);

        assert_eq!(listing.incomplete, None);
        assert_eq!(listing.from_macos, 2, "com.apple.mine and Xcode's helper");
        let summary: Vec<(Kind, &str, Runs, bool)> = listing
            .items
            .iter()
            .map(|item| (item.kind, item.name.as_str(), item.runs, item.off))
            .collect();
        assert_eq!(
            summary,
            [
                (Kind::Background, "com.ollama.ollama", Runs::No, true),
                (Kind::Background, "DockerHelper", Runs::Running(44), false),
                (Kind::YourAgent, "broken", Runs::No, false),
                (Kind::YourAgent, "com.gone", Runs::No, true),
                (Kind::YourAgent, "com.google.wake", Runs::Waiting, false),
                (Kind::YourAgent, "keystone", Runs::No, false),
                (
                    Kind::AgentForAll,
                    "com.ws1.ws1etlmu",
                    Runs::Running(708),
                    false
                ),
                (Kind::Daemon, "com.docker.vmnetd", Runs::Running(529), false),
            ]
        );
        let find = |name: &str| listing.items.iter().find(|item| item.name == name).unwrap();
        let problem = |name: &str| find(name).problem.clone().unwrap_or_default();
        assert!(problem("broken").contains("could not be read"));
        assert!(problem("keystone").contains("empty"));
        assert!(problem("com.gone").contains("missing"));
        assert_eq!(find("com.google.wake").problem, None);
        assert!(find("com.google.wake").at_start);
        let agent = find("com.ollama.ollama");
        assert_eq!(agent.app.as_deref(), Some("Ollama"));
        assert_eq!(
            agent.program,
            Some(ollama.join("Contents/Frameworks/Squirrel"))
        );
    }

    #[test]
    fn says_so_when_launchctl_cannot_tell() {
        let read = Read {
            plists: vec![plist(
                Kind::YourAgent,
                Path::new("/x/a.plist"),
                r#"{"Label":"a","Program":"/bin/ls"}"#,
            )],
            ..Read::default()
        };
        let listing = listing(&read, &|_| Signed::Unknown);
        assert!(listing.incomplete.is_some());
        assert_eq!(listing.items[0].runs, Runs::Unknown);
    }
}
