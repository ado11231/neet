use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use neet_core::safety::CleanupRoots;
use neet_core::tools::{self, Device, Docker, DockerUsage, DockerVolume, Runtime};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::checkbox;
use super::format;
use super::loading::Loading;

/// Runs `work` on its own thread, for the answer to arrive later
fn spawn<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (sender, answer) = mpsc::channel();
    thread::spawn(move || {
        // The receiver is gone only when the screen was closed.
        let _ = sender.send(work());
    });
    answer
}

/// Where a tool screen is: looking, showing what it found, asking, running
/// the tool, or done
enum Stage<T, R> {
    Looking(Instant, Receiver<Result<T, String>>),
    Failed(String),
    Ready(T),
    Asking(T),
    Working(Instant, Receiver<R>),
    Done(R),
}

impl<T, R> Stage<T, R> {
    fn look(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        Self::Looking(Instant::now(), spawn(work))
    }

    fn poll(&mut self) {
        let next = match self {
            Self::Looking(_, answer) => match answer.try_recv() {
                Ok(Ok(found)) => Self::Ready(found),
                Ok(Err(reason)) => Self::Failed(reason),
                Err(_) => return,
            },
            Self::Working(_, answer) => match answer.try_recv() {
                Ok(result) => Self::Done(result),
                Err(_) => return,
            },
            _ => return,
        };
        *self = next;
    }

    fn found(&self) -> Option<&T> {
        match self {
            Self::Ready(found) | Self::Asking(found) => Some(found),
            _ => None,
        }
    }

    /// From the question back to what was found
    fn back_to_ready(&mut self) {
        if let Self::Asking(_) = self
            && let Self::Asking(found) = std::mem::replace(self, Self::Failed(String::new()))
        {
            *self = Self::Ready(found);
        }
    }

    /// From what was found to the question
    fn ask(&mut self) {
        if let Self::Ready(_) = self
            && let Self::Ready(found) = std::mem::replace(self, Self::Failed(String::new()))
        {
            *self = Self::Asking(found);
        }
    }

    /// Asking or running the tool, when keys like `q` must not act
    fn is_busy(&self) -> bool {
        matches!(self, Self::Asking(_) | Self::Working(..))
    }
}

/// The widest a message box gets
const MESSAGE_WIDTH: u16 = 76;

/// How many rows `lines` take when wrapped at spaces to `width`, as the
/// paragraph wraps them, so a box is never too short for its last line
fn rows(lines: &[Line], width: u16) -> u16 {
    super::visual::wrapped_rows(lines, width)
}

/// A box in the middle of `area`, just big enough for `lines`, so short
/// news does not sit in a big empty box
pub(super) fn message(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    color: Color,
) {
    let widest = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(widest + 6)
        .unwrap_or(u16::MAX)
        .clamp(44, MESSAGE_WIDTH)
        .min(area.width);
    let height = (rows(&lines, width.saturating_sub(6)) + 4).min(area.height);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            super::visual::block()
                .border_style(Style::new().fg(color))
                .title(Line::from(format!(" {title} ")).fg(color).bold())
                .padding(Padding::new(2, 2, 1, 1)),
        ),
        area,
    );
}

/// The question before a tool acts, with `y` to go ahead
fn ask(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    mut lines: Vec<Line<'static>>,
    yes: &'static str,
    color: Color,
) {
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::raw("y").bold().fg(color),
        Span::raw(format!(" {yes}    ")),
        Span::raw("n").bold(),
        Span::raw(" or "),
        Span::raw("Esc").bold(),
        Span::raw(" go back"),
    ]));
    message(frame, area, title, lines, color);
}

/// A key to press, as it shows in text
fn key(name: &'static str) -> Span<'static> {
    Span::raw(name).fg(super::visual::ACCENT).bold()
}

/// A numbered step
fn step(number: usize, text: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![
        Span::raw(format!("{number}  "))
            .fg(super::visual::ACCENT)
            .bold(),
    ];
    spans.extend(text);
    Line::from(spans)
}

/// A labeled line, the label in its own color, so lines line up
fn labeled(label: &'static str, color: Color, text: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{label:<12}")).fg(color).bold()];
    spans.extend(text);
    Line::from(spans)
}

fn tick(text: String) -> Line<'static> {
    Line::from(vec![Span::raw("✓ ").green().bold(), Span::raw(text)])
}

fn cross(text: String) -> Line<'static> {
    Line::from(vec![Span::raw("✗ ").red().bold(), Span::raw(text).red()])
}

fn back_home() -> Line<'static> {
    Line::from(vec![
        Span::raw("Press "),
        key("Enter"),
        Span::raw(" to go back to Home."),
    ])
}

/// A box of text sized to fit, at the top of `area`. Returns the rest.
fn about_box(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) -> Rect {
    let height = (rows(&lines, area.width.saturating_sub(4)) + 2).min(area.height);
    let [about, rest] =
        Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(super::visual::block().padding(Padding::horizontal(1))),
        about,
    );
    rest
}

/// The date part of a time `simctl` gives, such as `2026-09-17`
fn day(time: &str) -> &str {
    time.split('T').next().unwrap_or(time)
}

/// `count` of `noun`, adding an s when it is not one
fn counted(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The simulator runtimes and devices `simctl` knows
pub struct SimulatorsFound {
    runtimes: Vec<Runtime>,
    devices: Vec<Device>,
}

impl SimulatorsFound {
    /// Simulators whose runtime is gone, which can never start again
    fn stranded(&self) -> Vec<&Device> {
        self.devices
            .iter()
            .filter(|device| !device.available)
            .collect()
    }

    /// The simulators that run on `runtime`, removed with it
    fn on(&self, runtime: &Runtime) -> Vec<&Device> {
        self.devices
            .iter()
            .filter(|device| {
                device.available
                    && !runtime.runtime_identifier.is_empty()
                    && device.runtime == runtime.runtime_identifier
            })
            .collect()
    }

    /// One row per runtime, and one for stranded simulators if there are any
    fn row_count(&self) -> usize {
        self.runtimes.len() + usize::from(!self.stranded().is_empty())
    }
}

fn size_of(devices: &[&Device]) -> u64 {
    devices.iter().map(|device| device.size).sum()
}

/// One runtime as a table row, with the simulators on it
fn runtime_row(
    runtime: &Runtime,
    on: &[&Device],
    chosen: bool,
    columns: &super::visual::Columns<6>,
) -> Row<'static> {
    columns.row([
        Cell::from(checkbox(chosen, runtime.deletable)),
        Cell::from(runtime.name.clone()),
        Cell::from(runtime.build.clone()),
        Cell::from(
            Line::from(format::size_span(runtime.size, format::size(runtime.size))).right_aligned(),
        ),
        Cell::from(
            runtime
                .last_used
                .as_deref()
                .map_or("never", day)
                .to_string(),
        ),
        Cell::from(if runtime.deletable {
            Span::raw(format!(
                "{}, {}",
                counted(on.len(), "simulator"),
                format::size(size_of(on))
            ))
        } else {
            Span::raw("Removal blocked").yellow()
        }),
    ])
}

