use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Instant;

use neet_core::apps;
use neet_core::clean::RulePlan;
use neet_core::removal::{self, App, Mark, Refusal};
use neet_core::safety::CleanupRoots;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Planned, checkbox, display_path, skip_reason};
use super::format;
use super::loading::Loading;
use super::review::Review;

fn step(list: &mut TableState, rows: usize, code: KeyCode) {
    let end = rows.saturating_sub(1);
    let index = list.selected().unwrap_or(0);
    let next = match code {
        KeyCode::Up | KeyCode::Char('k') => index.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => (index + 1).min(end),
        KeyCode::Char('g') | KeyCode::Home => 0,
        KeyCode::Char('G') | KeyCode::End => end,
        _ => return,
    };
    list.select(Some(next));
}

/// Width of the size bar beside each app
const APP_BAR: usize = 12;

/// Width of the labels in the boxes below the lists
const LABEL: usize = 10;

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<LABEL$}")).bold(), value])
}

/// Why neet will not remove an app, in plain words
fn refusal_text(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::AppleApp => "part of macOS",
        Refusal::Link => "a shortcut to another app",
        Refusal::NoBundleId => "neet can't tell which app it is",
    }
}

/// From this many spare rows under a list, the boxes go below it
const BOXES: u16 = 8;

/// How many apps are measured at once
const MEASURERS: usize = 4;

/// Where measuring one app is
#[derive(Clone, Copy)]
enum Measured {
    Waiting,
    Size(u64),
    /// It could not be read.
    Failed,
}

/// Each app's size, by path, kept while neet runs
static SIZES: LazyLock<Mutex<HashMap<PathBuf, Measured>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn sizes() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Measured>> {
    SIZES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Starts measuring, in the background, every app in `paths` that is not
/// measured or being measured yet, several at once. Sizes are kept while
/// neet runs, so Remove App opens with them.
pub fn measure(paths: Vec<PathBuf>) {
    let queue: Vec<PathBuf> = {
        let mut sizes = sizes();
        paths
            .into_iter()
            .filter(|path| {
                let new = !sizes.contains_key(path);
                if new {
                    sizes.insert(path.clone(), Measured::Waiting);
                }
                new
            })
            .collect()
    };
    let queue = Arc::new(Mutex::new(queue));
    for _ in 0..MEASURERS {
        let queue = Arc::clone(&queue);
        thread::spawn(move || {
            loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop();
                let Some(path) = next else { return };
                let measured = removal::app_size(&path).map_or(Measured::Failed, Measured::Size);
                sizes().insert(path, measured);
            }
        });
    }
}

/// Measures the apps in the Applications folders when neet starts, so Remove
/// App has their sizes when it opens
pub fn measure_at_start(home: PathBuf) {
    thread::spawn(move || {
        if let Ok(roots) = CleanupRoots::new(&home) {
            measure(
                removal::list_apps(&roots)
                    .into_iter()
                    .map(|app| app.path)
                    .collect(),
            );
        }
    });
}

/// The apps in the Applications folders, each with its size, measured in the
/// background. Apps neet will not remove are dimmed with the reason.
pub struct RemoveApp {
    roots: Option<CleanupRoots>,
    apps: Vec<App>,
    /// Each app's size, by path, as it is measured
    sizes: HashMap<PathBuf, u64>,
    /// Whether some of the apps are still being measured
    measuring: bool,
    /// Largest first, instead of by name
    by_size: bool,
    list: TableState,
    is_running: fn(&str) -> bool,
    note: Option<String>,
    failed: Option<String>,
}

impl RemoveApp {
    pub fn new() -> Self {
        let roots = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set.".to_string())
            .and_then(|home| {
                CleanupRoots::new(&home)
                    .map_err(|error| format!("The home folder could not be read: {error}"))
            });
        match roots {
            Ok(roots) => {
                let apps = removal::list_apps(&roots);
                measure(apps.iter().map(|app| app.path.clone()).collect());
                let mut screen = Self::from_apps(roots, apps, apps::is_running);
                screen.measuring = true;
                screen.poll();
                screen
            }
            Err(reason) => Self {
                roots: None,
                apps: Vec::new(),
                sizes: HashMap::new(),
                measuring: false,
                by_size: false,
                list: TableState::default(),
                is_running: apps::is_running,
                note: None,
                failed: Some(reason),
            },
        }
    }

    fn from_apps(roots: CleanupRoots, mut apps: Vec<App>, is_running: fn(&str) -> bool) -> Self {
        // Apps neet can remove first, each group by name
        apps.sort_by_cached_key(|app| (app.refused.is_some(), app.name.to_lowercase()));
        Self {
            roots: Some(roots),
            apps,
            sizes: HashMap::new(),
            measuring: false,
            by_size: false,
            list: TableState::default().with_selected(Some(0)),
            is_running,
            note: None,
            failed: None,
        }
    }

