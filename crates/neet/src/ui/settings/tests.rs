use super::*;
use crate::ui::scan::ScanStatus;
use crate::ui::visual::tests as view;
use neet_core::settings::power::{Awake, Battery};
use std::collections::BTreeMap;

fn power(sleep_disabled: bool) -> Power {
    let source = |display: &str| {
        BTreeMap::from([
            ("lowpowermode".to_string(), "1".to_string()),
            ("displaysleep".to_string(), display.to_string()),
            ("sleep".to_string(), "1".to_string()),
            ("powernap".to_string(), "1".to_string()),
            ("womp".to_string(), "0".to_string()),
        ])
    };
    Power {
        source: power::AC.into(),
        battery: Some(Battery {
            percent: 64,
            state: "charging".into(),
            remaining: None,
        }),
        settings: BTreeMap::from([
            (power::BATTERY.to_string(), source("30")),
            (power::AC.to_string(), source("10")),
        ]),
        supported: ["lowpowermode", "displaysleep", "sleep", "powernap", "womp"]
            .map(String::from)
            .to_vec(),
        sleep_disabled,
        awake: vec![Awake {
            pid: 85421,
            process: "caffeinate".into(),
            kind: "PreventUserIdleSystemSleep".into(),
            reason: "caffeinate command-line tool".into(),
        }],
    }
}

fn screen(sleep_disabled: bool) -> (tempfile::TempDir, Settings) {
    let home = tempfile::tempdir().unwrap();
    let mac = mac::SETTINGS.iter().map(|_| Current::Unset).collect();
    let screen = Settings::with(home.path().to_path_buf(), power(sleep_disabled), mac);
    (home, screen)
}

fn render(screen: &mut Settings, width: u16, height: u16) -> String {
    let scan = ScanStatus::Failed(String::new());
    let context = Context {
        scan: &scan,
        disk: None,
        cleanable: None,
        plan: None,
    };
    view::text(&view::render("settings", screen, &context, width, height))
}

fn press(screen: &mut Settings, code: KeyCode) -> Action {
    let scan = ScanStatus::Failed(String::new());
    let context = Context {
        scan: &scan,
        disk: None,
        cleanable: None,
        plan: None,
    };
    screen.handle_key(
        KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE),
        &context,
    )
}

/// Selects the row named `name` on the current tab
fn select(screen: &mut Settings, name: &str) {
    let index = screen
        .rows()
        .iter()
        .position(|row| row_name(*row) == name)
        .unwrap_or_else(|| panic!("{name} should be listed"));
    screen.table.select(Some(index));
}

#[test]
fn power_shows_settings_per_source_battery_and_what_keeps_it_awake() {
    let (_home, mut screen) = screen(false);

    let text = render(&mut screen, 130, 36);

    for expected in [
        "Low Power Mode",
        "30 min on battery · 10 min on charger",
        "Keeping it awake",
        "caffeinate",
        "stops sleep",
        "64%",
        "charger",
    ] {
        assert!(text.contains(expected), "{expected} missing:\n{text}");
    }
    assert!(
        !text.contains("Disk sleep"),
        "unsupported settings are left out"
    );
}

#[test]
fn disabled_sleep_is_flagged_first_and_fixed_with_sudo_pmset() {
    let (home, mut screen) = screen(true);
    assert_eq!(screen.selected(), Some(Item::SleepDisabled));
    let text = render(&mut screen, 130, 36);
    assert!(text.contains("Sleep is turned off"), "{text}");
    assert!(text.contains("never sleeps"), "{text}");

    press(&mut screen, KeyCode::Enter);
    let text = render(&mut screen, 130, 36);
    assert!(
        text.contains("sudo /usr/bin/pmset -a disablesleep 0"),
        "{text}"
    );

    let Action::Run(programs) = press(&mut screen, KeyCode::Char('y')) else {
        panic!("y should hand the terminal to sudo");
    };
    assert_eq!(programs[0].shown(), "sudo /usr/bin/pmset -a disablesleep 0");
    let saved = power::Saved::load(home.path()).unwrap();
    assert!(saved.before.contains_key(power::DISABLE_SLEEP));
}

