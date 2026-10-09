//! Startup: programs that start on their own, grouped by kind, with whether
//! each runs, is turned off, and who signed it. `t` turns your own launch
//! agents off and back on, after a question; everything else is view only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use neet_core::startup::turn::{self, Asked, Saved, Turn};
use neet_core::startup::{self, Item, Kind, Listing, Runs, Signed};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::format;
use super::loading::Loading;
use super::tools;
use super::visual::{self, Columns};

/// From this width the selected item shows on the right.
const MIN_SIDE_WIDTH: u16 = 96;

/// The list's width beside the details
const LIST_WIDTH: (u16, u16) = (48, 70);

/// Width of the status column, such as `○ waiting · !`
const STATUS_WIDTH: u16 = 15;

/// The longest bar in By kind
const BAR: usize = 20;

/// How many signers Signed by names
const SIGNERS: usize = 6;

/// Where System Settings lists apps that open at login
const LOGIN_ITEMS: &str = "x-apple.systempreferences:com.apple.LoginItems-Settings.extension";

/// One row of the list
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Kind(Kind),
    /// A blank row between kinds, on screens with room for it
    Gap,
    /// An index into the listing's items
    Item(usize),
}

enum State {
    Looking(Instant, Receiver<Listing>),
    Ready(Listing),
}

/// News after `t`, until a key is pressed
struct Note {
    title: &'static str,
    lines: Vec<Line<'static>>,
    color: Color,
}

/// Lists programs that start on their own, and turns your own launch agents
/// off and back on.
pub struct Startup {
    home: PathBuf,
    state: State,
    table: TableState,
    /// What neet saved before it last turned each of your launch agents
    saved: HashMap<String, Saved>,
    /// `t`: the launch agent to turn, waiting for `y`
    asked: Option<Asked>,
    note: Option<Note>,
    /// The item to select again once the list is read again
    reselect: Option<PathBuf>,
    /// Whether the list has a blank row between kinds
    gaps: bool,
}