    /// Picks up the sizes measured so far
    fn poll(&mut self) {
        if !self.measuring {
            return;
        }
        let sizes = sizes();
        let mut waiting = false;
        for app in &self.apps {
            match sizes.get(&app.path) {
                Some(Measured::Size(size)) => {
                    self.sizes.insert(app.path.clone(), *size);
                }
                Some(Measured::Failed) => {}
                Some(Measured::Waiting) | None => waiting = true,
            }
        }
        drop(sizes);
        if !waiting {
            self.measuring = false;
            if self.by_size {
                self.sort();
            }
        }
    }

    /// Sorts by name or by size, keeping the same app selected
    fn sort(&mut self) {
        let current = self
            .list
            .selected()
            .and_then(|index| self.apps.get(index))
            .map(|app| app.path.clone());
        if self.by_size {
            let sizes = &self.sizes;
            self.apps.sort_by_key(|app| {
                (
                    app.refused.is_some(),
                    Reverse(sizes.get(&app.path).copied().unwrap_or(0)),
                )
            });
        } else {
            self.apps
                .sort_by_cached_key(|app| (app.refused.is_some(), app.name.to_lowercase()));
        }
        let index = current.and_then(|path| self.apps.iter().position(|app| app.path == path));
        self.list.select(Some(index.unwrap_or(0)));
    }

    fn open(&mut self) -> Action {
        let (Some(roots), Some(app)) = (
            &self.roots,
            self.list.selected().and_then(|index| self.apps.get(index)),
        ) else {
            return Action::None;
        };
        if let Some(refusal) = app.refused {
            self.note = Some(format!(
                "neet does not remove {}: {}.",
                app.name,
                refusal_text(refusal)
            ));
            return Action::None;
        }
        if app.bundle_id.as_deref().is_some_and(self.is_running) {
            self.note = Some(format!("Quit {} first, then open it here.", app.name));
            return Action::None;
        }
        Action::Open(Box::new(AppFiles::start(roots.clone(), app.clone())))
    }

    fn app_row(
        &self,
        app: &App,
        home: Option<&Path>,
        largest: u64,
        columns: &super::visual::Columns<5>,
    ) -> Row<'static> {
        let folder = app
            .path
            .parent()
            .map_or_else(String::new, |folder| match home {
                Some(home) => display_path(home, folder),
                None => folder.display().to_string(),
            });
        // Your own Applications folder in magenta, the shared one in blue
        let folder = if folder.starts_with('~') {
            Span::raw(folder).magenta()
        } else {
            Span::raw(folder).fg(super::visual::ACCENT)
        };
        let id = app.bundle_id.clone().unwrap_or_default();
        let (size, bar) = match self.sizes.get(&app.path) {
            Some(&size) => (
                format::size_span(size, format::size(size)),
                size_bar(size, largest, APP_BAR),
            ),
            None if self.measuring => (Span::raw("…").fg(super::visual::ACCENT), Span::raw("")),
            None => (Span::raw(""), Span::raw("")),
        };
        match app.refused {
            // Keep the refusal readable even when the row cannot be opened.
            Some(refusal) => columns.row([
                Cell::from(
                    Span::raw(format::shorten_middle(
                        &format!("{} [blocked]", app.name),
                        columns.width(0),
                    ))
                    .yellow(),
                ),
                Cell::from(Line::from(size).right_aligned()),
                Cell::from(""),
                Cell::from(folder.content),
                Cell::from(Span::raw(format!("blocked: {}", refusal_text(refusal))).yellow()),
            ]),
            None => columns.row([
                Cell::from(format::shorten_middle(&app.name, columns.width(0))),
                Cell::from(Line::from(size).right_aligned()),
                Cell::from(bar),
                Cell::from(folder),
                Cell::from(Span::raw(id).fg(super::visual::ACCENT)),
            ]),
        }
    }
}

