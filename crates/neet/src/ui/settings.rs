//! Settings: power through `pmset`, Mac settings through `defaults`, and
//! neet's own preferences. Enter picks a value and shows the command; `y`
//! runs it. Every change to the Mac keeps the old value for `u` undo. Shell
//! PATH folders are taken out or moved after a diff of the shell file.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Instant, SystemTime};

use neet_core::path::edit::{self, Place, Prepared};
use neet_core::program::Program;
use neet_core::rewrite::Backups;
use neet_core::settings::mac::{self, Current};
use neet_core::settings::power::{self, Power};
use neet_core::settings::prefs::{self, Prefs};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::dotfiles::{Diff, draw_diff};
use super::format;
use super::loading::Loading;
use super::tools;
use super::visual::{self, Columns};

const TABS: [&str; 4] = ["Power", "Mac settings", "Preferences", "Shell PATH"];

/// The Mac settings tab
const MAC_TAB: usize = 1;

/// The Shell PATH tab
const PATH_TAB: usize = 3;

/// What Enter offers for a PATH folder
const PATH_ACTS: [(&str, Option<Place>); 3] = [
    ("Remove from PATH", None),
    ("Move to front", Some(Place::First)),
    ("Move to back", Some(Place::Last)),
];

/// The most rows Set on this Mac lists
const SET_ROWS: usize = 10;

/// From this width the selected row's details show on the right.
const SIDE_WIDTH: u16 = 100;

/// Width of the labels on the right
const LABEL: usize = 11;

/// What was read, together
struct Found {
    power: Result<Power, String>,
    mac: Vec<Result<Current, String>>,
    path: neet_core::path::Listing,
}

enum State {
    Looking(Instant, Receiver<Found>),
    Ready(Found),
}

/// One row of a tab
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Heading(&'static str),
    /// Sleep is turned off for good with `pmset disablesleep 1`
    SleepDisabled,
    /// An index into [`power::SETTINGS`]
    Power(usize),
    /// An index into [`mac::SETTINGS`]
    Mac(usize),
    /// An index into [`PREFS`]
    Pref(usize),
    /// An index into the PATH listing's folders
    Path(usize),
}

/// The preferences, in the order shown
const PREFS: [(&str, &str); 4] = [
    ("Disk sort", "How Disk sorts folders when it opens."),
    (
        "Large Files size",
        "The smallest file Large Files lists when it opens.",
    ),
    (
        "Large Files age",
        "How long files must be unchanged for Large Files to list them when it opens.",
    ),
    ("AI Tools tool", "Which tool AI Tools shows when it opens."),
];

/// A change waiting for `y`
enum Ask {
    /// Pick a value for a row, with the choice highlighted
    Choose { row: Item, choice: usize },
    /// Put a row back as it was before neet changed it
    Undo(Item),
    /// What to do with a PATH folder, with the choice highlighted
    PathMenu { index: usize, choice: usize },
    /// Changes to shell files, with their diff
    PathReview {
        title: String,
        prepared: Vec<Prepared>,
        diff: Diff,
    },
}

pub struct Settings {
    home: PathBuf,
    state: State,
    tab: usize,
    table: TableState,
    prefs: Prefs,
    prefs_warnings: Vec<String>,
    power_saved: power::Saved,
    mac_saved: mac::Saved,
    ask: Option<Ask>,
    /// News after a change, until a key is pressed
    note: Option<(String, Color)>,
    /// The power key whose undo is running, to forget once it worked
    pending_undo: Option<&'static str>,
    /// A program name typed to see which copy runs, and whether typing
    which: String,
    typing: bool,
    /// Text typed to find a Mac setting
    find: String,
    /// A shell file was changed, so PATH is new in the next terminal
    path_changed: bool,
}