impl Startup {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut screen = Self {
            home,
            state: State::Ready(Listing::default()),
            table: TableState::default(),
            saved: HashMap::new(),
            asked: None,
            note: None,
            reselect: None,
            gaps: false,
        };
        screen.look();
        screen
    }

    #[cfg(test)]
    fn from_listing(home: PathBuf, listing: Listing) -> Self {
        let mut screen = Self {
            home,
            state: State::Ready(listing),
            table: TableState::default(),
            saved: HashMap::new(),
            asked: None,
            note: None,
            reselect: None,
            gaps: false,
        };
        screen.read_saved();
        screen.select_first();
        screen
    }

    /// Reads every item on its own thread, since codesign takes a moment.
    fn look(&mut self) {
        let (sender, receiver) = mpsc::channel();
        let home = self.home.clone();
        thread::spawn(move || {
            // The receiver is gone only when the screen was closed.
            let _ = sender.send(startup::list_mine(&home));
        });
        self.state = State::Looking(Instant::now(), receiver);
    }

    fn poll(&mut self) {
        if let State::Looking(_, receiver) = &self.state
            && let Ok(listing) = receiver.try_recv()
        {
            self.state = State::Ready(listing);
            self.read_saved();
            self.select_first();
            if let Some(file) = self.reselect.take() {
                let listing = self.listing();
                let row = self.rows().iter().position(|row| match row {
                    Entry::Item(index) => listing
                        .and_then(|listing| listing.items.get(*index))
                        .is_some_and(|item| item.file == file),
                    Entry::Kind(_) | Entry::Gap => false,
                });
                if row.is_some() {
                    self.table.select(row);
                }
            }
        }
    }

    /// Reads what neet saved before turning each of your launch agents
    fn read_saved(&mut self) {
        let Some(listing) = self.listing() else {
            return;
        };
        let saved = listing
            .items
            .iter()
            .filter(|item| item.kind == Kind::YourAgent)
            .filter_map(|item| item.label.as_deref())
            .filter_map(|label| Some((label.to_string(), turn::saved(&self.home, label)?)))
            .collect();
        self.saved = saved;
    }

    /// `t`: the question, or why the selected item cannot be turned
    fn ask_turn(&mut self) {
        let Some(item) = self.selected() else {
            return;
        };
        let refused = if item.kind == Kind::YourAgent {
            match turn::ask(item, &self.home) {
                Ok(asked) => {
                    self.asked = Some(asked);
                    return;
                }
                Err(reason) => reason,
            }
        } else {
            view_only(item.kind).to_string()
        };
        self.note = Some(Note {
            title: "Startup",
            lines: vec![Line::from(refused)],
            color: Color::Yellow,
        });
    }

    /// `y`: turns the asked item, then reads the list again
    fn turn(&mut self) {
        let Some(asked) = self.asked.take() else {
            return;
        };
        let (title, done) = match asked.turn {
            Turn::Off => (
                "Turned off",
                "It won't start again, even after a restart, until you turn it back on. Its plist is as it was.",
            ),
            Turn::On => (
                "Turned back on",
                "launchd has it again, and starts it as its plist says.",
            ),
        };
        self.note = Some(match asked.apply(&self.home) {
            Ok(()) => Note {
                title,
                lines: vec![
                    Line::from(vec![
                        Span::raw("✓ ").green().bold(),
                        Span::raw(asked.label.clone()).bold(),
                    ]),
                    Line::default(),
                    Line::from(done),
                ],
                color: Color::Green,
            },
            Err(reason) => Note {
                title: "Not changed",
                lines: vec![Line::from(vec![
                    Span::raw("✗ ").red().bold(),
                    Span::raw(reason).red(),
                ])],
                color: Color::Red,
            },
        });
        self.reselect = Some(asked.plist.clone());
        self.look();
    }

    /// The question before `y`
    fn question(&self, asked: &Asked) -> Vec<Line<'static>> {
        let room = 60;
        let path = |path: &Path| Span::raw(format::shorten_path(&self.tilde(path), room));
        let (verb, what) = match asked.turn {
            Turn::Off => (
                "Turn off",
                "launchctl turns it off, and stops it now if it runs. It stays off after a restart, until you turn it back on.",
            ),
            Turn::On => (
                "Turn back on",
                "launchctl turns it on and loads it, so it runs as its plist says.",
            ),
        };
        let mut lines = vec![
            Line::from(format!("{verb} {}?", asked.label)).bold(),
            Line::default(),
        ];
        if let Some(program) = &asked.program {
            lines.push(field("Program", path(program)));
        }
        lines.push(field("Plist", path(&asked.plist)));
        lines.push(Line::default());
        lines.push(Line::from(what));
        lines.push(Line::from(
            "The plist is not changed. The state before is saved in ~/.local/state/neet/startup.",
        ));
        lines
    }

    fn listing(&self) -> Option<&Listing> {
        match &self.state {
            State::Ready(listing) => Some(listing),
            State::Looking(..) => None,
        }
    }

    /// Each kind that has an item, then its items
    fn rows(&self) -> Vec<Entry> {
        let Some(listing) = self.listing() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for kind in Kind::ALL {
            let mut first = true;
            for (index, item) in listing.items.iter().enumerate() {
                if item.kind != kind {
                    continue;
                }
                if first {
                    if self.gaps && !rows.is_empty() {
                        rows.push(Entry::Gap);
                    }
                    rows.push(Entry::Kind(kind));
                    first = false;
                }
                rows.push(Entry::Item(index));
            }
        }
        rows
    }

    /// Turns the blank rows between kinds on or off, keeping the same item
    /// selected
    fn set_gaps(&mut self, gaps: bool) {
        if self.gaps == gaps {
            return;
        }
        let selected = self
            .table
            .selected()
            .and_then(|row| self.rows().get(row).copied());
        self.gaps = gaps;
        if let Some(selected) = selected {
            let row = self.rows().iter().position(|row| *row == selected);
            self.table.select(row);
        }
    }

    fn selected(&self) -> Option<&Item> {
        match self.rows().get(self.table.selected()?) {
            Some(Entry::Item(index)) => self.listing()?.items.get(*index),
            _ => None,
        }
    }

    fn select_first(&mut self) {
        let first = self
            .rows()
            .iter()
            .position(|row| matches!(row, Entry::Item(_)));
        self.table.select(first);
    }

    /// Moves to the next item up or down, past kind names
    fn step(&mut self, down: bool) {
        let rows = self.rows();
        let Some(current) = self.table.selected() else {
            return;
        };
        let next = if down {
            (current + 1..rows.len()).find(|&index| matches!(rows[index], Entry::Item(_)))
        } else {
            (0..current)
                .rev()
                .find(|&index| matches!(rows[index], Entry::Item(_)))
        };
        if let Some(next) = next {
            self.table.select(Some(next));
        }
    }

    fn jump(&mut self, last: bool) {
        let rows = self.rows();
        let found = if last {
            rows.iter().rposition(|row| matches!(row, Entry::Item(_)))
        } else {
            rows.iter().position(|row| matches!(row, Entry::Item(_)))
        };
        if found.is_some() {
            self.table.select(found);
        }
    }

    /// A path from the home folder as `~/...`
    fn tilde(&self, path: &Path) -> String {
        if self.home.as_os_str().is_empty() {
            return path.display().to_string();
        }
        path.strip_prefix(&self.home).map_or_else(
            |_| path.display().to_string(),
            |rest| format!("~/{}", rest.display()),
        )
    }

    /// The line at the top: how many items, running, and turned off
    fn draw_top(&self, frame: &mut Frame, area: Rect) {
        let mut spans = vec![Span::raw(" ")];
        if let Some(listing) = self.listing() {
            let items = &listing.items;
            let running = items
                .iter()
                .filter(|item| matches!(item.runs, Runs::Running(_)))
                .count();
            let off = items.iter().filter(|item| item.off).count();
            spans.push(Span::raw(format!(
                "{} {}",
                items.len(),
                if items.len() == 1 { "item" } else { "items" }
            )));
            spans.push(Span::raw(" · "));
            spans.push(Span::raw(format!("{running} running")).green());
            if off > 0 {
                spans.push(Span::raw(" · "));
                spans.push(Span::raw(format!("{off} turned off")).yellow());
            }
        }
        spans.push(Span::raw(" "));
        frame.render_widget(
            Block::new()
                .borders(Borders::TOP)
                .title(" Startup ")
                .title_style(visual::HEADING)
                .title(Line::from(spans).right_aligned()),
            area,
        );
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.rows();
        let block = visual::block()
            .title(" Programs ")
            .padding(Padding::horizontal(1));
        let Some(listing) = self.listing() else {
            return;
        };
        if rows.is_empty() {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(
                Paragraph::new("Nothing starts on its own, other than macOS itself.")
                    .centered()
                    .wrap(Wrap { trim: true }),
                inner,
            );
            return;
        }
        let longest = listing
            .items
            .iter()
            .map(|item| format::display_width(&item.name))
            .max()
            .unwrap_or(0);
        let name_width = u16::try_from(longest + 4).unwrap_or(u16::MAX).max(14);
        let columns = Columns::new(area.width, [(name_width, 0), (STATUS_WIDTH, 1)], &[], true);
        let table_rows: Vec<Row> = rows
            .iter()
            .map(|row| match *row {
                Entry::Gap => columns.row([Cell::from(""), Cell::from("")]),
                Entry::Kind(kind) => columns.row([
                    Cell::from(Span::styled(kind.title(), visual::HEADING)),
                    Cell::from(""),
                ]),
                Entry::Item(index) => {
                    let item = &listing.items[index];
                    let name =
                        format::shorten_middle(&item.name, columns.width(0).saturating_sub(2));
                    columns.row([Cell::from(format!("  {name}")), Cell::from(status(item))])
                }
            })
            .collect();
        let table = Table::new(table_rows, columns.widths())
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// The selected item's lines, with long paths wrapped under their label
    fn details(&self, item: &Item, width: usize) -> Vec<Line<'static>> {
        let path = |label: &str, path: &Path| {
            wrap_path(&self.tilde(path), width.saturating_sub(9).max(1))
                .into_iter()
                .enumerate()
                .map(|(index, part)| {
                    let head = if index == 0 { label } else { "" };
                    Line::from(vec![
                        Span::raw(format!("{head:<9}")).bold(),
                        Span::raw(part),
                    ])
                })
                .collect::<Vec<_>>()
        };
        let mut lines = vec![field("Kind", Span::raw(kind_one(item.kind)))];
        if let Some(app) = &item.app {
            lines.push(field("App", Span::raw(app.clone())));
        }
        if let Some(label) = &item.label
            && *label != item.name
        {
            lines.push(field("Label", Span::raw(label.clone())));
        }
        if let Some(program) = &item.program {
            lines.extend(path("Program", program));
        }
        let file_label = if item.file.extension().is_some_and(|ext| ext == "plist") {
            "Plist"
        } else {
            "Helper"
        };
        lines.extend(path(file_label, &item.file));
        lines.push(field("Signed", signed(&item.signed)));
        lines.push(field(
            "Starts",
            Span::raw(match (item.at_start, item.kind) {
                (true, Kind::Daemon) => "when the Mac starts",
                (true, _) => "when you log in",
                (false, _) => "only when a program asks for it",
            }),
        ));
        lines.push(field("Now", now(item)));
        if let Some(saved) = item.label.as_ref().and_then(|label| self.saved.get(label)) {
            let day = saved.at.get(..10).unwrap_or(&saved.at);
            let turned = match saved.turned {
                Turn::Off => "off",
                Turn::On => "back on",
            };
            lines.push(field(
                "Changed",
                Span::raw(format!("neet turned it {turned} on {day}")),
            ));
        }
        lines.push(Line::default());
        if let Some(problem) = &item.problem {
            lines.push(Line::from(format!("! {problem}")).yellow());
        }
        lines.push(match item.kind {
            Kind::YourAgent => Line::from(vec![
                Span::raw("Press "),
                Span::raw("t").fg(visual::ACCENT).bold(),
                Span::raw(if item.off {
                    " to turn it back on."
                } else {
                    " to turn it off. Its plist stays as it is."
                }),
            ]),
            kind => Line::from(view_only(kind)),
        });
        lines
    }

    fn draw_details(&self, frame: &mut Frame, area: Rect, item: &Item) {
        let lines = self.details(item, usize::from(area.width.saturating_sub(4)));
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                visual::block()
                    .title(Line::from(format!(" {} ", item.name)).bold())
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
    }

    /// Apps that open at login, and what macOS keeps to itself
    fn about(&self) -> Vec<Line<'static>> {
        let mut login = vec![
            Line::from("Open at login").style(visual::HEADING),
            Line::from(
                "macOS shows apps that open at login only to an admin, so they are not listed here.",
            ),
            Line::from(vec![
                Span::raw("o").fg(visual::ACCENT).bold(),
                Span::raw(" opens them in System Settings, where you can turn them off."),
            ]),
        ];
        if let Some(listing) = self.listing() {
            if let Some(incomplete) = &listing.incomplete {
                login.push(Line::from(incomplete.clone()).yellow());
            }
            if listing.from_macos > 0 {
                login.push(Line::from(format!(
                    "{} more belong to macOS, and are not listed.",
                    listing.from_macos
                )));
            }
        }
        login
    }

    /// What `t` does, and the launch agents neet has turned off
    fn turning(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from("Turning off").style(visual::HEADING),
            Line::from(vec![
                Span::raw("Select one of your launch agents and press "),
                Span::raw("t").fg(visual::ACCENT).bold(),
                Span::raw(". neet asks first, and never changes its plist."),
            ]),
        ];
        let Some(listing) = self.listing() else {
            return lines;
        };
        let mut off: Vec<(&str, &str)> = listing
            .items
            .iter()
            .filter(|item| item.off)
            .filter_map(|item| {
                let saved = self.saved.get(item.label.as_deref()?)?;
                (saved.turned == Turn::Off)
                    .then(|| (item.name.as_str(), saved.at.get(..10).unwrap_or(&saved.at)))
            })
            .collect();
        off.sort_unstable();
        if off.is_empty() {
            lines.push(Line::from("neet has not turned anything off."));
        } else {
            lines.push(Line::from("Turned off by neet, t turns each back on:"));
            for (name, day) in off {
                lines.push(Line::from(vec![
                    Span::raw(format!("  {name}  ")).yellow(),
                    Span::raw(format!("on {day}")),
                ]));
            }
        }
        lines
    }

    /// How many of each kind, and how many of those run
    fn counts(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("By kind").style(visual::HEADING)];
        let Some(listing) = self.listing() else {
            return lines;
        };
        let widest = Kind::ALL
            .iter()
            .map(|kind| listing.items.iter().filter(|i| i.kind == *kind).count())
            .max()
            .unwrap_or(0)
            .min(BAR);
        for kind in Kind::ALL {
            let items: Vec<&Item> = listing.items.iter().filter(|i| i.kind == kind).collect();
            if items.is_empty() {
                continue;
            }
            let running = items
                .iter()
                .filter(|item| matches!(item.runs, Runs::Running(_)))
                .count();
            let off = items.iter().filter(|item| item.off).count();
            // One cell an item, the running ones green
            let mut spans = vec![
                Span::raw(format!("{:<30}", kind.title())).bold(),
                Span::raw(format!("{:>3} ", items.len())),
                Span::raw("█".repeat(running.min(BAR))).green(),
                Span::raw("█".repeat(items.len().min(BAR).saturating_sub(running)))
                    .fg(visual::ACCENT),
                Span::raw(" ".repeat(widest.saturating_sub(items.len().min(BAR)))),
            ];
            if running > 0 {
                spans.push(Span::raw(" · "));
                spans.push(Span::raw(format!("{running} running")).green());
            }
            if off > 0 {
                spans.push(Span::raw(" · "));
                spans.push(Span::raw(format!("{off} off")).yellow());
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    /// Who signed the programs, most first
    fn signers(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("Signed by").style(visual::HEADING)];
        let Some(listing) = self.listing() else {
            return lines;
        };
        let mut counts: HashMap<String, (usize, bool)> = HashMap::new();
        for item in &listing.items {
            let (name, warn) = match &item.signed {
                Signed::Unsigned => ("not signed".to_string(), true),
                Signed::Unknown => ("not known".to_string(), false),
                other => (signed(other).content.into_owned(), false),
            };
            counts.entry(name).or_insert((0, warn)).0 += 1;
        }
        let mut counts: Vec<(String, (usize, bool))> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
        let shown = counts.len().min(SIGNERS);
        for (name, (count, warn)) in &counts[..shown] {
            let name = format!("{:<30}", format::shorten_middle(name, 29));
            lines.push(Line::from(vec![
                if *warn {
                    Span::raw(name).yellow().bold()
                } else {
                    Span::raw(name).bold()
                },
                Span::raw(format!("{count:>3}")),
            ]));
        }
        if counts.len() > shown {
            lines.push(Line::from(format!("{} more", counts.len() - shown)));
        }
        lines
    }

    /// What the marks in the list mean
    fn legend() -> Vec<Line<'static>> {
        vec![
            Line::from("Legend").style(visual::HEADING),
            legend_line(
                Span::raw("● running").green(),
                "running now, with a process",
            ),
            legend_line(
                Span::raw("○ waiting").fg(visual::ACCENT),
                "loaded, and starts when it is needed",
            ),
            legend_line(Span::raw("· not loaded"), "launchd does not have it now"),
            legend_line(
                Span::raw("off").yellow(),
                "turned off, even after a restart",
            ),
            legend_line(
                Span::raw("!").yellow().bold(),
                "something is wrong; select it to see what",
            ),
        ]
    }

    /// The list sized to its rows, with the counts and the legend below it
    /// when there is room
    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let counts = self.counts();
        let counts_height = u16::try_from(counts.len()).unwrap_or(u16::MAX) + 1;
        let height = |screen: &Self| {
            u16::try_from(screen.rows().len())
                .unwrap_or(u16::MAX)
                .max(3)
                .saturating_add(2)
        };
        self.set_gaps(true);
        if area.height < height(self) + counts_height {
            self.set_gaps(false);
        }
        let table_height = height(self);
        if area.height < table_height + counts_height {
            self.draw_table(frame, area);
            return;
        }
        let [table, below] =
            Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)]).areas(area);
        self.draw_table(frame, table);
        visual::sections(frame, below, "", vec![counts, self.signers()]);
    }

    fn draw_body(&mut self, frame: &mut Frame, body: Rect) {
        let selected = self.selected().cloned();
        if body.width >= MIN_SIDE_WIDTH {
            let width = (body.width * 45 / 100).clamp(LIST_WIDTH.0, LIST_WIDTH.1);
            let [list, side] =
                Layout::horizontal([Constraint::Length(width), Constraint::Fill(1)]).areas(body);
            self.draw_list(frame, list);
            let about = vec![self.about(), self.turning(), Self::legend()];
            match selected {
                Some(item) => {
                    let lines = self.details(&item, usize::from(side.width.saturating_sub(4)));
                    let height = (visual::wrapped_rows(&lines, side.width.saturating_sub(4)) + 2)
                        .min(side.height);
                    let [details, below] =
                        Layout::vertical([Constraint::Length(height), Constraint::Fill(1)])
                            .areas(side);
                    self.draw_details(frame, details, &item);
                    visual::sections(frame, below, "", about);
                }
                None => visual::sections(frame, side, "", about),
            }
            return;
        }
        match selected {
            Some(item) if body.height >= 20 => {
                let width = body.width.saturating_sub(4);
                let lines = self.details(&item, usize::from(width));
                let height = (visual::wrapped_rows(&lines, width) + 2).min(body.height / 2);
                let [list, below] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(body);
                self.draw_table(frame, list);
                self.draw_details(frame, below, &item);
            }
            _ => self.draw_table(frame, body),
        }
    }

    /// Opens Login Items in System Settings.
    fn open_login_items() {
        // System Settings opens on its own; neet does not wait for it.
        let _ = std::process::Command::new("/usr/bin/open")
            .arg(LOGIN_ITEMS)
            .spawn();
    }

    /// Shows the selected item's plist or helper in Finder.
    fn show_in_finder(&self) {
        if let Some(item) = self.selected() {
            let _ = std::process::Command::new("/usr/bin/open")
                .arg("-R")
                .arg(&item.file)
                .spawn();
        }
    }
}