impl RemoveApp {
    /// The app the arrow is on: where it is, its size, its ID, and whether
    /// neet can remove it
    fn app_lines(&self, app: &App, width: usize) -> Vec<Line<'static>> {
        let size = match self.sizes.get(&app.path) {
            Some(&size) => format::size_span(size, format::size(size)),
            None if self.measuring => Span::raw("measuring…"),
            None => Span::raw("unknown"),
        };
        let open = app.bundle_id.as_deref().is_some_and(self.is_running);
        let (status, about) = match app.refused {
            Some(refusal) => (
                Span::raw(format!("blocked, {}", refusal_text(refusal))).yellow(),
                "neet leaves this one alone.",
            ),
            None if open => (
                Span::raw("open, quit it first").yellow(),
                "Quit the app, then press Enter to see its files.",
            ),
            None => (
                Span::raw("can be removed").green(),
                "Press Enter to see the app & the files it keeps around your Mac.",
            ),
        };
        let path = display_path_or_full(self.roots.as_ref(), &app.path);
        vec![
            field(
                "Location",
                Span::raw(format::shorten_path(&path, width.saturating_sub(LABEL)))
                    .fg(super::visual::ACCENT),
            ),
            field("Size", size),
            field(
                "App ID",
                Span::raw(format::shorten_middle(
                    app.bundle_id.as_deref().unwrap_or("none"),
                    width.saturating_sub(LABEL),
                )),
            ),
            field("Status", status),
            Line::default(),
            Line::from(about),
        ]
    }

    /// The boxes under the app list: the app the arrow is on, the largest
    /// apps, the totals, the folders, and the steps
    fn draw_boxes(&self, frame: &mut Frame, below: Rect, app: &App) {
        let (left, right) = inner_widths(below);
        super::visual::side_by_side(
            frame,
            below,
            &format!(" {} ", app.name),
            vec![self.app_lines(app, left), self.overview_lines(left)],
            vec![
                self.total_lines(),
                self.folder_lines(),
                steps(
                    &[
                        "Press Enter on an app to see its files.",
                        "Press Space to select or clear a file.",
                        "Press Enter to review, then confirm.",
                        "It all goes to the Trash. Put Back undoes it.",
                    ],
                    right,
                ),
            ],
        );
    }

    /// The largest apps neet can remove, with a bar against the largest
    fn overview_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut sized: Vec<(&App, u64)> = self
            .apps
            .iter()
            .filter(|app| app.refused.is_none())
            .filter_map(|app| self.sizes.get(&app.path).map(|&size| (app, size)))
            .collect();
        sized.sort_by_key(|&(_, size)| Reverse(size));
        let largest = sized.first().map_or(0, |&(_, size)| size);
        let room = width.saturating_sub(11 + 10 + 2);
        let mut lines = vec![Line::from("Largest").style(super::visual::HEADING)];
        if sized.is_empty() {
            lines.push(Line::from("Measuring…"));
        }
        for (app, size) in sized.into_iter().take(10) {
            lines.push(Line::from(vec![
                format::size_span(size, format!("{:>9}  ", format::size(size))),
                size_bar(size, largest, 10),
                Span::raw("  "),
                Span::raw(format::shorten_middle(&app.name, room)),
            ]));
        }
        lines
    }

    /// How many apps there are, how many neet can remove, and their size
    fn total_lines(&self) -> Vec<Line<'static>> {
        let removable: Vec<&App> = self
            .apps
            .iter()
            .filter(|app| app.refused.is_none())
            .collect();
        let size: u64 = removable
            .iter()
            .filter_map(|app| self.sizes.get(&app.path))
            .sum();
        let blocked = self.apps.len() - removable.len();
        let mut size = vec![format::size_span(size, format::size(size)).bold()];
        if self.measuring {
            size.push(Span::raw(" so far"));
        }
        let mut lines = vec![
            Line::from("Total").style(super::visual::HEADING),
            field("Apps", Span::raw(count(self.apps.len()))),
            field("Removable", Span::raw(count(removable.len())).green()),
        ];
        if blocked > 0 {
            lines.push(field("Blocked", Span::raw(count(blocked)).yellow()));
        }
        let mut line = field("Size", Span::raw(""));
        line.spans.extend(size);
        lines.push(line);
        lines
    }

    /// The two Applications folders, in the colors the list uses
    fn folder_lines(&self) -> Vec<Line<'static>> {
        let yours = self
            .apps
            .iter()
            .filter(|app| {
                self.roots
                    .as_ref()
                    .is_some_and(|roots| app.path.starts_with(roots.home()))
            })
            .count();
        let shared = self.apps.len() - yours;
        vec![
            Line::from("Folders").style(super::visual::HEADING),
            Line::from(vec![
                Span::raw(format!("{:<16}", "/Applications")).fg(super::visual::ACCENT),
                Span::raw(format!("{}, for every user", apps_count(shared))),
            ]),
            Line::from(vec![
                Span::raw(format!("{:<16}", "~/Applications")).magenta(),
                Span::raw(format!("{}, only yours", apps_count(yours))),
            ]),
        ]
    }
}

/// The steps from an app to the Trash, numbered, wrapped to `width`
fn steps(texts: &[&str], width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("Steps").style(super::visual::HEADING)];
    for (number, text) in texts.iter().enumerate() {
        lines.extend(super::visual::hanging(
            vec![
                Span::raw(format!("{}. ", number + 1))
                    .fg(super::visual::ACCENT)
                    .bold(),
            ],
            3,
            text,
            ratatui::style::Style::default(),
            width,
        ));
    }
    lines
}

fn count(value: usize) -> String {
    format::count(u64::try_from(value).unwrap_or(u64::MAX))
}

/// `1 app` or `3 apps`
fn apps_count(value: usize) -> String {
    format!(
        "{} {}",
        count(value),
        if value == 1 { "app" } else { "apps" }
    )
}

/// `path` with the home folder as `~`, when there is one
fn display_path_or_full(roots: Option<&CleanupRoots>, path: &Path) -> String {
    match roots {
        Some(roots) => display_path(roots.home(), path),
        None => path.display().to_string(),
    }
}

/// The text widths inside the left & right boxes of
/// [`super::visual::side_by_side`] in `area`, the same when they stack
fn inner_widths(area: Rect) -> (usize, usize) {
    if area.width < super::visual::SIDE_BY_SIDE {
        let width = usize::from(area.width.saturating_sub(4));
        return (width, width);
    }
    let left = area.width * 55 / 100;
    (
        usize::from(left.saturating_sub(4)),
        usize::from((area.width - left).saturating_sub(4)),
    )
}