impl Settings {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let (prefs, prefs_warnings) = Prefs::load(&Prefs::path(&home));
        let mut screen = Self {
            power_saved: power::Saved::load(&home).unwrap_or_default(),
            mac_saved: mac::Saved::load(&home).unwrap_or_default(),
            home,
            state: State::Ready(Found {
                power: Err(String::new()),
                mac: Vec::new(),
                path: neet_core::path::Listing::default(),
            }),
            tab: 0,
            table: TableState::default(),
            prefs,
            prefs_warnings,
            ask: None,
            note: None,
            pending_undo: None,
            which: String::new(),
            typing: false,
            find: String::new(),
            path_changed: false,
        };
        screen.look();
        screen
    }

    #[cfg(test)]
    fn with(home: PathBuf, power: Power, mac: Vec<Current>) -> Self {
        let mut screen = Self {
            power_saved: power::Saved::load(&home).unwrap_or_default(),
            mac_saved: mac::Saved::load(&home).unwrap_or_default(),
            prefs: Prefs::default(),
            prefs_warnings: Vec::new(),
            home,
            state: State::Ready(Found {
                power: Ok(power),
                mac: mac.into_iter().map(Ok).collect(),
                path: neet_core::path::Listing::default(),
            }),
            tab: 0,
            table: TableState::default(),
            ask: None,
            note: None,
            pending_undo: None,
            which: String::new(),
            typing: false,
            find: String::new(),
            path_changed: false,
        };
        screen.select_first();
        screen
    }

    /// Reads power and Mac settings on their own thread, since `pmset` and
    /// `defaults` take a moment each.
    fn look(&mut self) {
        let (sender, receiver) = mpsc::channel();
        let home = self.home.clone();
        thread::spawn(move || {
            let found = Found {
                path: neet_core::path::from_env(&home),
                power: power::read().map_err(|error| error.to_string()),
                mac: mac::read_all()
                    .into_iter()
                    .map(|result| result.map_err(|error| error.to_string()))
                    .collect(),
            };
            let _ = sender.send(found);
        });
        self.state = State::Looking(Instant::now(), receiver);
    }

    fn poll(&mut self) {
        let State::Looking(_, receiver) = &self.state else {
            return;
        };
        let found = match receiver.try_recv() {
            Ok(found) => found,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Found {
                power: Err("the task ended without a result".into()),
                mac: Vec::new(),
                path: neet_core::path::Listing::default(),
            },
        };
        let selected = self.table.selected();
        self.state = State::Ready(found);
        self.table.select(selected);
        if self
            .selected()
            .is_none_or(|row| matches!(row, Item::Heading(_)))
        {
            self.select_first();
        }
    }

    fn found(&self) -> Option<&Found> {
        match &self.state {
            State::Ready(found) => Some(found),
            State::Looking(..) => None,
        }
    }

    fn power(&self) -> Option<&Power> {
        self.found().and_then(|found| found.power.as_ref().ok())
    }

    fn mac_current(&self, index: usize) -> Option<&Current> {
        self.found()
            .and_then(|found| found.mac.get(index))
            .and_then(|result| result.as_ref().ok())
    }

    fn rows(&self) -> Vec<Item> {
        match self.tab {
            0 => {
                let Some(power) = self.power() else {
                    return Vec::new();
                };
                let mut rows = Vec::new();
                if power.sleep_disabled {
                    rows.push(Item::SleepDisabled);
                }
                rows.extend(
                    power::SETTINGS
                        .iter()
                        .enumerate()
                        .filter(|(_, setting)| {
                            power.supported.is_empty()
                                || power.supported.iter().any(|key| key == setting.key)
                        })
                        .filter(|(_, setting)| power.value(setting.key).is_some())
                        .map(|(index, _)| Item::Power(index)),
                );
                rows
            }
            1 => {
                let mut rows = Vec::new();
                let mut group = None;
                let find = self.find.to_lowercase();
                for (index, setting) in mac::SETTINGS.iter().enumerate() {
                    if !find.is_empty()
                        && !setting.name.to_lowercase().contains(&find)
                        && !setting.group.name().to_lowercase().contains(&find)
                    {
                        continue;
                    }
                    if group != Some(setting.group) {
                        group = Some(setting.group);
                        rows.push(Item::Heading(setting.group.name()));
                    }
                    rows.push(Item::Mac(index));
                }
                rows
            }
            2 => (0..PREFS.len()).map(Item::Pref).collect(),
            _ => (0..self.found().map_or(0, |found| found.path.entries.len()))
                .map(Item::Path)
                .collect(),
        }
    }

    fn selected(&self) -> Option<Item> {
        self.table
            .selected()
            .and_then(|index| self.rows().get(index).copied())
    }

    fn select_first(&mut self) {
        let first = self
            .rows()
            .iter()
            .position(|row| !matches!(row, Item::Heading(_)));
        self.table.select(first);
    }

    fn step(&mut self, down: bool) {
        let rows = self.rows();
        let Some(mut index) = self.table.selected() else {
            return;
        };
        loop {
            index = if down {
                index + 1
            } else {
                match index.checked_sub(1) {
                    Some(index) => index,
                    None => return,
                }
            };
            match rows.get(index) {
                None => return,
                Some(Item::Heading(_)) => {}
                Some(_) => break,
            }
        }
        self.table.select(Some(index));
    }

    fn change_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.find.clear();
        self.select_first();
    }

    /// Each choice for a row: its label and the value it writes. Mac
    /// settings add macOS's default at the end, which deletes the key.
    fn choices(row: Item) -> Vec<(String, Option<String>)> {
        match row {
            Item::Power(index) => power::SETTINGS[index]
                .choices
                .iter()
                .map(|(label, value)| ((*label).to_string(), Some((*value).to_string())))
                .collect(),
            Item::Mac(index) => {
                let setting = &mac::SETTINGS[index];
                let mut choices: Vec<_> = setting
                    .choices
                    .iter()
                    .map(|(label, value)| ((*label).to_string(), Some((*value).to_string())))
                    .collect();
                choices.push(("macOS default".into(), None));
                choices
            }
            Item::SleepDisabled => vec![("Let the Mac sleep".into(), Some("0".into()))],
            Item::Pref(index) => pref_choices(index)
                .into_iter()
                .map(|(label, value)| (label, Some(value)))
                .collect(),
            Item::Heading(_) | Item::Path(_) => Vec::new(),
        }
    }

    /// The choice a row has now, to start the picker on
    fn current_choice(&self, row: Item) -> usize {
        let choices = Self::choices(row);
        let now = match row {
            Item::Power(index) => self
                .power()
                .and_then(|power| power.value(power::SETTINGS[index].key))
                .map(str::to_string),
            Item::Mac(index) => self.mac_current(index).and_then(|current| {
                let setting = &mac::SETTINGS[index];
                match current {
                    Current::Unset => None,
                    Current::Set(value) => setting
                        .choice(value)
                        .map(|choice| setting.choices[choice].1.to_string()),
                }
            }),
            Item::Pref(index) => Some(pref_value(&self.prefs, index)),
            Item::SleepDisabled | Item::Heading(_) | Item::Path(_) => None,
        };
        choices
            .iter()
            .position(|(_, value)| *value == now)
            .unwrap_or(0)
    }

    fn open_choose(&mut self) {
        let Some(row) = self.selected() else {
            return;
        };
        if matches!(row, Item::Heading(_)) {
            return;
        }
        if let Item::Path(index) = row {
            self.ask = Some(Ask::PathMenu { index, choice: 0 });
            return;
        }
        if let Item::Mac(index) = row
            && self.mac_current(index).is_none()
        {
            self.note = Some((
                "This setting could not be read, so it cannot be changed.".into(),
                Color::Yellow,
            ));
            return;
        }
        let choice = self.current_choice(row);
        self.ask = Some(Ask::Choose { row, choice });
    }

    fn open_undo(&mut self) {
        let Some(row) = self.selected() else {
            return;
        };
        let saved = match row {
            Item::Power(index) => self
                .power_saved
                .before
                .contains_key(power::SETTINGS[index].key),
            Item::SleepDisabled => self.power_saved.before.contains_key(power::DISABLE_SLEEP),
            Item::Mac(index) => self.mac_saved.before(&mac::SETTINGS[index]).is_some(),
            Item::Pref(_) | Item::Heading(_) | Item::Path(_) => false,
        };
        if saved {
            self.ask = Some(Ask::Undo(row));
        } else {
            self.note = Some((
                "Nothing to undo. Only changes made here are kept.".into(),
                Color::Yellow,
            ));
        }
    }

    /// The commands a power change runs, and the change itself
    fn power_change(&self, row: Item, choice: usize) -> Option<power::Change> {
        match row {
            Item::SleepDisabled => Some(power::Change::allow_sleep()),
            Item::Power(index) => {
                let (_, value) = Self::choices(row).into_iter().nth(choice)?;
                power::Change::new(self.power()?, &power::SETTINGS[index], &value?).ok()
            }
            _ => None,
        }
    }

    fn mac_change(&self, index: usize, choice: usize) -> Option<mac::Change> {
        let (_, value) = Self::choices(Item::Mac(index)).into_iter().nth(choice)?;
        mac::Change::new(index, self.mac_current(index)?.clone(), value.as_deref()).ok()
    }

    /// What `y` will do, for the question
    fn question(&self, ask: &Ask) -> (String, Vec<Line<'static>>, Color) {
        match ask {
            Ask::PathMenu { index, choice } => {
                let mut lines: Vec<Line> = PATH_ACTS
                    .iter()
                    .enumerate()
                    .map(|(number, (label, _))| {
                        if number == *choice {
                            Line::from(format!("▸ {label}")).bold()
                        } else {
                            Line::from(format!("  {label}"))
                        }
                    })
                    .collect();
                lines.push(Line::default());
                lines.push(Line::from(
                    "The shell file's diff shows first. Nothing is written before y.",
                ));
                lines.push(Line::from("↑↓ choose · enter review · Esc cancel").bold());
                (self.name(Item::Path(*index)), lines, visual::ACCENT)
            }
            // Drawn as a card with its diff, not as a question
            Ask::PathReview { title, .. } => (title.clone(), Vec::new(), visual::ACCENT),
            Ask::Choose { row, choice } => {
                let (row, choice) = (*row, *choice);
                let mut lines = Vec::new();
                for (index, (label, _)) in Self::choices(row).iter().enumerate() {
                    let mark = if index == choice { "▸ " } else { "  " };
                    let line = Line::from(format!("{mark}{label}"));
                    lines.push(if index == choice { line.bold() } else { line });
                }
                lines.push(Line::default());
                let (title, color) = match row {
                    Item::Power(_) | Item::SleepDisabled => {
                        if let Some(change) = self.power_change(row, choice) {
                            for program in change.programs(&self.home) {
                                lines.push(command_line(&program.shown()));
                            }
                        }
                        lines.push(Line::from(
                            "sudo asks for your password in the terminal, then neet comes back. u undoes it later.",
                        ));
                        (row_name(row), Color::Yellow)
                    }
                    Item::Mac(index) => {
                        if let Some(change) = self.mac_change(index, choice) {
                            lines.push(command_line(&change.command()));
                        }
                        lines.push(Line::from(mac::SETTINGS[index].restart.note()));
                        lines.push(Line::from("u undoes it later."));
                        (row_name(row), visual::ACCENT)
                    }
                    _ => {
                        lines.push(Line::from(format!(
                            "Saved in {}.",
                            self.shown(&Prefs::path(&self.home))
                        )));
                        (row_name(row), visual::ACCENT)
                    }
                };
                lines.push(Line::from("↑↓ choose · y apply · Esc cancel").bold());
                (title.to_string(), lines, color)
            }
            Ask::Undo(row) => {
                let row = *row;
                let mut lines = vec![
                    Line::from("Put this back as it was before the change:"),
                    Line::default(),
                ];
                match row {
                    Item::Power(_) | Item::SleepDisabled => {
                        for program in self.power_undo(row).unwrap_or_default() {
                            lines.push(command_line(&program.shown()));
                        }
                    }
                    Item::Mac(index) => {
                        if let Some(change) = self
                            .mac_current(index)
                            .and_then(|now| self.mac_saved.undo(index, now.clone()))
                        {
                            lines.push(command_line(&change.command()));
                        }
                    }
                    _ => {}
                }
                lines.push(Line::default());
                lines.push(Line::from("y undo · Esc cancel").bold());
                (format!("Undo {}", row_name(row)), lines, Color::Yellow)
            }
        }
    }

    fn power_undo(&self, row: Item) -> Option<Vec<Program>> {
        let key = match row {
            Item::Power(index) => power::SETTINGS[index].key,
            Item::SleepDisabled => power::DISABLE_SLEEP,
            _ => return None,
        };
        self.power_saved.undo(&self.home, key)
    }

    /// Does what the question asked
    fn confirm(&mut self) -> Action {
        let Some(ask) = self.ask.take() else {
            return Action::None;
        };
        match ask {
            Ask::Choose { row, choice } => match row {
                Item::Power(_) | Item::SleepDisabled => {
                    let Some(change) = self.power_change(row, choice) else {
                        return Action::None;
                    };
                    if let Err(error) = self.power_saved.remember(&self.home, &change) {
                        self.note = Some((
                            format!(
                                "The old value could not be saved, so nothing changed: {error}"
                            ),
                            Color::Red,
                        ));
                        return Action::None;
                    }
                    return Action::Run(change.programs(&self.home));
                }
                Item::Mac(index) => {
                    let Some(change) = self.mac_change(index, choice) else {
                        return Action::None;
                    };
                    let result = change.apply(&self.home, &mut self.mac_saved);
                    self.after_mac(
                        &change,
                        result.map(|()| "Changed").map_err(|e| e.to_string()),
                    );
                }
                Item::Pref(index) => {
                    let (_, value) = Self::choices(row).swap_remove(choice);
                    set_pref(&mut self.prefs, index, &value.unwrap_or_default());
                    self.note = Some(match self.prefs.save(&Prefs::path(&self.home)) {
                        Ok(()) => (
                            "Saved. Screens use it the next time they open.".into(),
                            Color::Green,
                        ),
                        Err(error) => (format!("Not saved: {error}"), Color::Red),
                    });
                }
                Item::Heading(_) | Item::Path(_) => {}
            },
            Ask::PathMenu { index, choice } => {
                let change = match PATH_ACTS[choice].1 {
                    None => edit::Change::Remove(index),
                    Some(place) => edit::Change::Move(index, place),
                };
                self.review_path(PATH_ACTS[choice].0, &[change]);
            }
            Ask::PathReview { prepared, .. } => {
                let backups = Backups::dotfiles(&self.home);
                let written = edit::write(&prepared, &self.home, &backups, SystemTime::now());
                self.note = Some(match written {
                    Ok(done) => {
                        self.path_changed = true;
                        (path_done(&done), Color::Green)
                    }
                    Err(error) => (format!("Nothing changed: {error}"), Color::Red),
                });
            }
            Ask::Undo(row) => match row {
                Item::Power(_) | Item::SleepDisabled => {
                    if let Some(programs) = self.power_undo(row) {
                        let key = match row {
                            Item::Power(index) => power::SETTINGS[index].key,
                            _ => power::DISABLE_SLEEP,
                        };
                        // Forgotten after it runs, in `ran`, if it worked
                        self.pending_undo = Some(key);
                        return Action::Run(programs);
                    }
                }
                Item::Mac(index) => {
                    let Some(change) = self
                        .mac_current(index)
                        .and_then(|now| self.mac_saved.undo(index, now.clone()))
                    else {
                        return Action::None;
                    };
                    let result = change.apply(&self.home, &mut self.mac_saved);
                    if result.is_ok() {
                        let _ = self.mac_saved.forget(&self.home, change.setting());
                    }
                    self.after_mac(
                        &change,
                        result.map(|()| "Put back").map_err(|e| e.to_string()),
                    );
                }
                _ => {}
            },
        }
        Action::None
    }

    fn after_mac(&mut self, change: &mac::Change, result: Result<&str, String>) {
        self.note = Some(match result {
            Ok(done) => (
                format!(
                    "{done}: {}. {}",
                    change.setting().name,
                    change.setting().restart.note()
                ),
                Color::Green,
            ),
            Err(error) => (format!("Nothing changed: {error}"), Color::Red),
        });
        self.look();
    }

    /// Plans `changes` and shows the shell files' diff, or says why not
    fn review_path(&mut self, what: &str, changes: &[edit::Change]) {
        let Some(found) = self.found() else {
            return;
        };
        match edit::prepare(&found.path, changes, &self.home) {
            Ok(prepared) if prepared.is_empty() => {
                self.note = Some(("Nothing to change.".into(), Color::Yellow));
            }
            Ok(prepared) => {
                let diff = path_diff(&prepared);
                self.ask = Some(Ask::PathReview {
                    title: format!(" {what} "),
                    prepared,
                    diff,
                });
            }
            Err(reason) => self.note = Some((reason, Color::Yellow)),
        }
    }

    /// `c`: every repeated or missing folder that a shell file line adds,
    /// taken out in one review
    fn clean_path(&mut self) {
        let Some(found) = self.found() else {
            return;
        };
        let broken: Vec<usize> = found
            .path
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.problems.iter().any(|problem| {
                    matches!(
                        problem,
                        neet_core::path::Problem::Repeated(_)
                            | neet_core::path::Problem::Missing
                            | neet_core::path::Problem::NotFolder
                    )
                })
            })
            .map(|(index, _)| index)
            .collect();
        if broken.is_empty() {
            self.note = Some((
                "Nothing to clean up. No folder is repeated or missing.".into(),
                Color::Green,
            ));
            return;
        }
        // Folders that cannot be changed are left, each for its own reason.
        let mut changes = Vec::new();
        for &index in &broken {
            changes.push(edit::Change::Remove(index));
            if edit::prepare(&found.path, &changes, &self.home).is_err() {
                changes.pop();
            }
        }
        if changes.is_empty() {
            self.note = Some((
                format!(
                    "{} repeated or missing, none set by a plain PATH line in a shell file. Enter on one says why.",
                    broken.len()
                ),
                Color::Yellow,
            ));
            return;
        }
        let what = format!("Clean up {} of {}", changes.len(), broken.len());
        self.review_path(&what, &changes);
    }

    /// Mac settings with a value set, the ones changed here marked
    fn set_here(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("Set on this Mac").style(visual::HEADING)];
        let set: Vec<usize> = (0..mac::SETTINGS.len())
            .filter(|&index| matches!(self.mac_current(index), Some(Current::Set(_))))
            .collect();
        if set.is_empty() {
            lines.push(Line::from("Every setting is at its default."));
            return lines;
        }
        let name_width = width.saturating_sub(2) * 3 / 5;
        for &index in set.iter().take(SET_ROWS) {
            let setting = &mac::SETTINGS[index];
            let mark = if self.changed_by_neet(Item::Mac(index)) {
                Span::raw("• ").fg(visual::ACCENT)
            } else {
                Span::raw("  ")
            };
            let name = format::shorten_middle(setting.name, name_width.saturating_sub(1));
            let mut spans = vec![mark, Span::raw(format!("{name:<name_width$}")).bold()];
            spans.extend(self.value(Item::Mac(index)));
            lines.push(Line::from(spans));
        }
        if set.len() > SET_ROWS {
            lines.push(Line::from(format!("  {} more", set.len() - SET_ROWS)));
        }
        lines
    }

    fn path_entry(&self, index: usize) -> Option<&neet_core::path::Entry> {
        self.found().and_then(|found| found.path.entries.get(index))
    }

    /// A row's name. PATH folders show their path, with ~ for home.
    fn name(&self, row: Item) -> String {
        match row {
            Item::Path(index) => self
                .path_entry(index)
                .map(|entry| self.shown(std::path::Path::new(&entry.folder)))
                .unwrap_or_default(),
            _ => row_name(row).to_string(),
        }
    }

    /// Where a PATH folder was added, in a few words
    fn origin(&self, origin: &neet_core::path::Origin) -> String {
        use neet_core::path::Origin;
        match origin {
            Origin::Line { file, line } => format!("{}, line {line}", self.shown(file)),
            Origin::Tool { tool, file, line } => {
                format!("{tool}, run from {}, line {line}", self.shown(file))
            }
            Origin::System(file) => format!("macOS, from {}", file.display()),
            Origin::Unknown => {
                "not in your shell files. Another program added it, such as an editor's terminal."
                    .into()
            }
        }
    }

    /// Programs in more than one folder, and which copy runs
    fn shadowed(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("In more than one folder").style(visual::HEADING)];
        let Some(found) = self.found() else {
            return lines;
        };
        if found.path.shadowed.is_empty() {
            lines.push(Line::from("None. Every program name is in one folder."));
            return lines;
        }
        let count = found.path.shadowed.len();
        lines.push(Line::from(format!(
            "{count} {}. The first folder's copy runs.",
            if count == 1 { "program" } else { "programs" }
        )));
        for shadowed in found.path.shadowed.iter().take(8) {
            let first = &found.path.entries[shadowed.entries[0]].folder;
            let folder = self.shown(std::path::Path::new(first));
            let name = format!("{:<14}", format::shorten_middle(&shadowed.name, 14));
            lines.push(Line::from(vec![
                Span::raw(name).bold(),
                Span::raw(format::shorten_path(&folder, width.saturating_sub(15))),
            ]));
        }
        lines
    }

    /// Every copy of the typed program, in PATH order
    fn which_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("Which copy runs").style(visual::HEADING)];
        let cursor = if self.typing { "▏" } else { "" };
        lines.push(Line::from(vec![
            Span::raw("Program  ").bold(),
            Span::raw(format!("{}{cursor}", self.which)).fg(visual::ACCENT),
        ]));
        if self.which.is_empty() {
            lines.push(Line::from(
                "Press / and type a program name, such as python3.",
            ));
            return lines;
        }
        let copies = self
            .found()
            .map(|found| found.path.which(&self.which))
            .unwrap_or_default();
        if copies.is_empty() {
            lines.push(Line::from("Not found in any PATH folder.").yellow());
        }
        for (number, copy) in copies.iter().enumerate() {
            let shown = format::shorten_path(&self.shown(copy), width.saturating_sub(10));
            lines.push(if number == 0 {
                Line::from(vec![
                    Span::raw("runs     ").green().bold(),
                    Span::raw(shown),
                ])
            } else {
                Line::from(vec![Span::raw("hidden   ").yellow(), Span::raw(shown)])
            });
        }
        lines
    }

    fn shown(&self, path: &std::path::Path) -> String {
        path.strip_prefix(&self.home).map_or_else(
            |_| path.display().to_string(),
            |rest| format!("~/{}", rest.display()),
        )
    }

    /// A row's value now, colored by whether it is the default
    fn value(&self, row: Item) -> Vec<Span<'static>> {
        match row {
            Item::Heading(_) => Vec::new(),
            Item::Path(index) => {
                let Some(entry) = self.path_entry(index) else {
                    return Vec::new();
                };
                if entry.problems.is_empty() {
                    let programs = entry.programs;
                    vec![Span::raw(format!(
                        "{programs} {}",
                        if programs == 1 { "program" } else { "programs" }
                    ))]
                } else {
                    let mut spans = Vec::new();
                    for (number, problem) in entry.problems.iter().enumerate() {
                        if number > 0 {
                            spans.push(Span::raw(" · "));
                        }
                        let label = Span::raw(problem.label());
                        spans.push(match problem {
                            neet_core::path::Problem::Repeated(_)
                            | neet_core::path::Problem::Missing => label.yellow(),
                            _ => label.red().bold(),
                        });
                    }
                    spans
                }
            }
            Item::SleepDisabled => vec![Span::raw("never sleeps").red().bold()],
            Item::Power(index) => {
                let setting = &power::SETTINGS[index];
                let Some(power) = self.power() else {
                    return Vec::new();
                };
                let values = power.values(setting.key);
                let battery = values.get(power::BATTERY);
                let ac = values.get(power::AC);
                match (battery, ac) {
                    (Some(battery), Some(ac)) if battery != ac => vec![
                        Span::raw(format!("{} on battery", setting.label(battery))),
                        Span::raw(" · "),
                        Span::raw(format!("{} on charger", setting.label(ac))),
                    ],
                    _ => vec![Span::raw(
                        power
                            .value(setting.key)
                            .map_or_else(String::new, |v| setting.label(v)),
                    )],
                }
            }
            Item::Mac(index) => match self.found().and_then(|found| found.mac.get(index)) {
                Some(Ok(current)) => {
                    let label = mac::SETTINGS[index].label(current);
                    if *current == Current::Unset {
                        vec![Span::raw(label)]
                    } else {
                        vec![Span::raw(label).fg(visual::ACCENT)]
                    }
                }
                Some(Err(_)) => vec![Span::raw("unreadable").yellow()],
                None => vec![Span::raw("looking…")],
            },
            Item::Pref(index) => {
                let value = pref_value(&self.prefs, index);
                let label = pref_choices(index)
                    .into_iter()
                    .find(|(_, choice)| *choice == value)
                    .map_or(value, |(label, _)| label);
                vec![Span::raw(label)]
            }
        }
    }

    /// Whether neet changed a row, so `u` can put it back
    fn changed_by_neet(&self, row: Item) -> bool {
        match row {
            Item::Power(index) => self
                .power_saved
                .before
                .contains_key(power::SETTINGS[index].key),
            Item::Mac(index) => self.mac_saved.before(&mac::SETTINGS[index]).is_some(),
            Item::SleepDisabled | Item::Pref(_) | Item::Heading(_) | Item::Path(_) => false,
        }
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.rows();
        let names: Vec<String> = rows.iter().map(|&row| self.name(row)).collect();
        // Long PATH folders are shortened, so the value column keeps room.
        let name_width = format::column_width("Setting", names.iter().map(|n| format!("  {n}")))
            .min(area.width.saturating_sub(4) * 3 / 5);
        let columns = Columns::new(area.width, [(name_width, 0), (12, 1), (2, 0)], &[2], true);
        let table_rows: Vec<Row> = rows
            .iter()
            .zip(&names)
            .map(|(&row, name)| match row {
                Item::Heading(title) => columns.row([
                    Cell::from(Span::styled(title, visual::HEADING)),
                    Cell::from(""),
                    Cell::from(""),
                ]),
                Item::SleepDisabled => columns.row([
                    Cell::from(Span::raw(format!("  {name}")).red().bold()),
                    Cell::from(Line::from(self.value(row))),
                    Cell::from(""),
                ]),
                _ => columns.row([
                    Cell::from(format!(
                        "  {}",
                        format::shorten_middle(name, columns.width(0).saturating_sub(2))
                    )),
                    Cell::from(Line::from(self.value(row))),
                    Cell::from(if self.changed_by_neet(row) {
                        Span::raw("•").fg(visual::ACCENT)
                    } else {
                        Span::raw("")
                    }),
                ]),
            })
            .collect();
        let header = columns
            .row([
                Cell::from(if self.tab == PATH_TAB {
                    "Folder"
                } else {
                    "Setting"
                }),
                Cell::from(if self.tab == PATH_TAB {
                    "Programs or problem"
                } else {
                    "Now"
                }),
                Cell::from(""),
            ])
            .style(visual::HEADING);
        let finding = self.tab == MAC_TAB && (self.typing || !self.find.is_empty());
        let block = visual::block()
            .title(if finding {
                let cursor = if self.typing { "▏" } else { "" };
                format!(" {} · find: {}{cursor} ", TABS[self.tab], self.find)
            } else {
                format!(" {} ", TABS[self.tab])
            })
            .padding(Padding::horizontal(1));
        if rows.is_empty() {
            let reason = match self.found().map(|found| &found.power) {
                _ if finding => "No setting matches. Esc clears the search.".into(),
                Some(Err(error)) if self.tab == 0 => {
                    format!("Power settings could not be read: {error}")
                }
                _ => "Nothing to show.".into(),
            };
            frame.render_widget(
                Paragraph::new(reason)
                    .wrap(Wrap { trim: true })
                    .block(block),
                area,
            );
            return;
        }
        let table = Table::new(table_rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// A PATH folder's order, programs, where it was added, and problems
    fn path_details(&self, index: usize, lines: &mut Vec<Line<'static>>, width: usize) {
        let Some(entry) = self.path_entry(index) else {
            return;
        };
        let text = |lines: &mut Vec<Line<'static>>, text: &str| {
            lines.extend(visual::hanging(
                Vec::new(),
                0,
                text,
                ratatui::style::Style::default(),
                width,
            ));
        };
        let count = self.found().map_or(0, |found| found.path.entries.len());
        lines.push(field(
            "Order",
            Span::raw(format!("#{} of {count}, searched in this order", index + 1)),
        ));
        lines.push(field("Programs", Span::raw(entry.programs.to_string())));
        lines.push(field("Added by", Span::raw(self.origin(&entry.origin))));
        for problem in &entry.problems {
            lines.push(Line::default());
            lines.push(Line::from(problem.label()).red().bold());
            text(lines, problem.about());
        }
        if matches!(entry.origin, neet_core::path::Origin::System(_))
            && entry.problems.contains(&neet_core::path::Problem::Missing)
        {
            text(
                lines,
                "macOS lists this folder for every user. It exists only on some Macs, so a missing one here is harmless.",
            );
        }
    }

    /// The selected row: what it does, where it is kept, and the keys
    fn details(&self, row: Item, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(row_name(row)).style(visual::HEADING)];
        let text = |lines: &mut Vec<Line<'static>>, text: &str| {
            lines.extend(visual::hanging(
                Vec::new(),
                0,
                text,
                ratatui::style::Style::default(),
                width,
            ));
        };
        match row {
            Item::Path(index) => {
                lines[0] = Line::from(self.name(row)).style(visual::HEADING);
                self.path_details(index, &mut lines, width);
            }
            Item::SleepDisabled => {
                text(
                    &mut lines,
                    "Something ran sudo pmset disablesleep 1, so this Mac never sleeps on its own, not even with the lid closed. That drains the battery and keeps it warm.",
                );
                lines.push(Line::default());
                text(
                    &mut lines,
                    "Enter lets it sleep again with sudo pmset -a disablesleep 0.",
                );
            }
            Item::Power(index) => {
                let setting = &power::SETTINGS[index];
                text(&mut lines, setting.about);
                lines.push(Line::default());
                lines.push(field("Key", Span::raw(format!("pmset {}", setting.key))));
                if let Some(power) = self.power() {
                    for (source, value) in power.values(setting.key) {
                        let source = match source.as_str() {
                            power::BATTERY => "Battery",
                            power::AC => "Charger",
                            other => other,
                        };
                        lines.push(field(source, Span::raw(setting.label(&value))));
                    }
                }
            }
            Item::Mac(index) => {
                let setting = &mac::SETTINGS[index];
                text(&mut lines, setting.about);
                lines.push(Line::default());
                lines.push(field(
                    "Key",
                    Span::raw(format!("{} {}", setting.domain, setting.key)),
                ));
                if let Some(default) = setting.default {
                    let label = setting
                        .choice(default)
                        .map_or(default, |choice| setting.choices[choice].0);
                    lines.push(field("Default", Span::raw(label.to_string())));
                }
                if let Some(before) = self.mac_saved.before(setting) {
                    lines.push(field(
                        "Before neet",
                        Span::raw(setting.label(before)).fg(visual::ACCENT),
                    ));
                }
                lines.push(Line::default());
                text(&mut lines, setting.restart.note());
            }
            Item::Pref(index) => {
                text(&mut lines, PREFS[index].1);
                lines.push(Line::default());
                lines.push(field(
                    "Saved in",
                    Span::raw(self.shown(&Prefs::path(&self.home))).fg(visual::ACCENT),
                ));
            }
            Item::Heading(_) => {}
        }
        if self.changed_by_neet(row) {
            lines.push(Line::default());
            lines.push(Line::from(vec![
                Span::raw("• ").fg(visual::ACCENT),
                Span::raw("Changed here. u puts it back."),
            ]));
        }
        lines
    }

    /// How changes on this tab are made and undone
    fn how(&self, width: usize) -> Vec<Line<'static>> {
        let heading = if self.tab == PATH_TAB {
            "About this list"
        } else {
            "How changes work"
        };
        let mut lines = vec![Line::from(heading).style(visual::HEADING)];
        let steps: &[&str] = match self.tab {
            0 => &[
                "Enter picks a value and shows the sudo pmset command.",
                "y runs it once. sudo asks for your password in the terminal.",
                "• marks a change made here. u puts it back for both battery and charger.",
            ],
            1 => &[
                "Enter picks a value and shows the defaults command.",
                "y writes it and restarts Finder, the Dock, or the menu bar when needed.",
                "• marks a change made here. u puts it back; macOS default deletes the key.",
            ],
            2 => &[
                "Enter picks a value. y saves it right away.",
                "Screens use it the next time they open. Reviews and checks stay the same.",
            ],
            _ => &[
                "The folders your shell searches for programs, first to last, read from this terminal.",
                "Enter removes a folder or moves it. c takes out every repeated or missing one.",
                "The shell file's diff shows first. y writes it, with a backup in Dotfiles.",
            ],
        };
        for (number, step) in steps.iter().enumerate() {
            lines.extend(visual::hanging(
                vec![
                    Span::raw(format!("{}. ", number + 1))
                        .fg(visual::ACCENT)
                        .bold(),
                ],
                3,
                step,
                ratatui::style::Style::default(),
                width,
            ));
        }
        if self.tab < 2 {
            lines.push(Line::default());
            lines.push(field(
                "Old values",
                Span::raw(self.shown(&self.home.join(".local/state/neet/settings"))),
            ));
        }
        if self.tab == PATH_TAB && self.path_changed {
            lines.push(Line::default());
            lines.push(
                Line::from("A shell file changed. Open a new terminal to see the new PATH.")
                    .yellow(),
            );
        }
        lines
    }

    /// The battery and power source
    fn battery(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("Battery").style(visual::HEADING)];
        let Some(power) = self.power() else {
            return lines;
        };
        let source = match power.source.as_str() {
            power::AC => "charger",
            power::BATTERY => "battery",
            other => other,
        };
        lines.push(field("Running on", Span::raw(source.to_string())));
        match &power.battery {
            Some(battery) => {
                let level = Span::raw(format!("{}%", battery.percent));
                let level = match battery.percent {
                    0..=20 => level.red().bold(),
                    21..=40 => level.yellow(),
                    _ => level.green(),
                };
                lines.push(field("Level", level));
                lines.push(field("State", Span::raw(battery.state.clone())));
                if let Some(remaining) = &battery.remaining {
                    lines.push(field("Left", Span::raw(remaining.clone())));
                }
            }
            None => lines.push(Line::from("No battery.")),
        }
        lines
    }

    /// What keeps the Mac awake now
    fn awake(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("Keeping it awake").style(visual::HEADING)];
        let Some(power) = self.power() else {
            return lines;
        };
        let holders: Vec<_> = power
            .awake
            .iter()
            .filter(|awake| awake.kind != "UserIsActive")
            .collect();
        if holders.is_empty() {
            lines.push(Line::from("Nothing. The Mac can sleep when idle."));
        }
        for awake in holders {
            lines.push(Line::from(vec![
                Span::raw(format!(
                    "{:<14}",
                    format::shorten_middle(&awake.process, 14)
                ))
                .bold(),
                Span::raw(awake.stops()).yellow(),
            ]));
            if !awake.reason.is_empty() {
                lines.push(Line::from(format!("  {}", awake.reason)));
            }
        }
        lines
    }

    /// The boxes beside the details: the same for every row of a tab
    fn extras(&self, width: usize) -> Vec<Vec<Line<'static>>> {
        match self.tab {
            0 => vec![self.awake(), self.battery()],
            MAC_TAB => vec![self.set_here(width)],
            PATH_TAB => vec![self.which_lines(width), self.shadowed(width)],
            _ if !self.prefs_warnings.is_empty() => {
                let mut lines = vec![Line::from("Settings file").style(visual::HEADING)];
                lines.extend(
                    self.prefs_warnings
                        .iter()
                        .map(|warning| Line::from(warning.clone()).yellow()),
                );
                vec![lines]
            }
            _ => Vec::new(),
        }
    }

    /// The selected row's box, as tall as the tab's longest details, so the
    /// boxes under it stay where they are while the selection moves
    fn draw_side(&self, frame: &mut Frame, area: Rect) {
        let width = usize::from(area.width.saturating_sub(4));
        let inner = area.width.saturating_sub(4);
        let extras = self.extras(width);
        let Some(row) = self.selected() else {
            visual::sections(frame, area, "", extras);
            return;
        };
        let mut lines = self.details(row, width);
        let title = lines.remove(0).to_string();
        let tallest = self
            .rows()
            .into_iter()
            .filter(|row| !matches!(row, Item::Heading(_)))
            .map(|row| visual::wrapped_rows(&self.details(row, width), inner))
            .max()
            .unwrap_or(1)
            // The heading is the box's title, and the border takes two rows.
            .saturating_add(1);
        let needed: u16 = extras
            .iter()
            .map(|lines| visual::wrapped_rows(lines, inner) + 1)
            .sum();
        let block = visual::block()
            .title(format!(" {title} "))
            .padding(Padding::horizontal(1));
        if extras.is_empty() || area.height < tallest.saturating_add(needed) {
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(block),
                area,
            );
            return;
        }
        let [top, below] =
            Layout::vertical([Constraint::Length(tallest), Constraint::Fill(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            top,
        );
        visual::sections(frame, below, "", extras);
    }

    fn draw_body(&mut self, frame: &mut Frame, area: Rect) {
        let width = |area: Rect| usize::from(area.width.saturating_sub(4));
        if area.width >= SIDE_WIDTH {
            let [left, right] =
                Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)]).areas(area);
            // The table only as tall as its rows, with how changes work below
            let how = self.how(width(left));
            let table = u16::try_from(self.rows().len())
                .unwrap_or(u16::MAX)
                .max(1)
                .saturating_add(3);
            let below = visual::wrapped_rows(&how, left.width.saturating_sub(4)) + 2;
            if left.height >= table.saturating_add(below) {
                let [list, rest] =
                    Layout::vertical([Constraint::Length(table), Constraint::Fill(1)]).areas(left);
                self.draw_table(frame, list);
                visual::sections(frame, rest, "", vec![how]);
            } else {
                self.draw_table(frame, left);
            }
            self.draw_side(frame, right);
        } else {
            let rows = u16::try_from(self.rows().len()).unwrap_or(u16::MAX);
            let table = (rows + 3).min(area.height.saturating_sub(6).max(area.height / 2));
            let [list, below] =
                Layout::vertical([Constraint::Length(table), Constraint::Fill(1)]).areas(area);
            self.draw_table(frame, list);
            if below.height > 2 {
                let mut sections = Vec::new();
                if let Some(row) = self.selected() {
                    sections.push(self.details(row, width(below)));
                }
                sections.extend(self.extras(width(below)));
                visual::sections(frame, below, "", sections);
            }
        }
    }
}

