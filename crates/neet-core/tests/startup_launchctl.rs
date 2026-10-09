//! An opt in round trip with the real launchctl: a throwaway launch agent in
//! an isolated home is turned off and back on. Only its label is left in
//! launchd's list of turned off services, marked enabled.

use std::fs;
use std::process::Command;

use neet_core::startup::turn::{self, Turn};
use neet_core::startup::{self, Kind, Runs};

const LABEL: &str = "dev.neet.e2e";

fn launchctl(args: &[&str]) -> (bool, String) {
    let output = Command::new("/bin/launchctl").args(args).output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
#[ignore = "changes launchd; run with cargo test -p neet-core --test startup_launchctl -- --ignored"]
fn real_launchctl_round_trip() {
    let uid = rustix::process::getuid().as_raw();
    let service = format!("gui/{uid}/{LABEL}");
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    let agents = home.join("Library/LaunchAgents");
    fs::create_dir_all(&agents).unwrap();
    let plist = agents.join(format!("{LABEL}.plist"));
    fs::write(
        &plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>/bin/sleep</string><string>600</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
"#
        ),
    )
    .unwrap();
    let before = fs::read(&plist).unwrap();
    let _ = launchctl(&["enable", &service]);
    let _ = launchctl(&["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()]);
    assert!(launchctl(&["print", &service]).0, "it should be loaded");

    let find = || {
        startup::list(&home, uid)
            .items
            .into_iter()
            .find(|item| item.label.as_deref() == Some(LABEL))
            .unwrap()
    };
    let item = find();
    assert_eq!(item.kind, Kind::YourAgent);
    assert!(!item.off);
    assert!(matches!(item.runs, Runs::Running(_)), "{:?}", item.runs);

    let asked = turn::ask(&item, &home).unwrap();
    assert_eq!(asked.turn, Turn::Off);
    asked.apply(&home).unwrap();
    assert!(!launchctl(&["print", &service]).0, "it should be unloaded");
    let item = find();
    assert!(item.off);
    assert_eq!(item.runs, Runs::No);
    // Turning it off again, when it is not loaded, is not an error.
    let mut again = item.clone();
    again.off = false;
    turn::ask(&again, &home).unwrap().apply(&home).unwrap();

    let asked = turn::ask(&item, &home).unwrap();
    assert_eq!(asked.turn, Turn::On);
    asked.apply(&home).unwrap();
    assert!(
        launchctl(&["print", &service]).0,
        "it should be loaded again"
    );
    let item = find();
    assert!(!item.off);
    assert!(matches!(item.runs, Runs::Running(_)), "{:?}", item.runs);
    // Turning it on again, when it is loaded, is not an error.
    turn::ask(&item, &home)
        .map(|mut asked| {
            asked.turn = Turn::On;
            asked
        })
        .unwrap()
        .apply(&home)
        .unwrap();

    assert_eq!(
        fs::read(&plist).unwrap(),
        before,
        "the plist is never changed"
    );
    let saved = turn::saved(&home, LABEL).unwrap();
    assert_eq!(saved.turned, Turn::On);

    let _ = launchctl(&["bootout", &service]);
}