/// The list, only as tall as its rows, and the area below it for boxes,
/// when there is room for both
fn split_below(area: Rect, rows: usize) -> (Rect, Option<Rect>) {
    let rows = u16::try_from(rows).unwrap_or(u16::MAX);
    // Border and header above the rows, and the border below
    let table = rows.saturating_add(3);
    if area.height < table.saturating_add(BOXES) {
        return (area, None);
    }
    let [table, below] =
        Layout::vertical([Constraint::Length(table), Constraint::Fill(1)]).areas(area);
    (table, Some(below))
}

/// Keep the selected path and metadata available when secondary columns disappear.
fn draw_selected(frame: &mut Frame, area: Rect, details: Vec<Line<'static>>) -> Rect {
    if details.is_empty() {
        return area;
    }
    let height = super::visual::wrapped_rows(&details, area.width.saturating_sub(4))
        .saturating_add(2)
        .min(area.height.saturating_sub(6));
    let [table, detail] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(area);
    frame.render_widget(
        Paragraph::new(details).wrap(Wrap { trim: false }).block(
            super::visual::block()
                .title(" Selected ")
                .padding(Padding::horizontal(1)),
        ),
        detail,
    );
    table
}

/// A bar of `size` against the largest app, `width` wide, colored like
/// the size
fn size_bar(size: u64, largest: u64, width: usize) -> Span<'static> {
    let bar = Span::raw(format::bar(size, largest, width));
    if size >= format::HUGE {
        bar.red()
    } else if size >= format::HUGE / 5 {
        bar.yellow()
    } else {
        bar.green()
    }
}

impl Screen for RemoveApp {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        let block = super::visual::block().padding(Padding::horizontal(1));
        if let Some(reason) = &self.failed {
            let block = block.title(" Remove App ");
            frame.render_widget(Paragraph::new(reason.clone()).red().block(block), area);
            return;
        }
        let home = self.roots.as_ref().map(|roots| roots.home().to_path_buf());
        let removable: Vec<&App> = self
            .apps
            .iter()
            .filter(|app| app.refused.is_none())
            .collect();
        let total: u64 = removable
            .iter()
            .filter_map(|app| self.sizes.get(&app.path))
            .sum();
        let largest = self.sizes.values().copied().max().unwrap_or(0);
        let sizes =
            format::column_width("Size", self.sizes.values().map(|&size| format::size(size)))
                .max(4);
        let columns = super::visual::Columns::new(
            area.width,
            [(18, 2), (sizes, 0), (12, 0), (16, 0), (18, 1)],
            &[2, 4, 3],
            true,
        );
        let current = self.list.selected().and_then(|index| self.apps.get(index));
        let (area, below) = split_below(area, self.apps.len());
        if let (Some(below), Some(app)) = (below, current) {
            self.draw_boxes(frame, below, app);
        }
        let detail = current.filter(|_| below.is_none()).map(|app| {
            let status = app.refused.map_or_else(
                || app.bundle_id.clone().unwrap_or_default(),
                |why| format!("blocked: {}", refusal_text(why)),
            );
            let status = Span::raw(status);
            Line::from(vec![
                Span::raw(app.path.display().to_string()).fg(super::visual::ACCENT),
                Span::raw(" · "),
                if app.refused.is_some() {
                    status.yellow()
                } else {
                    status.fg(super::visual::ACCENT)
                },
            ])
        });
        let area = draw_selected(frame, area, detail.into_iter().collect());
        let rows: Vec<Row> = self
            .apps
            .iter()
            .map(|app| self.app_row(app, home.as_deref(), largest, &columns))
            .collect();
        let header = columns
            .row([
                Cell::from("Name"),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from(""),
                Cell::from("Folder"),
                Cell::from("App ID"),
            ])
            .style(super::visual::HEADING);
        let measuring = if self.measuring {
            " · measuring…"
        } else {
            ""
        };
        let summary = Line::from(vec![
            Span::raw(format!(
                " {} can be removed · ",
                apps_count(removable.len())
            )),
            format::size_span(total, format::size(total)).bold(),
            Span::raw(format!("{measuring} ")),
        ]);
        let note = self.note.clone().map_or_else(
            || {
                Line::from(if self.by_size {
                    " Largest first · s sorts by name "
                } else {
                    " By name · s sorts by size "
                })
            },
            |note| Line::from(format!(" {note} ")).yellow(),
        );
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(
                block
                    .title(" Remove App ")
                    .title_bottom(summary)
                    .title(note.right_aligned()),
            )
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.list);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        self.note = None;
        match key.code {
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.open(),
            KeyCode::Char('s') => {
                self.by_size = !self.by_size;
                self.sort();
                Action::None
            }
            code => {
                step(&mut self.list, self.apps.len(), code);
                Action::None
            }
        }
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · enter open · s sort · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between apps"),
            ("g  G", "Jump to the first or last app"),
            ("Enter  →  l", "Open the app & see its files"),
            ("s", "Sort by size or by name"),
            ("Esc", "Back to Home"),
            ("q", "Quit"),
        ]
    }
}