/// What removing did
pub struct Removal {
    /// Each runtime's name and size, and why it failed if it did
    runtimes: Vec<(String, u64, Result<(), String>)>,
    devices: usize,
    devices_size: u64,
    /// Why simulators were not removed
    device_errors: Vec<String>,
}

/// Every simulator runtime, and simulators left without one, to delete with
/// `xcrun simctl` after a red question. Nothing starts selected.
pub struct Simulators {
    stage: Stage<SimulatorsFound, Removal>,
    chosen: Vec<bool>,
    list: TableState,
}

impl Simulators {
    pub fn new() -> Self {
        Self::with(Stage::look(|| {
            let runtimes = tools::runtimes()
                .map_err(|error| format!("simctl could not list runtimes: {error}"))?;
            let devices = tools::devices()
                .map_err(|error| format!("simctl could not list simulators: {error}"))?;
            Ok(SimulatorsFound { runtimes, devices })
        }))
    }

    fn with(stage: Stage<SimulatorsFound, Removal>) -> Self {
        Self {
            stage,
            chosen: Vec::new(),
            list: TableState::default().with_selected(Some(0)),
        }
    }

    fn picked_runtimes(&self) -> Vec<&Runtime> {
        let Some(found) = self.stage.found() else {
            return Vec::new();
        };
        found
            .runtimes
            .iter()
            .zip(&self.chosen)
            .filter(|(_, chosen)| **chosen)
            .map(|(runtime, _)| runtime)
            .collect()
    }

    fn stranded_picked(&self) -> bool {
        self.stage.found().is_some_and(|found| {
            !found.stranded().is_empty() && self.chosen.get(found.runtimes.len()) == Some(&true)
        })
    }

    /// Everything that goes, in bytes
    fn picked_size(&self) -> u64 {
        let Some(found) = self.stage.found() else {
            return 0;
        };
        let runtimes: u64 = self
            .picked_runtimes()
            .iter()
            .map(|runtime| runtime.size + size_of(&found.on(runtime)))
            .sum();
        let stranded = if self.stranded_picked() {
            size_of(&found.stranded())
        } else {
            0
        };
        runtimes + stranded
    }

    fn nothing_picked(&self) -> bool {
        self.picked_runtimes().is_empty() && !self.stranded_picked()
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let Some(found) = self.stage.found() else {
            return;
        };
        let sizes = format::column_width(
            "Size",
            found
                .runtimes
                .iter()
                .map(|runtime| format::size(runtime.size)),
        )
        .max(9);
        let columns = super::visual::Columns::new(
            area.width,
            [(3, 0), (16, 1), (8, 0), (sizes, 0), (11, 0), (18, 1)],
            &[2, 4],
            true,
        );
        let stranded = found.stranded();
        let mut rows: Vec<Row> = found
            .runtimes
            .iter()
            .zip(&self.chosen)
            .map(|(runtime, &chosen)| runtime_row(runtime, &found.on(runtime), chosen, &columns))
            .collect();
        if !stranded.is_empty() {
            let size = size_of(&stranded);
            rows.push(columns.row([
                Cell::from(checkbox(
                    self.chosen.get(found.runtimes.len()) == Some(&true),
                    true,
                )),
                Cell::from(Span::raw("No runtime").yellow()),
                Cell::from(""),
                Cell::from(Line::from(format::size_span(size, format::size(size))).right_aligned()),
                Cell::from(""),
                Cell::from(format!(
                    "{} that cannot start",
                    counted(stranded.len(), "simulator")
                )),
            ]));
        }
        let header = columns
            .row([
                Cell::from(""),
                Cell::from("Runtime"),
                Cell::from("Build"),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from("Last used"),
                Cell::from("Simulators on it"),
            ])
            .style(super::visual::HEADING)
            .bottom_margin(1);
        let total = found
            .runtimes
            .iter()
            .map(|runtime| runtime.size)
            .sum::<u64>()
            + size_of(&found.devices.iter().collect::<Vec<_>>());
        let picked = self.picked_size();
        let selected = Span::raw(format!(" Selected: {} ", format::size(picked)));
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(
                super::visual::block()
                    .title(format!(" Simulators · {} ", format::size(total)))
                    .title_bottom(if picked == 0 {
                        selected
                    } else {
                        selected.red().bold()
                    })
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.list);
    }

