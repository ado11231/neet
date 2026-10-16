//! Power: what `pmset` reports, what keeps the Mac awake, and reviewed
//! changes that run `sudo pmset` once, with the old values saved for undo.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::program::Program;
use crate::run::capture;

/// How long `pmset` may take to report
const TIMEOUT: Duration = Duration::from_secs(10);

/// The power sources `pmset` keeps settings for
pub const BATTERY: &str = "Battery Power";
pub const AC: &str = "AC Power";

/// The battery, from `pmset -g batt`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    /// Such as `charging`, `discharging`, or `charged`
    pub state: String,
    /// Such as `3:12`, when macOS has an estimate
    pub remaining: Option<String>,
}

/// One thing keeping the Mac awake, from `pmset -g assertions`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Awake {
    pub pid: u32,
    pub process: String,
    /// What it stops, such as `PreventUserIdleSystemSleep`
    pub kind: String,
    /// The reason it gives, such as `caffeinate command-line tool`
    pub reason: String,
}

impl Awake {
    /// What `kind` stops, in plain words
    #[must_use]
    pub fn stops(&self) -> &'static str {
        match self.kind.as_str() {
            "PreventUserIdleSystemSleep" | "PreventSystemSleep" => "stops sleep",
            "PreventUserIdleDisplaySleep" | "NoDisplaySleepAssertion" => "keeps the display on",
            "UserIsActive" => "you are using the Mac",
            "BackgroundTask" => "a background task",
            "NetworkClientActive" => "network activity",
            _ => "other",
        }
    }
}

/// Everything the Power tab shows
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Power {
    /// The source in use now, such as `AC Power`
    pub source: String,
    pub battery: Option<Battery>,
    /// Each source's settings, such as `displaysleep` → `10`
    pub settings: BTreeMap<String, BTreeMap<String, String>>,
    /// Settings this Mac supports, from `pmset -g cap`
    pub supported: Vec<String>,
    /// `pmset disablesleep 1` was run, so the Mac never sleeps on its own
    pub sleep_disabled: bool,
    pub awake: Vec<Awake>,
}

impl Power {
    /// A setting's value for the source in use, else for any source
    #[must_use]
    pub fn value(&self, key: &str) -> Option<&str> {
        self.settings
            .get(&self.source)
            .and_then(|settings| settings.get(key))
            .or_else(|| {
                self.settings
                    .values()
                    .find_map(|settings| settings.get(key))
            })
            .map(String::as_str)
    }

    /// Each source's value for `key`
    #[must_use]
    pub fn values(&self, key: &str) -> BTreeMap<String, String> {
        self.settings
            .iter()
            .filter_map(|(source, settings)| {
                settings
                    .get(key)
                    .map(|value| (source.clone(), value.clone()))
            })
            .collect()
    }
}

fn pmset(args: &[&str]) -> io::Result<String> {
    let captured = capture(Command::new("/usr/bin/pmset").args(args), TIMEOUT)?
        .ok_or_else(|| io::Error::other("pmset took too long"))?;
    if captured.success {
        Ok(captured.stdout)
    } else {
        Err(io::Error::other(captured.stderr.trim().to_string()))
    }
}

/// Reads power settings, the battery, and what keeps the Mac awake. Needs
/// no admin rights.
///
/// # Errors
/// Returns an error if `pmset` could not report its settings.
pub fn read() -> io::Result<Power> {
    let custom = pmset(&["-g", "custom"])?;
    let mut power = Power {
        settings: parse_custom(&custom),
        ..Power::default()
    };
    if let Ok(battery) = pmset(&["-g", "batt"]) {
        (power.source, power.battery) = parse_battery(&battery);
    }
    if let Ok(capabilities) = pmset(&["-g", "cap"]) {
        power.supported = parse_capabilities(&capabilities);
    }
    if let Ok(current) = pmset(&["-g"]) {
        power.sleep_disabled = parse_sleep_disabled(&current);
    }
    if let Ok(assertions) = pmset(&["-g", "assertions"]) {
        power.awake = parse_assertions(&assertions);
    }
    Ok(power)
}