/// One of this kind, in a few words
fn kind_one(kind: Kind) -> &'static str {
    match kind {
        Kind::Background => "a helper inside an app",
        Kind::YourAgent => "your launch agent",
        Kind::AgentForAll => "a launch agent for every user",
        Kind::Daemon => "a launch daemon, run by the system",
    }
}

/// Why the item cannot be changed here
fn view_only(kind: Kind) -> &'static str {
    match kind {
        Kind::Background => {
            "View only. Press o to turn it off in System Settings, under Allow in the Background."
        }
        Kind::YourAgent => "Press t to turn it off, or back on.",
        Kind::AgentForAll | Kind::Daemon => {
            "View only. An admin set it up for every user, sometimes to manage this Mac."
        }
    }
}

fn status(item: &Item) -> Line<'static> {
    let mut spans = vec![if item.off {
        Span::raw("off").yellow()
    } else {
        match item.runs {
            Runs::Running(_) => Span::raw("● running").green(),
            Runs::Waiting => Span::raw("○ waiting").fg(visual::ACCENT),
            Runs::No => Span::raw("· not loaded"),
            Runs::Unknown => Span::raw("? not known"),
        }
    }];
    if item.problem.is_some() {
        spans.push(Span::raw(" !").yellow().bold());
    }
    Line::from(spans)
}