    fn about(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            labeled(
                "Runtime",
                super::visual::ACCENT,
                vec![Span::raw(
                    "lets Xcode run simulators of one iOS, watchOS, tvOS, or visionOS version.",
                )],
            ),
            labeled(
                "Removes",
                Color::Red,
                vec![Span::raw(
                    "the runtimes you select and every simulator on them, apps and data included.",
                )],
            ),
        ];
        if let Some(runtime) = self
            .stage
            .found()
            .and_then(|found| found.runtimes.get(self.list.selected().unwrap_or(0)))
        {
            lines.push(
                Line::from(format!(
                    "{} · Build {} · Last used {}",
                    runtime.name,
                    runtime.build,
                    runtime.last_used.as_deref().map_or("never", day)
                ))
                .fg(super::visual::ACCENT),
            );
        }
        if self
            .stage
            .found()
            .is_some_and(|found| !found.stranded().is_empty())
        {
            lines.push(labeled(
                "No runtime",
                Color::Yellow,
                vec![Span::raw(
                    "simulators whose runtime is gone. They can never start again.",
                )],
            ));
        }
        lines.push(labeled(
            "Undo",
            Color::Green,
            vec![Span::raw(
                "Xcode downloads a runtime again in Settings, Components.",
            )],
        ));
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::raw("Permanently: ").red().bold(),
            Span::raw("none of it goes to the Trash."),
        ]));
        lines
    }

    fn question(&self) -> Vec<Line<'static>> {
        let Some(found) = self.stage.found() else {
            return Vec::new();
        };
        let mut lines = vec![Line::from("Permanently remove:").bold(), Line::default()];
        for runtime in self.picked_runtimes() {
            let on = found.on(runtime);
            lines.push(Line::from(vec![
                Span::raw("•  ").red(),
                Span::raw(format!("{} runtime", runtime.name)).bold(),
                Span::raw(format!(", {}", format::size(runtime.size))),
            ]));
            if !on.is_empty() {
                lines.push(Line::from(format!(
                    "   and {} on it, {}",
                    counted(on.len(), "simulator"),
                    format::size(size_of(&on))
                )));
            }
        }
        if self.stranded_picked() {
            let stranded = found.stranded();
            lines.push(Line::from(vec![
                Span::raw("•  ").red(),
                Span::raw(format!(
                    "{} with no runtime",
                    counted(stranded.len(), "simulator")
                ))
                .bold(),
                Span::raw(format!(", {}", format::size(size_of(&stranded)))),
            ]));
        }
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::raw("In all  "),
            Span::raw(format::size(self.picked_size())).red().bold(),
            Span::raw(". None of it goes to the Trash."),
        ]));
        lines
    }

    fn start_removing(&mut self) {
        let Some(found) = self.stage.found() else {
            return;
        };
        let runtimes: Vec<(Runtime, Vec<Device>)> = self
            .picked_runtimes()
            .into_iter()
            .map(|runtime| {
                let on = found.on(runtime).into_iter().cloned().collect();
                (runtime.clone(), on)
            })
            .collect();
        let stranded: Vec<Device> = if self.stranded_picked() {
            found.stranded().into_iter().cloned().collect()
        } else {
            Vec::new()
        };
        self.stage = Stage::Working(
            Instant::now(),
            spawn(move || remove_simulators(runtimes, stranded)),
        );
    }

    fn done_lines(removal: &Removal) -> (Vec<Line<'static>>, Color) {
        let freed = removal
            .runtimes
            .iter()
            .filter(|(_, _, result)| result.is_ok())
            .map(|(_, size, _)| size)
            .sum::<u64>()
            + removal.devices_size;
        let failed = removal
            .runtimes
            .iter()
            .any(|(_, _, result)| result.is_err())
            || !removal.device_errors.is_empty();
        let mut lines = vec![if freed > 0 {
            Line::from(vec![
                Span::raw("✓ Removed ").green().bold(),
                Span::raw(format::size(freed)).green().bold(),
            ])
        } else {
            Line::from("Nothing was removed.").yellow().bold()
        }];
        lines.push(Line::default());
        for (name, _, result) in &removal.runtimes {
            lines.push(match result {
                Ok(()) => tick(format!("{name} runtime")),
                Err(reason) => cross(format!("{name} runtime was not removed: {reason}")),
            });
        }
        if removal.devices > 0 {
            lines.push(tick(counted(removal.devices, "simulator")));
        }
        if let Some(reason) = removal.device_errors.first() {
            lines.push(cross(format!(
                "{} not removed: {reason}",
                counted(removal.device_errors.len(), "simulator")
            )));
        }
        lines.push(Line::default());
        lines.push(Line::from(
            "Xcode downloads a runtime again in Settings, Components.",
        ));
        lines.push(Line::default());
        lines.push(back_home());
        (lines, if failed { Color::Yellow } else { Color::Green })
    }
}

/// Removes each runtime, then the simulators on the ones that went, and the
/// stranded simulators picked
fn remove_simulators(runtimes: Vec<(Runtime, Vec<Device>)>, stranded: Vec<Device>) -> Removal {
    let mut removal = Removal {
        runtimes: Vec::new(),
        devices: 0,
        devices_size: 0,
        device_errors: Vec::new(),
    };
    let mut devices = stranded;
    for (runtime, on) in runtimes {
        let result = tools::delete_runtime(&runtime.identifier).map_err(|error| error.to_string());
        // Simulators on a runtime that stays still work.
        if result.is_ok() {
            devices.extend(on);
        }
        removal.runtimes.push((runtime.name, runtime.size, result));
    }
    for device in devices {
        match tools::delete_device(&device.udid) {
            Ok(()) => {
                removal.devices += 1;
                removal.devices_size += device.size;
            }
            Err(error) => removal.device_errors.push(error.to_string()),
        }
    }
    removal
}