/// `pmset -g custom`: a heading per source, then `key value` lines
fn parse_custom(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut settings: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut source = None;
    for line in text.lines() {
        if let Some(heading) = line.strip_suffix(':').filter(|_| !line.starts_with(' ')) {
            source = Some(heading.trim().to_string());
            continue;
        }
        let Some(source) = &source else {
            continue;
        };
        // Keys can hold spaces, such as `Sleep On Power Button`, so the
        // value is the last word.
        let line = line.trim();
        if let Some((key, value)) = line.rsplit_once(char::is_whitespace) {
            settings
                .entry(source.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    settings
}

/// `pmset -g batt`: the source in use, and the battery if there is one
fn parse_battery(text: &str) -> (String, Option<Battery>) {
    let source = text
        .lines()
        .find_map(|line| line.split_once("drawing from '"))
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(source, _)| source.to_string())
        .unwrap_or_default();
    let battery = text
        .lines()
        .find(|line| line.contains("InternalBattery"))
        .and_then(|line| {
            let (_, rest) = line.split_once('\t')?;
            let mut parts = rest.split(';').map(str::trim);
            let percent = parts.next()?.trim_end_matches('%').parse().ok()?;
            let state = parts.next()?.to_string();
            let remaining = parts
                .next()
                .and_then(|part| part.split_whitespace().next())
                .filter(|time| time.contains(':') && *time != "0:00")
                .map(str::to_string);
            Some(Battery {
                percent,
                state,
                remaining,
            })
        });
    (source, battery)
}

/// `pmset -g cap`: a heading, then one setting a line
fn parse_capabilities(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| !line.starts_with("Capabilities"))
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// `pmset -g` starts with system-wide settings, such as `SleepDisabled 1`
fn parse_sleep_disabled(text: &str) -> bool {
    text.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.next() == Some("SleepDisabled") && words.next() == Some("1")
    })
}

/// `pmset -g assertions`: the lines under `Listed by owning process`, such
/// as `pid 85421(caffeinate): [0x…] 00:00:14 PreventUserIdleSystemSleep
/// named: "caffeinate command-line tool"`
fn parse_assertions(text: &str) -> Vec<Awake> {
    let mut found = Vec::new();
    let mut listed = false;
    for line in text.lines() {
        if line.starts_with("Listed by owning process") {
            listed = true;
            continue;
        }
        if line.starts_with("Kernel Assertions") {
            break;
        }
        let line = line.trim();
        if !listed || !line.starts_with("pid ") {
            continue;
        }
        let parse = || {
            let rest = line.strip_prefix("pid ")?;
            let (pid, rest) = rest.split_once('(')?;
            let (process, rest) = rest.split_once("):")?;
            let (before, reason) = rest.split_once("named:").unwrap_or((rest, ""));
            let kind = before.split_whitespace().last()?.to_string();
            Some(Awake {
                pid: pid.trim().parse().ok()?,
                process: process.to_string(),
                kind,
                reason: reason.trim().trim_matches('"').to_string(),
            })
        };
        if let Some(awake) = parse() {
            found.push(awake);
        }
    }
    found
}

/// One power setting neet can change, and the values it offers
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    /// The `pmset` key
    pub key: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    /// Each choice's label and `pmset` value
    pub choices: &'static [(&'static str, &'static str)],
}

impl Setting {
    /// The label for `value`, or the value in minutes
    #[must_use]
    pub fn label(&self, value: &str) -> String {
        self.choices
            .iter()
            .find(|(_, choice)| *choice == value)
            .map_or_else(
                || {
                    if value.chars().all(|c| c.is_ascii_digit()) && self.key.ends_with("sleep") {
                        format!("{value} min")
                    } else {
                        value.to_string()
                    }
                },
                |(label, _)| (*label).to_string(),
            )
    }
}

/// The power settings neet can change, in the order shown
pub const SETTINGS: [Setting; 6] = [
    Setting {
        key: "lowpowermode",
        name: "Low Power Mode",
        about: "Saves battery by slowing the Mac a little and pausing some background work.",
        choices: &[("Off", "0"), ("On", "1")],
    },
    Setting {
        key: "displaysleep",
        name: "Display sleep",
        about: "Minutes idle before the display turns off.",
        choices: &[
            ("2 min", "2"),
            ("5 min", "5"),
            ("10 min", "10"),
            ("15 min", "15"),
            ("30 min", "30"),
            ("Never", "0"),
        ],
    },
    Setting {
        key: "sleep",
        name: "System sleep",
        about: "Minutes idle before the Mac sleeps. On Apple silicon laptops it follows the display.",
        choices: &[
            ("1 min", "1"),
            ("10 min", "10"),
            ("30 min", "30"),
            ("60 min", "60"),
            ("Never", "0"),
        ],
    },
    Setting {
        key: "disksleep",
        name: "Disk sleep",
        about: "Minutes idle before disks spin down. Most Macs have no disk that spins.",
        choices: &[("10 min", "10"), ("Never", "0")],
    },
    Setting {
        key: "powernap",
        name: "Power Nap",
        about: "Lets the Mac check mail, iCloud, and backups while it sleeps.",
        choices: &[("Off", "0"), ("On", "1")],
    },
    Setting {
        key: "womp",
        name: "Wake for network",
        about: "Lets other devices wake the Mac over the network, such as to reach shared files.",
        choices: &[("Off", "0"), ("On", "1")],
    },
];