/// Every planned file's diff, one after the other, each under its name
fn path_diff(prepared: &[Prepared]) -> Diff {
    let mut all = Diff::new(b"", b"");
    for Prepared { planned, .. } in prepared {
        let diff = Diff::new(planned.before.as_bytes(), planned.after.as_bytes());
        if !all.lines.is_empty() {
            all.lines.push(Line::default());
        }
        all.lines
            .push(Line::from(format!("~/{}", planned.name)).bold());
        all.lines.extend(diff.lines);
        all.added += diff.added;
        all.removed += diff.removed;
    }
    all
}

/// What writing the shell files did, in a few words
fn path_done(done: &[neet_core::dotfiles::change::Done]) -> String {
    use neet_core::dotfiles::change::Done;
    use std::fmt::Write as _;
    let mut text = String::from(
        "Written, with a backup in Dotfiles. Open a new terminal to use the new PATH.",
    );
    for result in done {
        match result {
            Done::ApplyYourself(command) => {
                let _ = write!(text, " chezmoi's copy changed. Run {command} to finish.");
            }
            Done::ApplyFailed { error, command } => {
                let _ = write!(
                    text,
                    " chezmoi's copy changed, but chezmoi failed: {error} Run {command} once fixed."
                );
            }
            _ => {}
        }
    }
    text
}