impl Screen for Simulators {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.stage.poll();
        let title = "Simulators";
        match &self.stage {
            Stage::Looking(started, _) => {
                return Loading {
                    title,
                    doing: "Loading runtimes and simulators",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Read-only scan.",
                }
                .draw(frame, area);
            }
            Stage::Failed(reason) => {
                return message(frame, area, title, vec![cross(reason.clone())], Color::Red);
            }
            Stage::Working(started, _) => {
                return Loading {
                    title,
                    doing: "Removing with xcrun simctl",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Each runtime is unmounted and deleted, then its simulators. This can take a minute.",
                }
                .draw(frame, area);
            }
            Stage::Done(removal) => {
                let (lines, color) = Self::done_lines(removal);
                return message(frame, area, title, lines, color);
            }
            Stage::Ready(found) | Stage::Asking(found) if found.row_count() == 0 => {
                return message(
                    frame,
                    area,
                    title,
                    vec![
                        tick("Nothing to remove".to_string()).bold(),
                        Line::default(),
                        Line::from("There are no simulator runtimes or simulators on this Mac."),
                    ],
                    Color::Green,
                );
            }
            Stage::Ready(found) | Stage::Asking(found) => {
                self.chosen.resize(found.row_count(), false);
            }
        }
        let row_count = self.stage.found().map_or(0, SimulatorsFound::row_count);
        let list_height = u16::try_from(row_count + 4).unwrap_or(u16::MAX);
        let [list, rest] =
            Layout::vertical([Constraint::Length(list_height), Constraint::Fill(1)]).areas(area);
        self.draw_list(frame, list);
        about_box(frame, rest, self.about());
        if matches!(self.stage, Stage::Asking(_)) {
            ask(
                frame,
                area,
                "Remove permanently",
                self.question(),
                "remove permanently",
                Color::Red,
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match &self.stage {
            Stage::Asking(_) => {
                match key.code {
                    KeyCode::Char('y') => self.start_removing(),
                    KeyCode::Char('n') => self.stage.back_to_ready(),
                    _ => {}
                }
                return Action::None;
            }
            Stage::Done(_) => {
                return if key.code == KeyCode::Enter {
                    Action::Home
                } else {
                    Action::None
                };
            }
            Stage::Ready(_) => {}
            _ => return Action::None,
        }
        let Some(found) = self.stage.found() else {
            return Action::None;
        };
        let index = self.list.selected().unwrap_or(0);
        let last = found.row_count().saturating_sub(1);
        let selectable = found
            .runtimes
            .get(index)
            .map_or(index < found.row_count(), |runtime| runtime.deletable);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(index.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => self.list.select(Some((index + 1).min(last))),
            KeyCode::Char(' ') if selectable => {
                self.chosen.resize(found.row_count(), false);
                if let Some(chosen) = self.chosen.get_mut(index) {
                    *chosen = !*chosen;
                }
            }
            KeyCode::Enter if !self.nothing_picked() => self.stage.ask(),
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        match self.stage {
            Stage::Asking(_) => {
                self.stage.back_to_ready();
                Action::None
            }
            Stage::Working(..) => Action::None,
            Stage::Done(_) => Action::Home,
            _ => Action::Back,
        }
    }

    fn hints(&self) -> &'static str {
        match self.stage {
            Stage::Asking(_) => "y remove permanently · n or esc back",
            Stage::Working(..) => "removing, please wait",
            Stage::Done(_) => "enter or esc home",
            _ => "↑↓ move · space select · enter remove · esc back · ? help",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("Space", "Select or clear a row"),
            ("Enter", "Ask before removing what is selected"),
            ("y", "In the question: remove it permanently"),
            ("n  Esc", "In the question: go back"),
        ]
    }

    fn is_dialog(&self) -> bool {
        self.stage.is_busy()
    }
}

/// What Docker holds, and what neet can do about it
pub struct DockerFound {
    docker: Docker,
    /// Volumes no container uses, which the prune keeps unless picked
    volumes: Vec<DockerVolume>,
    /// The space Docker Desktop's disk image takes, if it has one
    image: Option<u64>,
}

/// Which question is open
#[derive(Clone, Copy, PartialEq, Eq)]
enum Question {
    Prune,
    Reset,
}

/// What Docker did
pub enum DockerDone {
    Pruned {
        /// The space Docker says it freed, or why the prune failed
        space: Result<String, String>,
        /// Each volume picked, and why it failed if it did
        volumes: Vec<(String, Result<(), String>)>,
    },
    /// The space the disk image took, or why it was not moved
    Reset(Result<u64, String>),
}

fn cleanup_roots() -> Result<CleanupRoots, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set.".to_string())?;
    CleanupRoots::new(&home).map_err(|error| format!("The home folder could not be read: {error}"))
}

/// Docker's images, containers, volumes, and build cache, to prune with
/// `docker system prune` after a red question, or to reset by moving its
/// whole disk image to the Trash
pub struct DockerSpace {
    stage: Stage<DockerFound, DockerDone>,
    question: Question,
    chosen: Vec<bool>,
    list: TableState,
    note: Option<String>,
}

impl DockerSpace {
    pub fn new() -> Self {
        Self::with(Self::looking())
    }

    fn looking() -> Stage<DockerFound, DockerDone> {
        Stage::look(|| {
            let docker = tools::docker_usage()
                .map_err(|error| format!("Docker could not say what it holds: {error}"))?;
            let volumes = if matches!(docker, Docker::Usage(_)) {
                tools::unused_volumes().unwrap_or_default()
            } else {
                Vec::new()
            };
            let image = cleanup_roots()
                .ok()
                .and_then(|roots| tools::docker_image(&roots))
                .map(|(_, size)| size);
            Ok(DockerFound {
                docker,
                volumes,
                image,
            })
        })
    }

    fn with(stage: Stage<DockerFound, DockerDone>) -> Self {
        Self {
            stage,
            question: Question::Prune,
            chosen: Vec::new(),
            list: TableState::default().with_selected(Some(0)),
            note: None,
        }
    }

    fn image(&self) -> Option<u64> {
        self.stage.found().and_then(|found| found.image)
    }

    fn picked_volumes(&self) -> Vec<&DockerVolume> {
        self.stage.found().map_or_else(Vec::new, |found| {
            found
                .volumes
                .iter()
                .zip(&self.chosen)
                .filter(|(_, chosen)| **chosen)
                .map(|(volume, _)| volume)
                .collect()
        })
    }