enum State {
    Planning {
        started: Instant,
        result: Receiver<Result<Planned, String>>,
    },
    Ready(Arc<Planned>),
    Failed(String),
}

/// Width of the folder column, such as `~/Library/Application Support`
const FOLDER_WIDTH: u16 = 30;

/// One app and the files that belong to it, each selected as SAFETY.md
/// says. `Enter` opens the same review as Clean.
pub struct AppFiles {
    name: String,
    bundle_id: String,
    state: State,
    list: TableState,
}

impl AppFiles {
    fn start(roots: CleanupRoots, app: App) -> Self {
        let (sender, result) = mpsc::channel();
        let name = app.name.clone();
        let bundle_id = app.bundle_id.clone().unwrap_or_default();
        thread::spawn(move || {
            let planned = removal::plan(&roots, &app)
                .map(|plan| Planned {
                    plan,
                    errors: Vec::new(),
                    home: roots.home().to_path_buf(),
                })
                .map_err(|refusal| format!("neet does not remove this app: {refusal}."));
            // The receiver is gone only when the screen was closed.
            let _ = sender.send(planned);
        });
        Self {
            name,
            bundle_id,
            state: State::Planning {
                started: Instant::now(),
                result,
            },
            list: TableState::default().with_selected(Some(0)),
        }
    }

    #[cfg(test)]
    fn ready(name: &str, bundle_id: &str, planned: Planned) -> Self {
        Self {
            name: name.to_string(),
            bundle_id: bundle_id.to_string(),
            state: State::Ready(Arc::new(planned)),
            list: TableState::default().with_selected(Some(0)),
        }
    }

    fn poll(&mut self) {
        if let State::Planning { result, .. } = &self.state
            && let Ok(outcome) = result.try_recv()
        {
            self.state = match outcome {
                Ok(planned) => State::Ready(Arc::new(planned)),
                Err(reason) => State::Failed(reason),
            };
        }
    }

    fn toggle(&mut self) {
        let index = self.list.selected().unwrap_or(0);
        if let State::Ready(planned) = &mut self.state
            && let Some(planned) = Arc::get_mut(planned)
            && let Some(rule) = planned.plan.rules.get_mut(index)
            && !rule.items.is_empty()
        {
            rule.selected = !rule.selected;
        }
    }
}

impl AppFiles {
    /// The boxes under the file list: the file the arrow is on, every
    /// file's size, the totals, the steps, and the disk after
    fn draw_boxes(
        frame: &mut Frame,
        below: Rect,
        planned: &Planned,
        current: usize,
        bundle_id: &str,
        context: &Context,
    ) {
        let plan = &planned.plan;
        let (left, right) = inner_widths(below);
        let (title, file) = file_lines(planned, current, left);
        super::visual::side_by_side(
            frame,
            below,
            &title,
            vec![file, files_overview(planned, left)],
            vec![
                files_total(planned, bundle_id),
                steps(
                    &[
                        "Press Space to select or clear a file.",
                        "Press Enter to review, then confirm.",
                        "It all goes to the Trash. Put Back undoes it.",
                    ],
                    right,
                ),
                super::visual::after_cleanup(
                    context.disk.as_ref(),
                    plan.selected_size(),
                    "Selected",
                    u16::try_from(right).unwrap_or(u16::MAX),
                ),
            ],
        );
    }
}

/// Why a file starts selected or not, from its mark, for the line under the
/// list
fn explain(rule: &RulePlan) -> &'static str {
    let marks = [
        Mark::MayBeYourData,
        Mark::Settings,
        Mark::SharedWithOtherApps,
        Mark::StartsOnItsOwn,
        Mark::MatchedByName,
    ];
    let mark = marks
        .into_iter()
        .find(|mark| mark.label() == rule.rule.description)
        .unwrap_or(Mark::None);
    match mark {
        Mark::None if rule.rule.name == "The app" => "The app itself.",
        Mark::None => "The app makes this again if it needs it. Selected for you.",
        Mark::MayBeYourData => "May hold your own work. Check it before you select it.",
        Mark::Settings => "The app's settings. Keep them if you may install it again.",
        Mark::SharedWithOtherApps => "Other apps from the same maker may use it too.",
        Mark::StartsOnItsOwn => "Starts a helper on its own, such as when you log in.",
        Mark::MatchedByName => "Found by name only. Make sure it belongs to this app.",
    }
}

/// What kind of file the rule found: the app, one the app makes again in
/// green, or one to check first in yellow
fn kind(rule: &RulePlan) -> Span<'static> {
    if !rule.rule.description.is_empty() {
        Span::raw(rule.rule.description.clone()).yellow()
    } else if rule.rule.name == "The app" {
        Span::raw("the app")
    } else {
        Span::raw("made again").green()
    }
}