#[test]
fn a_power_change_is_picked_reviewed_and_undone() {
    let (home, mut screen) = screen(false);
    select(&mut screen, "Display sleep");

    press(&mut screen, KeyCode::Enter);
    // Starts on the value in use, 10 minutes, then moves to 15.
    press(&mut screen, KeyCode::Down);
    assert!(render(&mut screen, 130, 36).contains("sudo /usr/bin/pmset -a displaysleep 15"));
    let Action::Run(programs) = press(&mut screen, KeyCode::Char('y')) else {
        panic!("y should run sudo pmset");
    };
    assert_eq!(programs.len(), 1);
    screen.state = State::Ready(Found {
        power: Ok(power(false)),
        mac: mac::SETTINGS.iter().map(|_| Ok(Current::Unset)).collect(),
        path: neet_core::path::Listing::default(),
    });
    assert!(
        render(&mut screen, 130, 36).contains('•'),
        "marked as changed"
    );

    press(&mut screen, KeyCode::Char('u'));
    let text = render(&mut screen, 130, 36);
    assert!(text.contains("pmset -b displaysleep 30"), "{text}");
    assert!(text.contains("pmset -c displaysleep 10"), "{text}");
    let Action::Run(programs) = press(&mut screen, KeyCode::Char('y')) else {
        panic!("y should run the undo");
    };
    assert_eq!(programs.len(), 2);
    screen.ran(Ok(()));
    assert!(power::Saved::load(home.path()).unwrap().before.is_empty());
}

#[test]
fn a_failed_sudo_keeps_the_saved_values() {
    let (home, mut screen) = screen(false);
    select(&mut screen, "Power Nap");
    press(&mut screen, KeyCode::Enter);
    let _ = press(&mut screen, KeyCode::Char('y'));
    screen.ran(Err(
        "sudo /usr/bin/pmset -a powernap 1 did not finish.".into()
    ));
    assert!(screen.note.as_ref().unwrap().0.contains("did not change"));
    assert!(
        power::Saved::load(home.path())
            .unwrap()
            .before
            .contains_key("powernap")
    );
}

#[test]
fn mac_settings_are_grouped_and_show_the_command_before_anything_runs() {
    let (_home, mut screen) = screen(false);
    press(&mut screen, KeyCode::Tab);
    let text = render(&mut screen, 130, 40);
    for expected in ["Finder", "Show hidden files", "Off (default)"] {
        assert!(text.contains(expected), "{expected} missing:\n{text}");
    }
    // The list scrolls to settings further down.
    select(&mut screen, "Smart quotes");
    let text = render(&mut screen, 130, 40);
    assert!(text.contains("Keyboard & typing"), "{text}");
    assert!(text.contains("curly"), "{text}");
    screen.select_first();
    assert_eq!(screen.selected(), Some(Item::Mac(0)));

    // An unset key starts on macOS default, the last choice.
    press(&mut screen, KeyCode::Enter);
    press(&mut screen, KeyCode::Up);
    press(&mut screen, KeyCode::Up);
    let text = render(&mut screen, 130, 40);
    assert!(
        text.contains("defaults write com.apple.finder AppleShowAllFiles -bool true"),
        "{text}"
    );
    assert!(text.contains("macOS default"), "{text}");
    assert!(text.contains("Finder restarts"), "{text}");
    assert!(screen.is_dialog());
    screen.back();
    assert!(!screen.is_dialog());

    press(&mut screen, KeyCode::Char('u'));
    assert!(screen.note.as_ref().unwrap().0.contains("nothing to undo"));
}