    fn reset_line(image: u64) -> Line<'static> {
        labeled(
            "Reset",
            Color::Yellow,
            vec![
                key("x"),
                Span::raw(": stop Docker; move disk image, "),
                Span::raw(format::size(image)).bold(),
                Span::raw(", to the Trash."),
            ],
        )
    }

    fn not_running_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from("Docker Desktop is not running").yellow().bold(),
            Line::from("Docker can only say what it holds, and prune, while it runs."),
            Line::default(),
            step(1, vec![key("o"), Span::raw(": open Docker Desktop")]),
            step(
                2,
                vec![Span::raw(
                    "Wait for the whale in the menu bar to stop moving",
                )],
            ),
            step(3, vec![key("r"), Span::raw(": refresh")]),
        ];
        if let Some(image) = self.image() {
            lines.push(Line::default());
            lines.push(Line::from(vec![
                Span::raw("Or press "),
                key("x"),
                Span::raw(" to reset Docker without opening it."),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Its disk image, "),
                Span::raw(format::size(image)).bold(),
                Span::raw(", goes to the Trash."),
            ]));
        }
        if let Some(note) = &self.note {
            lines.push(Line::default());
            lines.push(Line::from(note.clone()).fg(super::visual::ACCENT));
        }
        lines
    }

    fn draw_usage(&mut self, frame: &mut Frame, area: Rect) {
        let Some(found) = self.stage.found() else {
            return;
        };
        let Docker::Usage(usage) = &found.docker else {
            return;
        };
        let volumes = found.volumes.clone();
        let image = found.image;
        let usage_height = u16::try_from(usage.len() + 4).unwrap_or(u16::MAX);
        let [table, rest] =
            Layout::vertical([Constraint::Length(usage_height), Constraint::Fill(1)]).areas(area);
        frame.render_widget(usage_table(usage, table.width), table);

        let rest = if volumes.is_empty() {
            rest
        } else {
            let height = u16::try_from(volumes.len() + 4)
                .unwrap_or(u16::MAX)
                .min(rest.height / 2);
            let [list, rest] =
                Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(rest);
            self.chosen.resize(volumes.len(), false);
            let picked = self.picked_volumes().len();
            frame.render_stateful_widget(
                volume_table(&volumes, &self.chosen, picked, list.width),
                list,
                &mut self.list,
            );
            rest
        };

        let mut lines = vec![Line::from("Permanently: prune skips Trash.").red().bold()];
        if let Some(image) = image {
            lines.push(Self::reset_line(image));
        }
        lines.extend([
            labeled(
                "Removes",
                Color::Red,
                vec![Span::raw(
                    "stopped containers, unused networks/images, build cache",
                )],
            ),
            labeled(
                "Keeps",
                Color::Green,
                vec![Span::raw("running containers/images; unselected volumes")],
            ),
        ]);
        if let Some(note) = &self.note {
            lines.push(Line::default());
            lines.push(Line::from(note.clone()).fg(super::visual::ACCENT));
        }
        about_box(frame, rest, lines);
    }

    fn prune_question(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from("Run docker system prune --all?").bold(),
            Line::default(),
            Line::from(
                "Stopped containers, unused networks, every unused image, and the build cache are removed permanently.",
            ),
        ];
        let volumes = self.picked_volumes();
        if volumes.is_empty() {
            lines.push(Line::from("Volumes are kept."));
        } else {
            lines.push(Line::default());
            lines.push(Line::from("And these volumes, with their data:").bold());
            for volume in volumes {
                lines.push(Line::from(vec![
                    Span::raw("•  ").red(),
                    Span::raw(volume.name.clone()).bold(),
                    Span::raw(format!(", {}", volume.size)),
                ]));
            }
        }
        lines
    }

    fn reset_question(&self) -> Vec<Line<'static>> {
        let size = self.image().map_or_else(String::new, format::size);
        vec![
            Line::from(format!(
                "Stop Docker Desktop and move its disk image, {size}, to the Trash?"
            ))
            .bold(),
            Line::default(),
            Line::from("Every image, container, and volume goes with it, databases included."),
            Line::from(
                "Docker Desktop starts empty next time and downloads images again as needed.",
            ),
            Line::default(),
            Line::from(vec![
                Span::raw("Undo: ").green().bold(),
                Span::raw(
                    "move Docker.raw from the Trash back to its folder before opening Docker Desktop again.",
                ),
            ]),
        ]
    }

    fn start(&mut self) {
        let work = match self.question {
            Question::Prune => {
                let volumes: Vec<String> = self
                    .picked_volumes()
                    .iter()
                    .map(|volume| volume.name.clone())
                    .collect();
                spawn(move || {
                    let space = tools::docker_prune().map_err(|error| error.to_string());
                    let volumes = volumes
                        .into_iter()
                        .map(|name| {
                            let result =
                                tools::remove_volume(&name).map_err(|error| error.to_string());
                            (name, result)
                        })
                        .collect();
                    DockerDone::Pruned { space, volumes }
                })
            }
            Question::Reset => spawn(|| {
                DockerDone::Reset(
                    cleanup_roots()
                        .and_then(|roots| tools::reset_docker(&roots).map_err(|e| e.to_string())),
                )
            }),
        };
        self.stage = Stage::Working(Instant::now(), work);
    }

    fn done_lines(done: &DockerDone) -> (Vec<Line<'static>>, Color) {
        let mut lines = Vec::new();
        let mut color = Color::Green;
        match done {
            DockerDone::Pruned { space, volumes } => {
                match space {
                    Ok(space) if space.starts_with("0B") && volumes.is_empty() => {
                        color = Color::Yellow;
                        lines.push(Line::from("Docker had nothing to free").yellow().bold());
                        lines.push(Line::default());
                        lines.push(Line::from(
                            "What is left belongs to running containers, or sits in volumes.",
                        ));
                        lines.push(Line::from(vec![
                            Span::raw("To clear all of it, press "),
                            key("r"),
                            Span::raw(", then "),
                            key("x"),
                            Span::raw(" to reset Docker."),
                        ]));
                    }
                    Ok(space) => {
                        lines.push(Line::from(vec![
                            Span::raw("✓ Docker freed ").green().bold(),
                            Span::raw(space.clone()).green().bold(),
                        ]));
                    }
                    Err(reason) => {
                        color = Color::Red;
                        lines.push(cross(format!("The prune failed: {reason}")));
                    }
                }
                if !volumes.is_empty() {
                    lines.push(Line::default());
                }
                for (name, result) in volumes {
                    lines.push(match result {
                        Ok(()) => tick(format!("Volume {name} removed")),
                        Err(reason) => {
                            color = Color::Yellow;
                            cross(format!("Volume {name} was not removed: {reason}"))
                        }
                    });
                }
                if color == Color::Green {
                    lines.push(Line::default());
                    lines.push(Line::from(
                        "Docker's disk image gives the space back to macOS over a few minutes.",
                    ));
                }
            }
            DockerDone::Reset(Ok(size)) => {
                lines.push(tick("Docker is reset".to_string()).bold());
                lines.push(Line::default());
                lines.push(Line::from(vec![
                    Span::raw("Its disk image, "),
                    Span::raw(format::size(*size)).bold(),
                    Span::raw(", is in the Trash."),
                ]));
                lines.push(Line::from("Empty the Trash to free the space."));
                lines.push(Line::from(
                    "Docker Desktop makes a new, empty disk image when it opens.",
                ));
                lines.push(Line::default());
                lines.push(Line::from(vec![
                    Span::raw("Undo: ").green().bold(),
                    Span::raw("move Docker.raw from the Trash back to its folder before opening Docker Desktop again."),
                ]));
            }
            DockerDone::Reset(Err(reason)) => {
                color = Color::Red;
                lines.push(cross(format!("Docker was not reset: {reason}")));
                lines.push(Line::default());
                lines.push(Line::from("Nothing was moved."));
            }
        }
        lines.push(Line::default());
        lines.push(back_home());
        (lines, color)
    }
}