/// The file the arrow is on, as a title and its lines: where it is, its
/// size, what kind it is, and whether it is selected
fn file_lines(planned: &Planned, index: usize, width: usize) -> (String, Vec<Line<'static>>) {
    let Some(rule) = planned.plan.rules.get(index) else {
        return (String::new(), Vec::new());
    };
    let (path, size) = match (rule.items.first(), rule.skipped.first()) {
        (Some(item), _) => (
            item.path.path(),
            format::size_span(item.size, format::size(item.size)),
        ),
        (None, Some(skipped)) => (
            skipped.path.as_path(),
            Span::raw(format!("skipped, {}", skip_reason(&skipped.reason, None))).yellow(),
        ),
        (None, None) => return (String::new(), Vec::new()),
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = path
        .parent()
        .map(|folder| display_path(&planned.home, folder))
        .unwrap_or_default();
    let selected = if rule.items.is_empty() {
        Span::raw("can't be")
    } else if rule.selected {
        Span::raw("yes").green()
    } else {
        Span::raw("no")
    };
    let mut lines = vec![
        field(
            "Folder",
            Span::raw(format::shorten_path(&folder, width.saturating_sub(LABEL)))
                .fg(super::visual::ACCENT),
        ),
        field("Size", size),
        field("Kind", kind(rule)),
        field("Selected", selected),
        Line::default(),
    ];
    lines.extend(super::visual::hanging(
        Vec::new(),
        0,
        explain(rule),
        ratatui::style::Style::default(),
        width,
    ));
    (
        format!(
            " {} ",
            format::shorten_middle(&name, width.saturating_sub(4))
        ),
        lines,
    )
}

/// Every file found, largest first, with a bar against the largest, green
/// when selected
fn files_overview(planned: &Planned, width: usize) -> Vec<Line<'static>> {
    let mut found: Vec<(&RulePlan, u64)> = planned
        .plan
        .rules
        .iter()
        .filter_map(|rule| rule.items.first().map(|item| (rule, item.size)))
        .collect();
    found.sort_by_key(|&(_, size)| Reverse(size));
    let largest = found.first().map_or(0, |&(_, size)| size);
    let room = width.saturating_sub(11 + 10 + 2);
    let mut lines = vec![Line::from("Overview").style(super::visual::HEADING)];
    for (rule, size) in found {
        let name = rule
            .items
            .first()
            .and_then(|item| item.path.path().file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bar = Span::raw(format::bar(size, largest, 10));
        lines.push(Line::from(vec![
            format::size_span(size, format!("{:>9}  ", format::size(size))),
            if rule.selected {
                bar.green()
            } else {
                bar.fg(super::visual::ACCENT)
            },
            Span::raw("  "),
            Span::raw(format::shorten_middle(&name, room)),
        ]));
    }
    lines
}

/// Everything found and selected for the app
fn files_total(planned: &Planned, bundle_id: &str) -> Vec<Line<'static>> {
    let plan = &planned.plan;
    let found: Vec<_> = plan.rules.iter().flat_map(|rule| &rule.items).collect();
    let size: u64 = found.iter().map(|item| item.size).sum();
    let selected = Span::raw(format!(
        "{} · {}",
        format::size(plan.selected_size()),
        super::clean::items(plan.selected_count())
    ));
    vec![
        Line::from("Total").style(super::visual::HEADING),
        field(
            "Found",
            Span::raw(format!(
                "{} · {}",
                format::size(size),
                super::clean::items(found.len())
            )),
        ),
        field(
            "Selected",
            if plan.selected_count() == 0 {
                selected
            } else {
                selected.green().bold()
            },
        ),
        field("App ID", Span::raw(bundle_id.to_string())),
    ]
}

/// One file as a table row. `current` is the row the arrow is on, whose name
/// turns bold, so the checkbox and note keep their own colors.
fn file_row(
    rule: &RulePlan,
    home: &Path,
    current: bool,
    columns: &super::visual::Columns<5>,
) -> Row<'static> {
    let (path, size, note) = match (rule.items.first(), rule.skipped.first()) {
        (Some(item), _) => (
            display_path(home, item.path.path()),
            format::size_span(item.size, format::size(item.size)),
            kind(rule),
        ),
        (None, Some(skipped)) => (
            display_path(home, &skipped.path),
            Span::raw(""),
            Span::raw(format!("skipped, {}", skip_reason(&skipped.reason, None))).yellow(),
        ),
        (None, None) => (String::new(), Span::raw(""), Span::raw("")),
    };
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
    let name = Span::raw(format::shorten_middle(name, columns.width(2)));
    let name = if current { name.bold() } else { name };
    columns.row([
        Cell::from(checkbox(rule.selected, !rule.items.is_empty())),
        Cell::from(Line::from(size).right_aligned()),
        Cell::from(name),
        Cell::from(
            Span::raw(format::shorten_path(folder, columns.width(3))).fg(super::visual::ACCENT),
        ),
        Cell::from(note),
    ])
}