#[test]
fn preferences_save_right_away() {
    let (home, mut screen) = screen(false);
    press(&mut screen, KeyCode::Tab);
    press(&mut screen, KeyCode::Tab);
    assert_eq!(screen.selected(), Some(Item::Pref(0)));
    assert!(render(&mut screen, 130, 36).contains("By size"));

    press(&mut screen, KeyCode::Enter);
    press(&mut screen, KeyCode::Down);
    press(&mut screen, KeyCode::Char('y'));

    let (prefs, warnings) = Prefs::load(&Prefs::path(home.path()));
    assert_eq!(prefs.disk_sort, "name");
    assert!(warnings.is_empty(), "{warnings:?}");
    press(&mut screen, KeyCode::Enter);
    assert!(render(&mut screen, 130, 36).contains("By name"));
}

#[test]
fn fits_every_screen_size() {
    for (width, height) in view::SIZES {
        for tab in 0..TABS.len() {
            let (_home, mut screen) = screen(true);
            screen.change_tab(tab);
            let text = render(&mut screen, width, height);
            assert!(text.contains("Settings"), "{width}x{height}\n{text}");
        }
    }
}

#[test]
fn a_lost_read_stops_loading() {
    let (_home, mut screen) = screen(false);
    let (sender, receiver) = mpsc::channel();
    screen.state = State::Looking(Instant::now(), receiver);
    drop(sender);
    screen.poll();
    assert!(matches!(screen.state, State::Ready(_)));
    assert!(render(&mut screen, 120, 30).contains("could not be read"));
}

/// A PATH of a folder with python3, the same folder again, a missing one,
/// and a system folder that also has python3
fn path_screen() -> (tempfile::TempDir, Settings) {
    use std::os::unix::fs::PermissionsExt;
    let (home, mut screen) = screen(false);
    let root = std::fs::canonicalize(home.path()).unwrap();
    let program = |path: &std::path::Path| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    program(&root.join("bin/python3"));
    program(&root.join("usr/bin/python3"));
    std::fs::write(root.join(".zshrc"), "path=($HOME/bin $path)\n").unwrap();
    std::fs::create_dir_all(root.join("etc")).unwrap();
    let listing = neet_core::path::read(
        &format!("{0}/bin:{0}/bin:{0}/gone:{0}/usr/bin", root.display()),
        &root,
        &root.join("etc"),
    );
    screen.home = root;
    if let State::Ready(found) = &mut screen.state {
        found.path = listing;
    }
    screen.change_tab(PATH_TAB);
    (home, screen)
}

#[test]
fn shell_path_lists_folders_problems_and_where_they_are_added() {
    let (_home, mut screen) = path_screen();
    let text = render(&mut screen, 140, 40);
    for expected in [
        "~/bin",
        "1 program",
        "repeat of #1",
        "missing",
        "~/.zshrc, line 1",
        "In more than one folder",
        "python3",
    ] {
        assert!(text.contains(expected), "{expected} missing:\n{text}");
    }

    press(&mut screen, KeyCode::Enter);
    assert!(screen.note.as_ref().unwrap().0.contains("view only"));
}

#[test]
fn slash_shows_which_copy_runs() {
    let (_home, mut screen) = path_screen();
    press(&mut screen, KeyCode::Char('/'));
    assert!(screen.takes_text());
    for c in "python3".chars() {
        press(&mut screen, KeyCode::Char(c));
    }
    press(&mut screen, KeyCode::Enter);
    assert!(!screen.takes_text());

    let text = render(&mut screen, 140, 40);
    assert!(text.contains("runs     ~/bin/python3"), "{text}");
    assert!(text.contains("hidden   ~/usr/bin/python3"), "{text}");

    press(&mut screen, KeyCode::Char('/'));
    for c in "nothing-here".chars() {
        press(&mut screen, KeyCode::Char(c));
    }
    assert!(render(&mut screen, 140, 40).contains("Not found"));
    assert!(matches!(screen.back(), Action::None));
    assert_eq!(screen.which, "");
}