/// The volumes no container uses, to pick for the prune
fn volume_table(
    volumes: &[DockerVolume],
    chosen: &[bool],
    picked: usize,
    width: u16,
) -> Table<'static> {
    let sizes = format::column_width("Size", volumes.iter().map(|volume| volume.size.clone()));
    let columns = super::visual::Columns::new(width, [(3, 0), (sizes, 0), (16, 1)], &[], true);
    let rows: Vec<Row> = volumes
        .iter()
        .zip(chosen)
        .map(|(volume, &chosen)| {
            columns.row([
                Cell::from(checkbox(chosen, true)),
                Cell::from(Line::from(Span::raw(volume.size.clone()).yellow()).right_aligned()),
                Cell::from(format::shorten_middle(&volume.name, columns.width(2))),
            ])
        })
        .collect();
    let bottom = if picked == 0 {
        Span::raw(" Space: select volumes to remove ")
    } else {
        Span::raw(format!(
            " {} removed permanently, data included ",
            counted(picked, "volume")
        ))
        .red()
        .bold()
    };
    Table::new(rows, columns.widths())
        .header(
            columns
                .row([
                    Cell::from(""),
                    Cell::from(Line::from("Size").right_aligned()),
                    Cell::from("Volume"),
                ])
                .style(super::visual::HEADING)
                .bottom_margin(1),
        )
        .column_spacing(2)
        .block(
            super::visual::block()
                .title(" Unused volumes ")
                .title_bottom(bottom)
                .padding(Padding::horizontal(1)),
        )
        .highlight_symbol("▸ ")
        .row_highlight_style(super::visual::SELECTED)
}

/// `docker system df`, as a table
fn usage_table(usage: &[DockerUsage], width: u16) -> Table<'static> {
    let counts = format::column_width("Count", usage.iter().map(|line| line.total_count.clone()));
    let active = format::column_width("In use", usage.iter().map(|line| line.active.clone()));
    let sizes = format::column_width("Size", usage.iter().map(|line| line.size.clone()));
    let reclaim = format::column_width(
        "Reclaimable",
        usage.iter().map(|line| line.reclaimable.clone()),
    );
    let columns = super::visual::Columns::new(
        width,
        [(11, 1), (counts, 0), (active, 0), (sizes, 0), (reclaim, 0)],
        &[],
        false,
    );
    let rows: Vec<Row> = usage
        .iter()
        .map(|line| {
            let reclaim = if line.reclaimable.starts_with("0B") {
                Span::raw(line.reclaimable.clone())
            } else {
                Span::raw(line.reclaimable.clone()).yellow().bold()
            };
            columns.row([
                Cell::from(line.kind.clone()),
                Cell::from(Line::from(line.total_count.clone()).right_aligned()),
                Cell::from(Line::from(line.active.clone()).right_aligned()),
                Cell::from(Line::from(line.size.clone()).right_aligned()),
                Cell::from(Line::from(reclaim).right_aligned()),
            ])
        })
        .collect();
    let header = columns
        .row([
            Cell::from("Kind"),
            Cell::from(Line::from("Count").right_aligned()),
            Cell::from(Line::from("In use").right_aligned()),
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from(Line::from("Reclaimable").right_aligned()),
        ])
        .style(super::visual::HEADING)
        .bottom_margin(1);
    Table::new(rows, columns.widths())
        .header(header)
        .column_spacing(2)
        .block(
            super::visual::block()
                .title(" Docker ")
                .padding(Padding::horizontal(1)),
        )
}