impl Screen for AppFiles {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        self.poll();
        let title = format!(
            " {} ",
            format::shorten_middle(&self.name, usize::from(area.width.saturating_sub(30)))
        );
        let block = super::visual::block()
            .title(title)
            .padding(Padding::horizontal(1));
        let planned = match &self.state {
            State::Planning { started, .. } => {
                Loading {
                    title: &self.name,
                    doing: "Looking for the app's files",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Nothing is changed while neet looks.",
                }
                .draw(frame, area);
                return;
            }
            State::Failed(reason) => {
                frame.render_widget(
                    Paragraph::new(reason.clone())
                        .red()
                        .wrap(Wrap { trim: true })
                        .block(block),
                    area,
                );
                return;
            }
            State::Ready(planned) => Arc::clone(planned),
        };
        let plan = &planned.plan;
        let selected = Span::raw(format!(
            " Selected · {} · {} ",
            super::clean::items(plan.selected_count()),
            format::size(plan.selected_size())
        ));
        let block = block
            .title_bottom(if plan.selected_count() == 0 {
                selected
            } else {
                selected.green().bold()
            })
            .title(Line::from(" Goes to the Trash ").right_aligned());
        let current = self.list.selected().unwrap_or(0);
        let (table_area, below) = split_below(area, plan.rules.len());
        let list_area = if let Some(below) = below {
            Self::draw_boxes(frame, below, &planned, current, &self.bundle_id, context);
            table_area
        } else {
            let mut details = vec![Line::from(self.bundle_id.clone()).fg(super::visual::ACCENT)];
            if let Some(rule) = plan.rules.get(current) {
                if let Some(path) = rule
                    .items
                    .first()
                    .map(|item| item.path.path())
                    .or_else(|| rule.skipped.first().map(|item| item.path.as_path()))
                {
                    details.push(
                        Line::from(display_path(&planned.home, path)).fg(super::visual::ACCENT),
                    );
                }
                details.push(Line::from(explain(rule)));
            }
            draw_selected(frame, area, details)
        };
        let sizes = format::column_width(
            "Size",
            plan.rules.iter().map(|rule| format::size(rule.size())),
        );
        let columns = super::visual::Columns::new(
            list_area.width,
            [(3, 0), (sizes, 0), (16, 1), (FOLDER_WIDTH, 0), (18, 0)],
            &[3],
            true,
        );
        let current = self.list.selected().unwrap_or(0);
        let rows: Vec<Row> = plan
            .rules
            .iter()
            .enumerate()
            .map(|(index, rule)| file_row(rule, &planned.home, index == current, &columns))
            .collect();
        let header = columns
            .row([
                super::visual::check_header(),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from("Name"),
                Cell::from("Folder"),
                Cell::from("Kind"),
            ])
            .style(super::visual::HEADING);
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, list_area, &mut self.list);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        let State::Ready(planned) = &self.state else {
            return Action::None;
        };
        let len = planned.plan.rules.len();
        match key.code {
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter => {
                if planned.plan.selected_count() > 0 {
                    return Action::Open(Box::new(Review::new(Arc::clone(planned))));
                }
            }
            code => step(&mut self.list, len, code),
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · space select · enter review · esc back · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between files"),
            ("Space", "Select or clear a file"),
            ("Enter", "Review the selected files"),
            ("Esc", "Back to the app list"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use neet_core::removal::Refusal;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use std::fs;
    use tempfile::{TempDir, tempdir};

    fn render(screen: &mut dyn Screen) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        terminal
            .draw(|frame| screen.draw(frame, frame.area(), &context))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    fn press(screen: &mut dyn Screen, code: KeyCode) -> Action {
        let scan = ScanStatus::Failed(String::new());
        screen.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            &Context {
                scan: &scan,
                disk: None,
                cleanable: None,
                plan: None,
            },
        )
    }

    fn setup() -> (TempDir, TempDir, CleanupRoots, Vec<App>) {
        let home = tempdir().expect("home should be created");
        let shared = tempdir().expect("applications should be created");
        fs::create_dir_all(shared.path().join("Example.app/Contents"))
            .expect("app should be created");
        fs::create_dir_all(home.path().join("Library/Caches/com.example.app"))
            .expect("cache should be created");
        fs::create_dir_all(
            home.path()
                .join("Library/Application Support/com.example.app"),
        )
        .expect("data should be created");
        let roots = CleanupRoots::with_applications(home.path(), shared.path())
            .expect("roots should be made");
        let apps = vec![
            App {
                path: shared.path().join("Example.app"),
                name: "Example".to_string(),
                bundle_id: Some("com.example.app".to_string()),
                bundle_name: None,
                refused: None,
            },
            App {
                path: shared.path().join("Notes.app"),
                name: "Notes".to_string(),
                bundle_id: Some("com.apple.Notes".to_string()),
                bundle_name: None,
                refused: Some(Refusal::AppleApp),
            },
        ];
        (home, shared, roots, apps)
    }

    #[test]
    fn lists_apps_and_labels_refused_ones() {
        let (_home, _shared, roots, apps) = setup();
        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);

        let text = render(&mut screen);

        assert!(text.contains("Example"));
        assert!(text.contains("com.example.app"));
        assert!(text.contains("part of macOS"));
    }

    #[test]
    fn opens_with_sizes_measured_before() {
        let (_home, _shared, roots, apps) = setup();
        let path = apps[0].path.clone();
        for app in &apps {
            sizes().insert(app.path.clone(), Measured::Failed);
        }
        sizes().insert(path.clone(), Measured::Size(3_000_000_000));
        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);
        screen.measuring = true;
        screen.poll();
        assert_eq!(screen.sizes.get(&path), Some(&3_000_000_000));
        assert!(!screen.measuring, "nothing is left to measure");
        let text = render(&mut screen);
        assert!(text.contains("3.0 GB"), "{text}");
        assert!(!text.contains("measuring"), "{text}");
    }

    #[test]
    fn measures_each_app_once_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Once.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::write(app.join("Contents/x"), vec![0u8; 10_000]).unwrap();
        measure(vec![app.clone()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while matches!(sizes().get(&app), Some(Measured::Waiting)) {
            assert!(
                std::time::Instant::now() < deadline,
                "measuring took too long"
            );
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(sizes().get(&app), Some(Measured::Size(size)) if *size >= 10_000));
        // Asked again, it is kept, not measured again.
        sizes().insert(app.clone(), Measured::Size(1));
        measure(vec![app.clone()]);
        assert!(matches!(sizes().get(&app), Some(Measured::Size(1))));
    }

    #[test]
    fn shows_sizes_and_sorts_by_them() {
        let (_home, _shared, roots, mut apps) = setup();
        apps.push(App {
            path: apps[0].path.with_file_name("Big.app"),
            name: "Big".to_string(),
            bundle_id: Some("com.example.big".to_string()),
            bundle_name: None,
            refused: None,
        });
        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);
        // By name, Big comes first, though Example is larger.
        screen.sizes.insert(screen.apps[0].path.clone(), 2_000_000);
        screen
            .sizes
            .insert(screen.apps[1].path.clone(), 6_000_000_000);

        let text = render(&mut screen);
        assert!(text.contains("6.0 GB"));
        assert!(text.contains("2.0 MB"));
        assert!(text.contains("can be removed · 6.0 GB"));
        assert_eq!(screen.apps[0].name, "Big");

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Char('s'));
        // Still on Example, now first by size
        assert_eq!(screen.apps[0].name, "Example");
        assert_eq!(screen.list.selected(), Some(0));
    }

    #[test]
    fn refuses_to_open_an_apple_app_or_an_open_app() {
        let (_home, _shared, roots, apps) = setup();
        let mut screen = RemoveApp::from_apps(roots.clone(), apps.clone(), |_| true);

        assert!(matches!(press(&mut screen, KeyCode::Enter), Action::None));
        assert!(render(&mut screen).contains("Quit Example first"));

        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);
        press(&mut screen, KeyCode::Down);
        assert!(matches!(press(&mut screen, KeyCode::Enter), Action::None));
        assert!(render(&mut screen).contains("does not remove Notes"));
    }

    #[test]
    fn lists_files_with_data_unselected_and_leads_to_the_review() {
        let (_home, _shared, roots, apps) = setup();
        let plan = removal::plan(&roots, &apps[0]).expect("app should be planned");
        let planned = Planned {
            plan,
            errors: Vec::new(),
            home: roots.home().to_path_buf(),
        };
        let mut files = AppFiles::ready("Example", "com.example.app", planned);

        let text = render(&mut files);
        assert!(text.contains("[✓]"));
        assert!(text.contains("com.example.app"));
        assert!(text.contains("~/Library/Caches"));
        assert!(text.contains("[ ]"));
        assert!(text.contains("may be your data"));
        assert!(text.contains("Selected · 2 items"));

        press(&mut files, KeyCode::Down);
        press(&mut files, KeyCode::Down);
        press(&mut files, KeyCode::Char(' '));
        assert!(render(&mut files).contains("Selected · 3 items"));

        assert!(matches!(press(&mut files, KeyCode::Enter), Action::Open(_)));
    }

    #[test]
    fn opening_an_app_finds_its_files_in_the_background() {
        let (_home, _shared, roots, apps) = setup();
        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);

        let Action::Open(mut files) = press(&mut screen, KeyCode::Enter) else {
            panic!("the app should open");
        };
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let text = render(files.as_mut());
            if text.contains("Selected ·") {
                break;
            }
            assert!(Instant::now() < deadline, "planning should finish");
            thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let scan = crate::ui::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let (_home, _shared, roots, apps) = setup();
        let plan = removal::plan(&roots, &apps[0]).unwrap();
        let mut files = AppFiles::ready(
            "Example",
            "com.example.app",
            Planned {
                plan,
                errors: Vec::new(),
                home: roots.home().to_path_buf(),
            },
        );
        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);
        screen
            .sizes
            .insert(screen.apps[0].path.clone(), 6_000_000_000);
        for (width, height) in view::SIZES {
            let buffer = view::render("apps", &mut screen, &context, width, height);
            view::aligned(&buffer, "Size", "6.0 GB");
            let buffer = view::render("app-files", &mut files, &context, width, height);
            assert!(view::text(&buffer).contains("Example.app"));
            assert!(view::text(&buffer).contains("may be your data"));
        }
    }
}