/// The preferences screens open with. Tests use the defaults, so a
/// settings file on the Mac running them changes nothing.
pub(super) fn saved_prefs() -> Prefs {
    if cfg!(test) {
        Prefs::default()
    } else {
        Prefs::from_env()
    }
}

/// A command, as the review shows it
fn command_line(command: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("Runs  ").bold(),
        Span::raw(command.to_string()).fg(visual::ACCENT),
    ])
}

fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<LABEL$}")).bold(), value])
}

fn row_name(row: Item) -> &'static str {
    match row {
        Item::Heading(title) => title,
        Item::Path(_) => "Shell PATH",
        Item::SleepDisabled => "Sleep is turned off",
        Item::Power(index) => power::SETTINGS[index].name,
        Item::Mac(index) => mac::SETTINGS[index].name,
        Item::Pref(index) => PREFS[index].0,
    }
}

/// Each choice for preference `index`: its label and saved value
fn pref_choices(index: usize) -> Vec<(String, String)> {
    match index {
        0 => prefs::DISK_SORTS
            .iter()
            .map(|sort| (format!("By {sort}"), (*sort).to_string()))
            .collect(),
        1 => prefs::LARGE_SIZES
            .iter()
            .map(|&size| (format!("{} and up", format::size(size)), size.to_string()))
            .collect(),
        2 => prefs::LARGE_AGES
            .iter()
            .map(|&days| {
                let label = match days {
                    0 => "Any age".to_string(),
                    365 => "1 year".to_string(),
                    730 => "2 years".to_string(),
                    days if days % 30 == 0 && days >= 90 => format!("{} months", days / 30),
                    days => format!("{days} days"),
                };
                (label, days.to_string())
            })
            .collect(),
        _ => prefs::AI_TOOLS
            .iter()
            .map(|tool| {
                let label = match *tool {
                    "all" => "All tools",
                    "claude" => "Claude Code",
                    "codex" => "Codex",
                    "gemini" => "Gemini CLI",
                    _ => "Shared",
                };
                (label.to_string(), (*tool).to_string())
            })
            .collect(),
    }
}