/// The key that keeps the Mac from sleeping at all
pub const DISABLE_SLEEP: &str = "disablesleep";

/// What `sudo pmset` will change, and what it was
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub key: String,
    /// Each source's value before, to save for undo
    pub before: BTreeMap<String, String>,
    /// The new value for every source
    pub to: String,
}

impl Change {
    /// A change of `setting` to `to` for every source.
    ///
    /// # Errors
    /// Refuses a value the setting does not offer.
    pub fn new(power: &Power, setting: &Setting, to: &str) -> io::Result<Self> {
        if !setting.choices.iter().any(|(_, value)| *value == to) {
            return Err(io::Error::other("that value is not one neet offers"));
        }
        Ok(Self {
            key: setting.key.to_string(),
            before: power.values(setting.key),
            to: to.to_string(),
        })
    }

    /// Lets the Mac sleep again, after `pmset disablesleep 1`
    #[must_use]
    pub fn allow_sleep() -> Self {
        Self {
            key: DISABLE_SLEEP.to_string(),
            before: BTreeMap::from([("All".to_string(), "1".to_string())]),
            to: "0".to_string(),
        }
    }

    /// The command, run once in the terminal so `sudo` can ask for your
    /// password
    #[must_use]
    pub fn programs(&self, home: &Path) -> Vec<Program> {
        vec![sudo_pmset(home, "-a", &self.key, &self.to)]
    }
}

fn sudo_pmset(home: &Path, scope: &str, key: &str, value: &str) -> Program {
    Program {
        program: "sudo",
        args: vec![
            "/usr/bin/pmset".into(),
            scope.into(),
            key.into(),
            value.into(),
        ],
        folder: home.to_path_buf(),
    }
}

/// The values from before neet's first change to each power setting,
/// saved under `~/.local/state/neet/settings/power.json`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// Each key's values by source
    pub before: BTreeMap<String, BTreeMap<String, String>>,
}

impl Saved {
    #[must_use]
    pub fn path(home: &Path) -> PathBuf {
        home.join(".local/state/neet/settings/power.json")
    }

    /// Reads what was saved. A missing file is an empty record.
    ///
    /// # Errors
    /// Returns an error if the file exists but cannot be read.
    pub fn load(home: &Path) -> io::Result<Self> {
        super::load_json(&Self::path(home))
    }

    /// Keeps `change`'s old values, unless an older change already did, so
    /// undo goes back to how the Mac was before neet.
    ///
    /// # Errors
    /// Returns an error if the record cannot be written.
    pub fn remember(&mut self, home: &Path, change: &Change) -> io::Result<()> {
        if !self.before.contains_key(&change.key) {
            self.before
                .insert(change.key.clone(), change.before.clone());
            super::save_json(&Self::path(home), self)?;
        }
        Ok(())
    }

    /// The commands that put `key` back as it was, a source at a time
    #[must_use]
    pub fn undo(&self, home: &Path, key: &str) -> Option<Vec<Program>> {
        let before = self.before.get(key)?;
        let scope = |source: &str| match source {
            BATTERY => Some("-b"),
            AC => Some("-c"),
            "All" => Some("-a"),
            _ => None,
        };
        let programs: Vec<Program> = before
            .iter()
            .filter(|(_, value)| value.chars().all(|c| c.is_ascii_digit()))
            .filter_map(|(source, value)| Some(sudo_pmset(home, scope(source)?, key, value)))
            .collect();
        (!programs.is_empty()).then_some(programs)
    }

