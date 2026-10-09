//! Turning your own launch agents off and back on with `launchctl`. The
//! plist is never changed. See Startup Items › What May Change in
//! `docs/SAFETY.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::{Item, Kind, Runs};
use crate::rewrite::Opened;
use crate::run::Captured;

/// Which way an item is turned
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Turn {
    Off,
    On,
}

/// The state before neet turned an item, saved in
/// `~/.local/state/neet/startup/<label>.json`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub label: String,
    pub plist: PathBuf,
    pub was_off: bool,
    pub was_loaded: bool,
    /// Which way neet turned it
    pub turned: Turn,
    /// When, in UTC, such as `2026-10-08T16-30-12.123Z`
    pub at: String,
}

/// A launch agent to turn, held unchanged while the question is on screen
#[derive(Debug, Clone)]
pub struct Asked {
    pub turn: Turn,
    pub label: String,
    pub program: Option<PathBuf>,
    pub plist: PathBuf,
    opened: Opened,
    was_off: bool,
    was_loaded: bool,
}

/// The folder neet saves the state before in
fn folder(home: &Path) -> PathBuf {
    home.join(".local/state/neet/startup")
}

/// What neet last saved for `label`, if anything
#[must_use]
pub fn saved(home: &Path, label: &str) -> Option<Saved> {
    if !safe_label(label) {
        return None;
    }
    let text = fs::read_to_string(folder(home).join(format!("{label}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn safe_label(label: &str) -> bool {
    !label.is_empty() && !label.contains('/') && !label.starts_with('.')
}

/// Checks `item` may be turned, and opens its plist so a change after the
/// question is caught. Runs nothing.
///
/// # Errors
///
/// Returns why, in plain words, when the item may not be turned.
pub fn ask(item: &Item, home: &Path) -> Result<Asked, String> {
    if item.kind != Kind::YourAgent {
        return Err("Only your own launch agents can be turned off here.".to_string());
    }
    let Some(label) = item.label.clone() else {
        return Err("Its plist has no label, so launchctl cannot name it.".to_string());
    };
    if label.starts_with("com.apple.") {
        return Err("It belongs to macOS, so it stays as it is.".to_string());
    }
    if !safe_label(&label) {
        return Err(format!("Its label {label} is not one neet can save."));
    }
    if item.file.parent() != Some(home.join("Library/LaunchAgents").as_path()) {
        return Err("Its plist is not in ~/Library/LaunchAgents.".to_string());
    }
    let opened = Opened::open(&item.file)
        .map_err(|error| format!("Its plist could not be read: {error}."))?;
    let home_real = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if !opened.path().starts_with(&home_real) {
        return Err("Its plist is a link that leads out of your home folder.".to_string());
    }
    Ok(Asked {
        turn: if item.off { Turn::On } else { Turn::Off },
        label,
        program: item.program.clone(),
        plist: item.file.clone(),
        opened,
        was_off: item.off,
        was_loaded: matches!(item.runs, Runs::Running(_) | Runs::Waiting),
    })
}

impl Asked {
    /// The `launchctl` commands this runs, in order, for the user `uid`
    #[must_use]
    pub fn commands(&self, uid: u32) -> [Vec<String>; 2] {
        let service = format!("gui/{uid}/{}", self.label);
        match self.turn {
            Turn::Off => [
                vec!["disable".to_string(), service.clone()],
                vec!["bootout".to_string(), service],
            ],
            Turn::On => [
                vec!["enable".to_string(), service],
                vec![
                    "bootstrap".to_string(),
                    format!("gui/{uid}"),
                    self.plist.to_string_lossy().into_owned(),
                ],
            ],
        }
    }

    /// Turns it for the user neet runs as: saves the state before, then
    /// runs `launchctl`.
    ///
    /// # Errors
    ///
    /// Returns why, when nothing ran or a command failed.
    pub fn apply(&self, home: &Path) -> Result<(), String> {
        let uid = rustix::process::getuid().as_raw();
        let label_now = |path: &Path| {
            super::plist_json(path)
                .ok()
                .and_then(|json| super::launchd(&json).label)
        };
        self.apply_with(home, uid, SystemTime::now(), &label_now, &mut |args| {
            super::run("/bin/launchctl", args)
        })
    }

    fn apply_with(
        &self,
        home: &Path,
        uid: u32,
        now: SystemTime,
        label_now: &dyn Fn(&Path) -> Option<String>,
        launchctl: &mut dyn FnMut(&[&str]) -> Result<Captured, String>,
    ) -> Result<(), String> {
        if !self.opened.is_unchanged().unwrap_or(false) {
            return Err("Its plist changed since the question. Nothing was run.".to_string());
        }
        if label_now(self.opened.path()).as_deref() != Some(self.label.as_str()) {
            return Err("Its plist names a different label now. Nothing was run.".to_string());
        }
        self.save(home, now).map_err(|error| {
            format!("The state before could not be saved: {error}. Nothing was run.")
        })?;
        let [first, second] = self.commands(uid);
        let run = |launchctl: &mut dyn FnMut(&[&str]) -> Result<Captured, String>,
                   args: &[String]| {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            launchctl(&args)
        };
        let first_done = run(launchctl, &first)?;
        if !first_done.success {
            return Err(format!(
                "launchctl {} failed: {}",
                first[0],
                first_done.stderr.trim()
            ));
        }
        let second_done = run(launchctl, &second)?;
        if second_done.success {
            return Ok(());
        }
        // Not loaded when turned off, or already loaded when turned on, is
        // what was wanted.
        let loaded = launchctl(&["print", &format!("gui/{uid}/{}", self.label)])
            .is_ok_and(|captured| captured.success);
        match (self.turn, loaded) {
            (Turn::Off, false) | (Turn::On, true) => Ok(()),
            (Turn::Off, true) => Err(format!(
                "It is turned off, but still running: {}",
                second_done.stderr.trim()
            )),
            (Turn::On, false) => Err(format!(
                "It is turned on, but launchctl could not load it: {}",
                second_done.stderr.trim()
            )),
        }
    }

    /// Writes the state before, through a copy, so a half written file is
    /// never left
    fn save(&self, home: &Path, now: SystemTime) -> std::io::Result<()> {
        let folder = folder(home);
        fs::create_dir_all(&folder)?;
        let saved = Saved {
            label: self.label.clone(),
            plist: self.plist.clone(),
            was_off: self.was_off,
            was_loaded: self.was_loaded,
            turned: self.turn,
            at: crate::rewrite::stamp(now),
        };
        let text = serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)?;
        let path = folder.join(format!("{}.json", self.label));
        let copy = folder.join(format!(".{}.json.new", self.label));
        fs::write(&copy, text)?;
        fs::rename(&copy, &path)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::{Duration, UNIX_EPOCH};

    use super::super::Signed;
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, Item) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        let agents = home.join("Library/LaunchAgents");
        fs::create_dir_all(&agents).unwrap();
        let plist = agents.join("com.google.wake.plist");
        fs::write(&plist, "<plist/>").unwrap();
        let item = Item {
            kind: Kind::YourAgent,
            name: "com.google.wake".to_string(),
            label: Some("com.google.wake".to_string()),
            file: plist,
            program: Some(PathBuf::from("/x/updater")),
            runs: Runs::Waiting,
            off: false,
            at_start: true,
            signed: Signed::By("Google LLC".to_string()),
            app: None,
            problem: None,
        };
        (dir, home, item)
    }

    fn ok() -> Captured {
        Captured {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    fn failed(stderr: &str) -> Captured {
        Captured {
            success: false,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    /// Runs `asked` with `answer` deciding what each command prints, and
    /// returns the result and the commands that ran
    fn run(
        asked: &Asked,
        home: &Path,
        answer: impl Fn(&[&str]) -> Captured,
    ) -> (Result<(), String>, Vec<String>) {
        let ran = RefCell::new(Vec::new());
        let label = |_: &Path| Some(asked.label.clone());
        let result = asked.apply_with(
            home,
            501,
            UNIX_EPOCH + Duration::from_secs(1_791_417_600),
            &label,
            &mut |args| {
                ran.borrow_mut().push(args.join(" "));
                Ok(answer(args))
            },
        );
        (result, ran.into_inner())
    }

    #[test]
    fn turns_off_then_back_on_and_saves_the_state_before() {
        let (_dir, home, mut item) = setup();
        let asked = ask(&item, &home).unwrap();
        assert_eq!(asked.turn, Turn::Off);
        let (result, ran) = run(&asked, &home, |_| ok());
        assert_eq!(result, Ok(()));
        assert_eq!(
            ran,
            [
                "disable gui/501/com.google.wake",
                "bootout gui/501/com.google.wake"
            ]
        );
        let state = saved(&home, "com.google.wake").unwrap();
        assert_eq!(state.turned, Turn::Off);
        assert!(!state.was_off);
        assert!(state.was_loaded);
        assert_eq!(state.plist, item.file);
        assert!(state.at.starts_with("2026-10-08T"), "{}", state.at);

        item.off = true;
        item.runs = Runs::No;
        let asked = ask(&item, &home).unwrap();
        assert_eq!(asked.turn, Turn::On);
        let (result, ran) = run(&asked, &home, |_| ok());
        assert_eq!(result, Ok(()));
        assert_eq!(
            ran,
            [
                "enable gui/501/com.google.wake".to_string(),
                format!("bootstrap gui/501 {}", item.file.display()),
            ]
        );
        let state = saved(&home, "com.google.wake").unwrap();
        assert_eq!(state.turned, Turn::On);
        assert!(state.was_off);
        assert_eq!(fs::read_to_string(&item.file).unwrap(), "<plist/>");
    }

    #[test]
    fn not_loaded_or_already_loaded_is_not_an_error() {
        let (_dir, home, mut item) = setup();
        let asked = ask(&item, &home).unwrap();
        let (result, ran) = run(&asked, &home, |args| match args[0] {
            "bootout" => failed("Boot-out failed: 3: No such process"),
            "print" => failed("Could not find service"),
            _ => ok(),
        });
        assert_eq!(result, Ok(()));
        assert_eq!(ran.len(), 3);

        item.off = true;
        let asked = ask(&item, &home).unwrap();
        let (result, _) = run(&asked, &home, |args| match args[0] {
            "bootstrap" => failed("Bootstrap failed: 5: Input/output error"),
            _ => ok(),
        });
        assert_eq!(result, Ok(()));
        let (result, _) = run(&asked, &home, |args| match args[0] {
            "bootstrap" => failed("Bootstrap failed: 5: Input/output error"),
            "print" => failed("Could not find service"),
            _ => ok(),
        });
        assert!(result.unwrap_err().contains("could not load it"));
    }

    #[test]
    fn a_failed_disable_stops_before_bootout() {
        let (_dir, home, item) = setup();
        let asked = ask(&item, &home).unwrap();
        let (result, ran) = run(&asked, &home, |_| failed("Operation not permitted"));
        assert!(result.unwrap_err().contains("Operation not permitted"));
        assert_eq!(ran, ["disable gui/501/com.google.wake"]);
    }

    #[test]
    fn refuses_items_that_are_not_your_own_agents() {
        let (_dir, home, item) = setup();
        let refused = |change: &dyn Fn(&mut Item)| {
            let mut item = item.clone();
            change(&mut item);
            ask(&item, &home).unwrap_err()
        };
        assert!(refused(&|item| item.kind = Kind::AgentForAll).contains("Only your own"));
        assert!(refused(&|item| item.kind = Kind::Background).contains("Only your own"));
        assert!(refused(&|item| item.label = None).contains("no label"));
        assert!(refused(&|item| item.label = Some("com.apple.x".into())).contains("macOS"));
        assert!(refused(&|item| item.label = Some("a/../b".into())).contains("not one"));
        assert!(
            refused(&|item| item.file = PathBuf::from("/Library/LaunchAgents/x.plist"))
                .contains("not in ~/Library/LaunchAgents")
        );
    }

    #[test]
    fn refuses_a_link_out_of_home() {
        let (_dir, home, mut item) = setup();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("x.plist");
        fs::write(&target, "<plist/>").unwrap();
        let link = home.join("Library/LaunchAgents/x.plist");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        item.file = link;
        assert!(
            ask(&item, &home)
                .unwrap_err()
                .contains("leads out of your home")
        );
    }

    #[test]
    fn refuses_a_plist_changed_since_the_question() {
        let (_dir, home, item) = setup();
        let asked = ask(&item, &home).unwrap();
        fs::write(&item.file, "<plist>changed</plist>").unwrap();
        let (result, ran) = run(&asked, &home, |_| ok());
        assert!(result.unwrap_err().contains("changed since the question"));
        assert_eq!(ran, Vec::<String>::new());
        assert!(saved(&home, "com.google.wake").is_none());

        let asked = ask(&item, &home).unwrap();
        let ran = RefCell::new(0);
        let result = asked.apply_with(
            &home,
            501,
            SystemTime::now(),
            &|_| Some("com.other".to_string()),
            &mut |_| {
                *ran.borrow_mut() += 1;
                Ok(ok())
            },
        );
        assert!(result.unwrap_err().contains("different label"));
        assert_eq!(ran.into_inner(), 0);
    }
}