fn now(item: &Item) -> Span<'static> {
    let runs = match item.runs {
        Runs::Running(0) => "running".to_string(),
        Runs::Running(pid) => format!("running, process {pid}"),
        Runs::Waiting => "waiting, and starts when it is needed".to_string(),
        Runs::No => "not running".to_string(),
        Runs::Unknown => "not known".to_string(),
    };
    if item.off {
        Span::raw(format!("turned off · {runs}")).yellow()
    } else if matches!(item.runs, Runs::Running(_)) {
        Span::raw(runs).green()
    } else {
        Span::raw(runs)
    }
}

fn signed(signed: &Signed) -> Span<'static> {
    match signed {
        Signed::Apple => Span::raw("Apple"),
        Signed::By(name) => Span::raw(name.clone()),
        Signed::AppStore(team) if team.is_empty() => Span::raw("an App Store app"),
        Signed::AppStore(team) => Span::raw(format!("an App Store app, team {team}")),
        Signed::Unsigned => Span::raw("not signed").yellow(),
        Signed::Unknown => Span::raw("not known"),
    }
}

/// `path` in lines of at most `width` characters, broken after a `/`, and
/// inside a name only when the name alone is too long
fn wrap_path(path: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for part in path.split_inclusive('/') {
        let fits = current.chars().count() + part.chars().count() <= width;
        if !fits && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        let mut part: Vec<char> = part.chars().collect();
        while part.len() > width {
            lines.push(part.drain(..width).collect());
        }
        current.extend(part);
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<9}")).bold(), value])
}