fn pref_value(prefs: &Prefs, index: usize) -> String {
    match index {
        0 => prefs.disk_sort.clone(),
        1 => prefs.large_min_size.to_string(),
        2 => prefs.large_unchanged_days.to_string(),
        _ => prefs.ai_tool.clone(),
    }
}

fn set_pref(prefs: &mut Prefs, index: usize, value: &str) {
    match index {
        0 => prefs.disk_sort = value.to_string(),
        1 => prefs.large_min_size = value.parse().unwrap_or(prefs.large_min_size),
        2 => prefs.large_unchanged_days = value.parse().unwrap_or(prefs.large_unchanged_days),
        _ => prefs.ai_tool = value.to_string(),
    }
}

impl Screen for Settings {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        if let State::Looking(started, _) = &self.state {
            Loading {
                title: "Settings",
                doing: "Reading power and Mac settings",
                progress: format!("{}s", started.elapsed().as_secs()),
                note: "Reading changes nothing.",
            }
            .draw(frame, area);
            return;
        }
        let [header, body] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);
        let mut tabs = Vec::new();
        for (index, name) in TABS.iter().enumerate() {
            if index > 0 {
                tabs.push(Span::raw("   "));
            }
            tabs.push(if index == self.tab {
                Span::raw(format!("[{name}]")).bold().fg(visual::ACCENT)
            } else {
                Span::raw((*name).to_string())
            });
        }
        frame.render_widget(
            Paragraph::new(Line::from(tabs)).block(
                visual::block()
                    .title(" Settings ")
                    .padding(Padding::horizontal(1)),
            ),
            header,
        );
        self.draw_body(frame, body);
        if let Some(Ask::PathReview {
            title,
            prepared,
            diff,
        }) = &self.ask
        {
            let files: Vec<String> = prepared
                .iter()
                .map(|file| format!("~/{}", file.planned.name))
                .collect();
            let lines = vec![
                field("Writes", Span::raw(files.join(", "))),
                field(
                    "Backup",
                    Span::raw("first, in ~/.local/state/neet/backups/dotfiles"),
                ),
                Line::from(vec![
                    Span::raw(format!("{:<LABEL$}", "Changes")).bold(),
                    Span::raw(format!("+{}", diff.added)).green().bold(),
                    Span::raw(" "),
                    Span::raw(format!("−{}", diff.removed)).red().bold(),
                ]),
                field("Then", Span::raw("open a new terminal to use the new PATH")),
            ];
            draw_diff(frame, area, title, lines, diff, false);
        } else if let Some(ask) = &self.ask {
            let (title, lines, color) = self.question(ask);
            tools::message(frame, area, &title, lines, color);
        } else if let Some((note, color)) = &self.note {
            tools::message(
                frame,
                area,
                "Settings",
                vec![
                    Line::from(note.clone()),
                    Line::default(),
                    Line::from("Press any key to go on."),
                ],
                *color,
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if matches!(self.state, State::Looking(..)) {
            return Action::None;
        }
        if self.note.take().is_some() {
            return Action::None;
        }
        if self.typing && self.tab == PATH_TAB {
            match key.code {
                KeyCode::Enter => self.typing = false,
                KeyCode::Backspace => {
                    self.which.pop();
                }
                KeyCode::Char(c) if !c.is_whitespace() && c != '/' => self.which.push(c),
                _ => {}
            }
            return Action::None;
        }
        if self.typing {
            match key.code {
                KeyCode::Enter => self.typing = false,
                KeyCode::Backspace => {
                    self.find.pop();
                }
                KeyCode::Char(c) => self.find.push(c),
                _ => {}
            }
            self.select_first();
            return Action::None;
        }
        if let Some(ask) = &mut self.ask {
            match (ask, key.code) {
                (Ask::Choose { row, choice }, KeyCode::Up | KeyCode::Char('k')) => {
                    let _ = row;
                    *choice = choice.saturating_sub(1);
                }
                (Ask::Choose { row, choice }, KeyCode::Down | KeyCode::Char('j')) => {
                    let last = Self::choices(*row).len().saturating_sub(1);
                    *choice = (*choice + 1).min(last);
                }
                (Ask::PathMenu { choice, .. }, KeyCode::Up | KeyCode::Char('k')) => {
                    *choice = choice.saturating_sub(1);
                }
                (Ask::PathMenu { choice, .. }, KeyCode::Down | KeyCode::Char('j')) => {
                    *choice = (*choice + 1).min(PATH_ACTS.len() - 1);
                }
                (Ask::PathReview { diff, .. }, KeyCode::Up | KeyCode::Char('k')) => {
                    diff.scroll(false);
                }
                (Ask::PathReview { diff, .. }, KeyCode::Down | KeyCode::Char('j')) => {
                    diff.scroll(true);
                }
                (Ask::PathMenu { .. }, KeyCode::Enter) | (_, KeyCode::Char('y')) => {
                    return self.confirm();
                }
                (_, KeyCode::Char('n')) => self.ask = None,
                _ => {}
            }
            return Action::None;
        }
        match key.code {
            KeyCode::Char('c') if self.tab == PATH_TAB => self.clean_path(),
            KeyCode::Char('/') if self.tab == MAC_TAB => {
                self.typing = true;
                self.find.clear();
                self.select_first();
            }
            KeyCode::Tab | KeyCode::Right => self.change_tab((self.tab + 1) % TABS.len()),
            KeyCode::BackTab | KeyCode::Left => {
                self.change_tab((self.tab + TABS.len() - 1) % TABS.len());
            }
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Enter => self.open_choose(),
            KeyCode::Char('u') => self.open_undo(),
            KeyCode::Char('/') if self.tab == PATH_TAB => {
                self.typing = true;
                self.which.clear();
            }
            KeyCode::Char('r') => self.look(),
            _ => {}
        }
        Action::None
    }

    fn ran(&mut self, result: Result<(), String>) {
        let key = self.pending_undo.take();
        self.note = Some(match result {
            Ok(()) => {
                if let Some(key) = key {
                    let _ = self.power_saved.forget(&self.home, key);
                }
                ("Done. pmset has the new value.".into(), Color::Green)
            }
            Err(error) => (format!("pmset did not change: {error}"), Color::Red),
        });
        self.look();
    }

    fn back(&mut self) -> Action {
        if self.typing {
            self.typing = false;
            self.which.clear();
            self.find.clear();
            self.select_first();
            return Action::None;
        }
        if self.ask.take().is_some() || self.note.take().is_some() {
            return Action::None;
        }
        if self.tab == MAC_TAB && !self.find.is_empty() {
            self.find.clear();
            self.select_first();
            return Action::None;
        }
        Action::Back
    }

    fn is_dialog(&self) -> bool {
        self.ask.is_some() || self.note.is_some()
    }

    fn takes_text(&self) -> bool {
        self.typing
    }

    fn hints(&self) -> &'static str {
        if matches!(self.ask, Some(Ask::Choose { .. })) {
            "↑↓ choose · y apply · n or esc cancel"
        } else if matches!(self.ask, Some(Ask::PathMenu { .. })) {
            "↑↓ choose · enter review · esc cancel"
        } else if matches!(self.ask, Some(Ask::PathReview { .. })) {
            "↑↓ scroll · y write · n or esc cancel"
        } else if self.ask.is_some() {
            "y undo · n or esc cancel"
        } else if self.note.is_some() {
            "any key go on"
        } else if self.typing && self.tab == PATH_TAB {
            "type a program name · enter done · esc clear"
        } else if self.typing {
            "type a setting's name · enter done · esc clear"
        } else if self.tab == PATH_TAB {
            "↑↓ move · Tab section · enter change · c clean up · / which copy runs · esc home · ? help"
        } else if self.tab == MAC_TAB {
            "↑↓ move · Tab section · enter change · u undo · / find · r look again · esc home · ? help"
        } else {
            "↑↓ move · Tab section · enter change · u undo · r look again · esc home · ? help"
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            (
                "Tab  ← →",
                "Switch Power, Mac settings, Preferences, and Shell PATH",
            ),
            ("↑ ↓  j k", "Move between settings"),
            ("Enter", "Pick a value. The command shows before it runs"),
            ("y", "Apply the picked value"),
            ("u", "Put a setting back as it was before the change"),
            (
                "/",
                "In Mac settings: find a setting. In Shell PATH: see which copy of a program runs",
            ),
            (
                "Enter",
                "In Shell PATH: remove a folder, or move it to the front or back",
            ),
            (
                "c",
                "In Shell PATH: take out every repeated or missing folder",
            ),
            ("r", "Read every setting again"),
            ("Esc", "Cancel, then back to Home"),
        ]
    }
}

#[cfg(test)]
mod tests;