    /// Forgets `key` once it has been undone.
    ///
    /// # Errors
    /// Returns an error if the record cannot be written.
    pub fn forget(&mut self, home: &Path, key: &str) -> io::Result<()> {
        if self.before.remove(key).is_some() {
            super::save_json(&Self::path(home), self)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUSTOM: &str = "Battery Power:\n Sleep On Power Button 1\n lowpowermode         1\n displaysleep         30\n womp                 0\nAC Power:\n Sleep On Power Button 1\n lowpowermode         1\n displaysleep         10\n womp                 1\n";

    #[test]
    fn reads_each_sources_settings() {
        let settings = parse_custom(CUSTOM);

        assert_eq!(settings[BATTERY]["displaysleep"], "30");
        assert_eq!(settings[AC]["displaysleep"], "10");
        assert_eq!(settings[AC]["Sleep On Power Button"], "1");
    }

    #[test]
    fn reads_the_battery_and_source() {
        let (source, battery) = parse_battery(
            "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=22085731)\t64%; discharging; 3:12 remaining present: true\n",
        );
        assert_eq!(source, BATTERY);
        assert_eq!(
            battery,
            Some(Battery {
                percent: 64,
                state: "discharging".into(),
                remaining: Some("3:12".into()),
            })
        );
        let (source, battery) = parse_battery(
            "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true\n",
        );
        assert_eq!(source, AC);
        assert_eq!(battery.unwrap().remaining, None);
        assert_eq!(parse_battery("Now drawing from 'AC Power'\n").1, None);
    }

    #[test]
    fn reads_capabilities_and_disabled_sleep() {
        assert_eq!(
            parse_capabilities("Capabilities for AC Power:\n displaysleep\n womp\n"),
            ["displaysleep", "womp"]
        );
        assert!(parse_sleep_disabled(
            "System-wide power settings:\n SleepDisabled\t\t1\nCurrently in use:\n sleep 1\n"
        ));
        assert!(!parse_sleep_disabled(
            "System-wide power settings:\n SleepDisabled\t\t0\n"
        ));
        assert!(!parse_sleep_disabled("Currently in use:\n sleep 1\n"));
    }

    #[test]
    fn reads_what_keeps_the_mac_awake() {
        let text = "Assertion status system-wide:\n   UserIsActive 1\nListed by owning process:\n   pid 85421(caffeinate): [0x000245360001a34a] 00:00:14 PreventUserIdleSystemSleep named: \"caffeinate command-line tool\"  \n\tDetails: caffeinate asserting for 300 secs\n   pid 337(powerd): [0x0002] 01:26:38 PreventUserIdleSystemSleep named: \"Powerd - Prevent sleep while display is on\"  \nKernel Assertions: 0x100=MAGICWAKE\n   id=541  level=255 0x100=MAGICWAKE\n";

        let awake = parse_assertions(text);

        assert_eq!(awake.len(), 2);
        assert_eq!(awake[0].pid, 85421);
        assert_eq!(awake[0].process, "caffeinate");
        assert_eq!(awake[0].kind, "PreventUserIdleSystemSleep");
        assert_eq!(awake[0].reason, "caffeinate command-line tool");
        assert_eq!(awake[0].stops(), "stops sleep");
    }

    #[test]
    fn changes_run_sudo_pmset_once_and_undo_puts_each_source_back() {
        let home = tempfile::tempdir().unwrap();
        let power = Power {
            source: AC.into(),
            settings: parse_custom(CUSTOM),
            ..Power::default()
        };
        let setting = SETTINGS
            .iter()
            .find(|setting| setting.key == "displaysleep")
            .unwrap();
        assert!(Change::new(&power, setting, "7").is_err());
        let change = Change::new(&power, setting, "5").unwrap();

        let programs = change.programs(home.path());
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].shown(), "sudo /usr/bin/pmset -a displaysleep 5");

        let mut saved = Saved::load(home.path()).unwrap();
        saved.remember(home.path(), &change).unwrap();
        // A second change keeps the values from before the first.
        let later = Change::new(&power, setting, "2").map(|mut later| {
            later.before = BTreeMap::from([(AC.into(), "5".into())]);
            later
        });
        saved.remember(home.path(), &later.unwrap()).unwrap();
        let saved = Saved::load(home.path()).unwrap();
        let undo: Vec<String> = saved
            .undo(home.path(), "displaysleep")
            .unwrap()
            .iter()
            .map(Program::shown)
            .collect();
        assert_eq!(
            undo,
            [
                "sudo /usr/bin/pmset -c displaysleep 10",
                "sudo /usr/bin/pmset -b displaysleep 30"
            ]
        );
        assert!(saved.undo(home.path(), "womp").is_none());
    }

    #[test]
    fn allowing_sleep_turns_off_disablesleep() {
        let home = Path::new("/Users/test");
        assert_eq!(
            Change::allow_sleep().programs(home)[0].shown(),
            "sudo /usr/bin/pmset -a disablesleep 0"
        );
        let saved = Saved {
            before: BTreeMap::from([(
                DISABLE_SLEEP.into(),
                BTreeMap::from([("All".into(), "1".into())]),
            )]),
        };
        assert_eq!(
            saved.undo(home, DISABLE_SLEEP).unwrap()[0].shown(),
            "sudo /usr/bin/pmset -a disablesleep 1"
        );
    }

    #[test]
    fn labels_choices_and_unknown_minutes() {
        let display = SETTINGS[1];
        assert_eq!(display.label("0"), "Never");
        assert_eq!(display.label("45"), "45 min");
        assert_eq!(SETTINGS[0].label("1"), "On");
    }
}