fn legend_line(mark: Span<'static>, meaning: &'static str) -> Line<'static> {
    let width = format::display_width(&mark.content);
    Line::from(vec![
        mark,
        Span::raw(" ".repeat(14usize.saturating_sub(width))),
        Span::raw(meaning),
    ])
}

impl Screen for Startup {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        if let State::Looking(started, _) = &self.state {
            Loading {
                title: "Startup",
                doing: "Looking for startup programs",
                progress: format!("{}s", started.elapsed().as_secs()),
                note: "Nothing is changed while neet looks.",
            }
            .draw(frame, area);
            return;
        }
        let [top, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
        self.draw_top(frame, top);
        self.draw_body(frame, body);
        if let Some(asked) = &self.asked {
            let (title, yes, color) = match asked.turn {
                Turn::Off => ("Turn off", "turn it off", Color::Yellow),
                Turn::On => ("Turn back on", "turn it on", visual::ACCENT),
            };
            tools::ask(frame, area, title, self.question(asked), yes, color);
        } else if let Some(note) = &self.note {
            let mut lines = note.lines.clone();
            lines.push(Line::default());
            lines.push(Line::from("Press any key to go on."));
            tools::message(frame, area, note.title, lines, note.color);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if self.listing().is_none() {
            return Action::None;
        }
        if self.asked.is_some() {
            match key.code {
                KeyCode::Char('y') => self.turn(),
                KeyCode::Char('n') => self.asked = None,
                _ => {}
            }
            return Action::None;
        }
        if self.note.take().is_some() {
            return Action::None;
        }
        match key.code {
            KeyCode::Char('t') => self.ask_turn(),
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Char('g') | KeyCode::Home => self.jump(false),
            KeyCode::Char('G') | KeyCode::End => self.jump(true),
            KeyCode::Char('o') => Self::open_login_items(),
            KeyCode::Char('f') => self.show_in_finder(),
            KeyCode::Char('r') => self.look(),
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        if self.asked.take().is_some() || self.note.take().is_some() {
            return Action::None;
        }
        Action::Back
    }

    fn is_dialog(&self) -> bool {
        self.asked.is_some() || self.note.is_some()
    }

    fn hints(&self) -> &'static str {
        if self.asked.is_some() {
            "y go ahead · n or esc go back"
        } else if self.note.is_some() {
            "any key go on"
        } else {
            "↑↓ move · t turn off or on · o login items · f show in Finder · r look again · esc home · ? help"
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑↓", "Move between programs"),
            ("g G", "First or last program"),
            (
                "t",
                "Turn your own launch agent off, or back on, after a question",
            ),
            ("y", "In the question: go ahead"),
            ("o", "Open Login Items in System Settings"),
            ("f", "Show the selected plist or helper in Finder"),
            ("r", "Look again"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use crate::ui::visual::tests as view;

    fn item(kind: Kind, name: &str, runs: Runs) -> Item {
        Item {
            kind,
            name: name.to_string(),
            label: Some(name.to_string()),
            file: PathBuf::from(format!("/home/Library/LaunchAgents/{name}.plist")),
            program: Some(PathBuf::from("/usr/local/bin/x")),
            runs,
            off: false,
            at_start: true,
            signed: Signed::By("Google LLC".to_string()),
            app: None,
            problem: None,
        }
    }

    fn sample() -> Listing {
        let mut empty = item(Kind::YourAgent, "com.google.keystone.agent", Runs::No);
        empty.problem = Some("Its plist is empty, so it does nothing.".to_string());
        empty.signed = Signed::Unknown;
        let mut helper = item(Kind::Background, "DockerHelper", Runs::Running(44));
        helper.app = Some("Docker".to_string());
        helper.label = Some("com.docker.helper".to_string());
        helper.file =
            PathBuf::from("/Applications/Docker.app/Contents/Library/LoginItems/DockerHelper.app");
        let mut off = item(Kind::AgentForAll, "com.vmware.deem", Runs::No);
        off.off = true;
        Listing {
            items: vec![
                helper,
                item(
                    Kind::YourAgent,
                    "com.google.GoogleUpdater.wake",
                    Runs::Waiting,
                ),
                empty,
                off,
                item(Kind::Daemon, "com.docker.vmnetd", Runs::Running(529)),
            ],
            incomplete: None,
            from_macos: 3,
        }
    }

    fn render(screen: &mut Startup, width: u16, height: u16) -> String {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        view::text(&view::render("startup", screen, &context, width, height))
    }

    fn press(screen: &mut Startup, code: KeyCode) {
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
        );
    }

    #[test]
    fn lists_items_by_kind_with_their_status() {
        let mut screen = Startup::from_listing(PathBuf::from("/home"), sample());
        let text = render(&mut screen, 140, 40);
        assert!(
            text.contains("5 items · 2 running · 1 turned off"),
            "{text}"
        );
        for kind in Kind::ALL {
            assert!(text.contains(kind.title()), "{}\n{text}", kind.title());
        }
        assert!(text.find("Allowed in the background") < text.find("Your launch agents"));
        assert!(text.contains("● running"));
        assert!(text.contains("○ waiting"));
        assert!(text.contains("· not loaded !"), "{text}");
        assert!(text.contains("off"));
        assert!(text.contains("3 more belong to macOS"));
        assert!(text.contains("o opens them in System Settings"));
        assert!(
            text.contains("Launch daemons                  1 █  · 1 running"),
            "{text}"
        );
    }

    #[test]
    fn shows_the_selected_item_and_why_it_is_view_only() {
        let mut screen = Startup::from_listing(PathBuf::from("/home"), sample());
        let text = render(&mut screen, 140, 40);
        assert!(text.contains("App      Docker"), "{text}");
        assert!(text.contains("Label    com.docker.helper"));
        assert!(text.contains("Helper   /Applications/"), "{text}");
        assert!(text.contains("DockerHelper.app"), "{text}");
        assert!(text.contains("Signed   Google LLC"));
        assert!(text.contains("running, process 44"));
        assert!(text.contains("Press o to turn it off in System Settings"));

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Down);
        let text = render(&mut screen, 140, 40);
        assert!(
            text.contains("Plist    ~/Library/LaunchAgents/com.google.keystone.agent.plist"),
            "{text}"
        );
        assert!(text.contains("! Its plist is empty"), "{text}");
        assert!(
            text.contains("Press t to turn it off. Its plist stays"),
            "{text}"
        );

        press(&mut screen, KeyCode::Char('G'));
        let text = render(&mut screen, 140, 40);
        assert!(text.contains("when the Mac starts"), "{text}");
        assert!(text.contains("An admin set it up for every user"));
    }

    #[test]
    fn t_asks_first_and_n_goes_back() {
        let dir = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(dir.path()).unwrap();
        let agents = home.join("Library/LaunchAgents");
        std::fs::create_dir_all(&agents).unwrap();
        let mut listing = sample();
        let agent = &mut listing.items[1];
        agent.file = agents.join("com.google.GoogleUpdater.wake.plist");
        std::fs::write(&agent.file, "<plist/>").unwrap();
        let mut screen = Startup::from_listing(home, listing);
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Char('t'));
        assert!(screen.is_dialog());
        let text = render(&mut screen, 140, 40);
        assert!(
            text.contains("Turn off com.google.GoogleUpdater.wake?"),
            "{text}"
        );
        assert!(
            text.contains("Plist    ~/Library/LaunchAgents/com.google.GoogleUpdater.wake.plist"),
            "{text}"
        );
        assert!(text.contains("stops it now if it runs"), "{text}");
        assert!(text.contains("y turn it off"), "{text}");
        press(&mut screen, KeyCode::Char('n'));
        assert!(!screen.is_dialog());
        assert!(!render(&mut screen, 140, 40).contains("Turn off com.google"));
    }

    #[test]
    fn t_says_why_other_items_stay_as_they_are() {
        let mut screen = Startup::from_listing(PathBuf::from("/home"), sample());
        press(&mut screen, KeyCode::Char('G'));
        press(&mut screen, KeyCode::Char('t'));
        let text = render(&mut screen, 140, 40);
        assert!(text.contains("An admin set it up for every user"), "{text}");
        assert!(text.contains("Press any key to go on"), "{text}");
        press(&mut screen, KeyCode::Char('x'));
        assert!(!screen.is_dialog());

        // Your agent whose plist is not there any more is refused, not asked.
        press(&mut screen, KeyCode::Char('g'));
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Char('t'));
        let text = render(&mut screen, 140, 40);
        assert!(text.contains("could not be read"), "{text}");
    }

    #[test]
    fn long_paths_wrap_after_a_slash() {
        assert_eq!(
            wrap_path("/Applications/Docker.app/Contents/Library", 20),
            ["/Applications/", "Docker.app/Contents/", "Library"]
        );
        assert_eq!(wrap_path("/abcdefghij", 4), ["/", "abcd", "efgh", "ij"]);
        assert_eq!(wrap_path("", 4), [""]);
    }

    #[test]
    fn fits_a_narrow_screen() {
        let mut screen = Startup::from_listing(PathBuf::from("/home"), sample());
        let text = render(&mut screen, 80, 30);
        assert!(text.contains("DockerHelper"), "{text}");
        assert!(text.contains("Signed"), "{text}");
    }

    #[test]
    fn says_when_nothing_starts() {
        let mut screen = Startup::from_listing(PathBuf::from("/home"), Listing::default());
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Nothing starts on its own"), "{text}");
        assert!(text.contains("Open at login"), "{text}");
    }
}