impl Screen for DockerSpace {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.stage.poll();
        let title = "Docker";
        match &self.stage {
            Stage::Looking(started, _) => Loading {
                title,
                doing: "Asking Docker what it holds",
                progress: format!("{}s", started.elapsed().as_secs()),
                note: "Read-only scan.",
            }
            .draw(frame, area),
            Stage::Failed(reason) => message(
                frame,
                area,
                title,
                vec![
                    cross(reason.clone()),
                    Line::default(),
                    Line::from(vec![
                        Span::raw("Press "),
                        key("r"),
                        Span::raw(" to look again."),
                    ]),
                ],
                Color::Red,
            ),
            Stage::Working(started, _) => Loading {
                title,
                doing: match self.question {
                    Question::Prune => "Running docker system prune --all",
                    Question::Reset => "Stopping Docker Desktop, then moving its disk image",
                },
                progress: format!("{}s", started.elapsed().as_secs()),
                note: "This can take a minute.",
            }
            .draw(frame, area),
            Stage::Done(done) => {
                let (lines, color) = Self::done_lines(done);
                message(frame, area, title, lines, color);
            }
            Stage::Ready(found) | Stage::Asking(found) => {
                match found.docker {
                    Docker::NotInstalled => message(
                        frame,
                        area,
                        title,
                        vec![Line::from(
                            "Docker is not installed, so there is nothing to clear.",
                        )],
                        Color::Reset,
                    ),
                    Docker::NotRunning => {
                        message(frame, area, title, self.not_running_lines(), Color::Yellow);
                    }
                    Docker::Usage(_) => self.draw_usage(frame, area),
                }
                if matches!(self.stage, Stage::Asking(_)) {
                    match self.question {
                        Question::Prune => ask(
                            frame,
                            area,
                            "Remove permanently",
                            self.prune_question(),
                            "remove permanently",
                            Color::Red,
                        ),
                        Question::Reset => ask(
                            frame,
                            area,
                            "Reset Docker",
                            self.reset_question(),
                            "reset Docker",
                            Color::Yellow,
                        ),
                    }
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        self.note = None;
        match &self.stage {
            Stage::Asking(_) => {
                match key.code {
                    KeyCode::Char('y') => self.start(),
                    KeyCode::Char('n') => self.stage.back_to_ready(),
                    _ => {}
                }
                return Action::None;
            }
            Stage::Done(_) => {
                return match key.code {
                    KeyCode::Enter => Action::Home,
                    KeyCode::Char('r') => {
                        self.chosen.clear();
                        self.stage = Self::looking();
                        Action::None
                    }
                    _ => Action::None,
                };
            }
            Stage::Looking(..) | Stage::Working(..) => return Action::None,
            Stage::Failed(_) | Stage::Ready(_) => {}
        }
        let (usage, volumes) = match self.stage.found() {
            Some(found) => (
                matches!(found.docker, Docker::Usage(_)),
                found.volumes.len(),
            ),
            None => (false, 0),
        };
        let index = self.list.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('r') => {
                self.chosen.clear();
                self.stage = Self::looking();
            }
            KeyCode::Char('o') => {
                self.note = Some(match tools::start_docker() {
                    Ok(()) => "Opening Docker Desktop. Press r once it is running.".to_string(),
                    Err(error) => format!("Docker Desktop could not be opened: {error}"),
                });
            }
            KeyCode::Char('x') if self.image().is_some() => {
                self.question = Question::Reset;
                self.stage.ask();
            }
            KeyCode::Enter if usage => {
                self.question = Question::Prune;
                self.stage.ask();
            }
            KeyCode::Up | KeyCode::Char('k') if volumes > 0 => {
                self.list.select(Some(index.saturating_sub(1)));
            }
            KeyCode::Down | KeyCode::Char('j') if volumes > 0 => {
                self.list.select(Some((index + 1).min(volumes - 1)));
            }
            KeyCode::Char(' ') if volumes > 0 => {
                self.chosen.resize(volumes, false);
                if let Some(chosen) = self.chosen.get_mut(index) {
                    *chosen = !*chosen;
                }
            }
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        match self.stage {
            Stage::Asking(_) => {
                self.stage.back_to_ready();
                Action::None
            }
            Stage::Working(..) => Action::None,
            Stage::Done(_) => Action::Home,
            _ => Action::Back,
        }
    }

    fn hints(&self) -> &'static str {
        let reset = self.image().is_some();
        match &self.stage {
            Stage::Asking(_) if self.question == Question::Reset => {
                "y reset Docker · n or esc back"
            }
            Stage::Asking(_) => "y remove permanently · n or esc back",
            Stage::Working(..) => "Working…",
            Stage::Done(_) => "enter home · r look again · esc home",
            Stage::Ready(found) => match (&found.docker, found.volumes.is_empty(), reset) {
                (Docker::Usage(_), false, true) => {
                    "↑↓ move · space select · enter prune · x reset · r look again · esc back · ? help"
                }
                (Docker::Usage(_), false, false) => {
                    "↑↓ move · space select · enter prune · r look again · esc back · ? help"
                }
                (Docker::Usage(_), true, true) => {
                    "enter prune · x reset · r look again · esc back · ? help"
                }
                (Docker::Usage(_), true, false) => "enter prune · r look again · esc back · ? help",
                (Docker::NotRunning, _, true) => {
                    "o open Docker Desktop · x reset · r look again · esc back · ? help"
                }
                (Docker::NotRunning, _, false) => {
                    "o open Docker Desktop · r look again · esc back · ? help"
                }
                (Docker::NotInstalled, ..) => "esc back · ? help",
            },
            _ => "r look again · esc back · ? help",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("Enter", "Ask before running docker system prune --all"),
            ("↑ ↓  j k", "Move between unused volumes"),
            ("Space", "Select a volume to remove with the prune"),
            ("x", "Ask before resetting Docker to the Trash"),
            ("o", "Open Docker Desktop"),
            ("r", "Ask Docker again"),
            ("y", "In the question: go ahead"),
            ("n  Esc", "In the question: go back"),
        ]
    }

    fn is_dialog(&self) -> bool {
        self.stage.is_busy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;

    fn render(screen: &mut dyn Screen) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
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

    fn runtime(name: &str, size: u64) -> Runtime {
        Runtime {
            identifier: "AAAA-1111".to_string(),
            runtime_identifier: format!(
                "com.apple.CoreSimulator.SimRuntime.{}",
                name.replace([' ', '.'], "-")
            ),
            name: name.to_string(),
            build: "22G86".to_string(),
            size,
            last_used: Some("2026-09-17T22:26:41Z".to_string()),
            deletable: true,
        }
    }

    fn device(name: &str, runtime: &str, available: bool, size: u64) -> Device {
        Device {
            udid: "89B174CA-DC9E-4D90".to_string(),
            name: name.to_string(),
            runtime: format!(
                "com.apple.CoreSimulator.SimRuntime.{}",
                runtime.replace([' ', '.'], "-")
            ),
            available,
            size,
        }
    }

    #[test]
    fn nothing_is_deleted_without_a_selection_and_a_yes() {
        let mut screen = Simulators::with(Stage::Ready(SimulatorsFound {
            runtimes: vec![
                runtime("iOS 18.6", 8_800_000_000),
                runtime("iOS 26.5", 8_500_000_000),
            ],
            devices: vec![device("iPhone 16 Pro", "iOS 18.6", true, 2_100_000_000)],
        }));

        let text = render(&mut screen);
        assert!(text.contains("iOS 18.6"));
        assert!(text.contains("8.8 GB"));
        assert!(text.contains("2026-09-17"));
        assert!(text.contains("1 simulator, 2.1 GB"));
        assert!(text.contains("Selected: 0 B"));

        // Nothing selected, so Enter does not ask.
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(screen.stage, Stage::Ready(_)));

        press(&mut screen, KeyCode::Char(' '));
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen);
        assert!(text.contains("iOS 18.6 runtime, 8.8 GB"));
        assert!(text.contains("and 1 simulator on it, 2.1 GB"));
        assert!(text.contains("10.9 GB"));
        assert!(screen.is_dialog());

        // n goes back without deleting.
        press(&mut screen, KeyCode::Char('n'));
        assert!(matches!(screen.stage, Stage::Ready(_)));
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(screen.back(), Action::None));
        assert!(matches!(screen.stage, Stage::Ready(_)));
    }

    #[test]
    fn simulators_with_no_runtime_get_their_own_row() {
        let mut screen = Simulators::with(Stage::Ready(SimulatorsFound {
            runtimes: Vec::new(),
            devices: vec![
                device("iPhone 16 Pro", "iOS 18.6", false, 2_000_000_000),
                device("iPhone 16", "iOS 18.6", false, 1_000_000_000),
            ],
        }));

        let text = render(&mut screen);
        assert!(text.contains("No runtime"));
        assert!(text.contains("2 simulators that cannot start"));

        press(&mut screen, KeyCode::Char(' '));
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen);
        assert!(text.contains("2 simulators with no runtime, 3.0 GB"));
    }

    #[test]
    fn no_simulators_says_so_in_a_small_box() {
        let mut screen = Simulators::with(Stage::Ready(SimulatorsFound {
            runtimes: Vec::new(),
            devices: Vec::new(),
        }));

        assert!(render(&mut screen).contains("Nothing to remove"));
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(screen.stage, Stage::Ready(_)));
    }

    #[test]
    fn removing_shows_what_went_with_a_tick() {
        let mut screen = Simulators::with(Stage::Done(Removal {
            runtimes: vec![("iOS 18.6".to_string(), 8_800_000_000, Ok(()))],
            devices: 6,
            devices_size: 2_100_000_000,
            device_errors: Vec::new(),
        }));

        let text = render(&mut screen);
        assert!(text.contains("✓ Removed 10.9 GB"));
        assert!(text.contains("✓ iOS 18.6 runtime"));
        assert!(text.contains("✓ 6 simulators"));
        assert!(matches!(press(&mut screen, KeyCode::Enter), Action::Home));
    }

    fn docker(docker: Docker, volumes: Vec<DockerVolume>, image: Option<u64>) -> DockerSpace {
        DockerSpace::with(Stage::Ready(DockerFound {
            docker,
            volumes,
            image,
        }))
    }

    #[test]
    fn docker_shows_what_can_be_freed_and_asks_first() {
        let usage = DockerUsage {
            kind: "Images".to_string(),
            total_count: "9".to_string(),
            active: "2".to_string(),
            size: "1.808GB".to_string(),
            reclaimable: "1.105GB (61%)".to_string(),
        };
        let volume = DockerVolume {
            name: "influxdb-storage".to_string(),
            size: "2.812GB".to_string(),
        };
        let mut screen = docker(
            Docker::Usage(vec![usage]),
            vec![volume],
            Some(9_400_000_000),
        );

        let text = render(&mut screen);
        assert!(text.contains("1.105GB (61%)"));
        assert!(text.contains("influxdb-storage"));
        assert!(text.contains("unselected volumes"));

        // Volumes are kept unless picked.
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen);
        assert!(text.contains("Run docker system prune --all?"));
        assert!(text.contains("Volumes are kept."));
        press(&mut screen, KeyCode::Char('n'));
        assert!(matches!(screen.stage, Stage::Ready(_)));

        press(&mut screen, KeyCode::Char(' '));
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen);
        assert!(text.contains("And these volumes, with their data:"));
        assert!(text.contains("influxdb-storage, 2.812GB"));
        press(&mut screen, KeyCode::Char('n'));
        assert!(matches!(screen.stage, Stage::Ready(_)));
    }

    #[test]
    fn docker_says_to_open_docker_desktop_when_it_is_not_running() {
        let mut screen = docker(Docker::NotRunning, Vec::new(), None);

        let text = render(&mut screen);
        assert!(text.contains("o: open Docker Desktop"));
        assert!(!text.contains("reset Docker"));
        press(&mut screen, KeyCode::Enter);
        press(&mut screen, KeyCode::Char('x'));
        assert!(matches!(screen.stage, Stage::Ready(_)));
        assert!(!screen.hints().contains("enter prune"));
    }

    #[test]
    fn docker_reset_asks_first_and_names_the_trash() {
        let mut screen = docker(Docker::NotRunning, Vec::new(), Some(9_400_000_000));

        assert!(render(&mut screen).contains("reset Docker without opening it"));
        press(&mut screen, KeyCode::Char('x'));
        let text = render(&mut screen);
        assert!(text.contains("Reset Docker"));
        assert!(text.contains("move its disk image, 9.4 GB, to the Trash?"));
        assert!(screen.is_dialog());
        press(&mut screen, KeyCode::Char('n'));
        assert!(matches!(screen.stage, Stage::Ready(_)));
    }

    #[test]
    fn docker_done_shows_a_tick() {
        let mut screen = DockerSpace::with(Stage::Done(DockerDone::Pruned {
            space: Ok("1.2GB".to_string()),
            volumes: vec![("influxdb-storage".to_string(), Ok(()))],
        }));
        let text = render(&mut screen);
        assert!(text.contains("✓ Docker freed 1.2GB"));
        assert!(text.contains("✓ Volume influxdb-storage removed"));

        let mut screen = DockerSpace::with(Stage::Done(DockerDone::Reset(Ok(9_400_000_000))));
        let text = render(&mut screen);
        assert!(text.contains("✓ Docker is reset"));
        assert!(text.contains("9.4 GB"));
    }

    #[test]
    fn a_long_failure_shows_its_last_line() {
        let mut screen = DockerSpace::with(Stage::Failed(
            "Docker could not say what it holds: failed to start: dial unix /Users/someone/.docker/run/docker.sock: connect: no such file or directory, and more words after it".to_string(),
        ));

        assert!(render(&mut screen).contains("Press r to look again."));
    }
    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let mut sims = Simulators::with(Stage::Ready(SimulatorsFound {
            runtimes: vec![runtime("iOS 18.6", 8_800_000_000)],
            devices: vec![device("iPhone 16 Pro", "iOS 18.6", true, 2_100_000_000)],
        }));
        let usage = DockerUsage {
            kind: "Images".into(),
            total_count: "1234567".into(),
            active: "2".into(),
            size: "1.808GB".into(),
            reclaimable: "1.105GB (61%)".into(),
        };
        let mut docker = docker(
            Docker::Usage(vec![usage]),
            vec![DockerVolume {
                name: "influxdb-storage".into(),
                size: "2.812GB".into(),
            }],
            Some(9_400_000_000),
        );
        for (width, height) in view::SIZES {
            let buffer = view::render("simulators", &mut sims, &context, width, height);
            view::aligned(&buffer, "Size", "8.8 GB");
            assert!(view::text(&buffer).contains("Permanently"));
            let buffer = view::render("docker", &mut docker, &context, width, height);
            view::aligned(&buffer, "Count", "1234567");
            view::aligned(&buffer, "Reclaimable", "1.105GB (61%)");
            assert!(view::text(&buffer).contains("influxdb-storage"));
        }
        press(&mut sims, KeyCode::Char(' '));
        press(&mut sims, KeyCode::Enter);
        press(&mut docker, KeyCode::Enter);
        for (width, height) in view::SIZES {
            let buffer = view::render("simulator-confirm", &mut sims, &context, width, height);
            assert!(view::text(&buffer).contains("remove permanently"));
            let buffer = view::render("docker-confirm", &mut docker, &context, width, height);
            assert!(view::text(&buffer).contains("remove permanently"));
        }
    }
}
