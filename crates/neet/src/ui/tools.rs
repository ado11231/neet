use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use neet_core::tools::{self, Docker, Runtime};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap};

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

    /// Asking or running the tool, when keys like `q` must not act
    fn is_busy(&self) -> bool {
        matches!(self, Self::Asking(_) | Self::Working(..))
    }
}

/// The red question before a tool removes something permanently
fn ask(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>) {
    let mut lines = lines;
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::raw("y").bold().red(),
        Span::raw(" remove permanently    "),
        Span::raw("n").bold(),
        Span::raw(" or "),
        Span::raw("Esc").bold(),
        Span::raw(" go back"),
    ]));
    let width = 66.min(area.width);
    let inner = usize::from(width.saturating_sub(4)).max(1);
    let rows: usize = lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(inner))
        .sum();
    let height = u16::try_from(rows + 3).unwrap_or(u16::MAX).min(area.height);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .border_style(Style::new().red())
                .title(Line::from(format!(" {title} ")).red().bold())
                .padding(Padding::horizontal(1)),
        ),
        area,
    );
}

fn failed_box(frame: &mut Frame, area: Rect, title: &str, reason: &str) {
    frame.render_widget(
        Paragraph::new(reason.to_string())
            .red()
            .wrap(Wrap { trim: true })
            .block(
                Block::bordered()
                    .title(format!(" {title} "))
                    .padding(Padding::horizontal(1)),
            ),
        area,
    );
}

/// The date part of a time `simctl` gives, such as `2026-09-17`
fn day(time: &str) -> &str {
    time.split('T').next().unwrap_or(time)
}

/// What deleting each runtime did: its name, and why it failed if it did
type Deleted = Vec<(String, Result<(), String>)>;

/// Every simulator runtime, to delete with `xcrun simctl` after a red
/// question. Nothing starts selected.
pub struct Simulators {
    stage: Stage<Vec<Runtime>, Deleted>,
    chosen: Vec<bool>,
    list: TableState,
}

impl Simulators {
    pub fn new() -> Self {
        Self::with(Stage::look(|| {
            tools::runtimes().map_err(|error| format!("simctl could not list runtimes: {error}"))
        }))
    }

    fn with(stage: Stage<Vec<Runtime>, Deleted>) -> Self {
        Self {
            stage,
            chosen: Vec::new(),
            list: TableState::default().with_selected(Some(0)),
        }
    }

    fn runtimes(&self) -> &[Runtime] {
        match &self.stage {
            Stage::Ready(runtimes) | Stage::Asking(runtimes) => runtimes,
            _ => &[],
        }
    }

    fn picked(&self) -> Vec<&Runtime> {
        self.runtimes()
            .iter()
            .zip(&self.chosen)
            .filter(|(_, chosen)| **chosen)
            .map(|(runtime, _)| runtime)
            .collect()
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let runtimes = self.runtimes().to_vec();
        self.chosen.resize(runtimes.len(), false);
        let picked = self.picked();
        let picked_size: u64 = picked.iter().map(|runtime| runtime.size).sum();
        let total: u64 = runtimes.iter().map(|runtime| runtime.size).sum();
        let rows: Vec<Row> = runtimes
            .iter()
            .zip(&self.chosen)
            .map(|(runtime, &chosen)| {
                let row = Row::new([
                    Cell::from(checkbox(chosen, runtime.deletable)),
                    Cell::from(runtime.name.clone()),
                    Cell::from(runtime.build.clone()),
                    Cell::from(
                        Line::from(format::size_span(runtime.size, format::size(runtime.size)))
                            .right_aligned(),
                    ),
                    Cell::from(
                        runtime
                            .last_used
                            .as_deref()
                            .map_or("never", day)
                            .to_string(),
                    ),
                    Cell::from(if runtime.deletable {
                        Span::raw("")
                    } else {
                        Span::raw("simctl will not delete it").italic()
                    }),
                ]);
                if runtime.deletable {
                    row
                } else {
                    row.dark_gray()
                }
            })
            .collect();
        let header = Row::new([
            Cell::from(""),
            Cell::from("Runtime"),
            Cell::from("Build"),
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from("Last used"),
            Cell::from(""),
        ])
        .bold()
        .bottom_margin(1);
        let selected = Span::raw(format!(
            " Selected: {} of {} · {} ",
            picked.len(),
            runtimes.len(),
            format::size(picked_size)
        ));
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(16),
                Constraint::Length(10),
                Constraint::Length(9),
                Constraint::Length(12),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(
            Block::bordered()
                .title(format!(" Simulator runtimes · {} ", format::size(total)))
                .title_bottom(if picked.is_empty() {
                    selected
                } else {
                    selected.red().bold()
                })
                .padding(Padding::horizontal(1)),
        )
        .highlight_symbol("▸ ")
        .row_highlight_style(Style::new().bold());
        frame.render_stateful_widget(table, area, &mut self.list);
    }
}

fn simulators_about(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(
            "Each runtime lets Xcode run simulators of one iOS, watchOS, tvOS, or visionOS version.",
        ),
        Line::default(),
        Line::from(vec![
            Span::raw("Permanently: ").red().bold(),
            Span::raw(
                "runtimes cannot go to the Trash. neet asks xcrun simctl runtime delete to remove them.",
            ),
        ]),
        Line::from(
            "Simulators on a deleted runtime stop working until Xcode downloads it again, in Settings, Components.",
        ),
        Line::default(),
        Line::from("Select the ones you do not test on with Space, then press Enter."),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().padding(Padding::horizontal(1))),
        area,
    );
}

impl Screen for Simulators {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.stage.poll();
        let title = "Simulator runtimes";
        match &self.stage {
            Stage::Looking(started, _) => {
                return Loading {
                    title,
                    doing: "Asking xcrun simctl for the runtimes",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Nothing is changed while neet looks.",
                }
                .draw(frame, area);
            }
            Stage::Failed(reason) => return failed_box(frame, area, title, reason),
            Stage::Working(started, _) => {
                return Loading {
                    title,
                    doing: "Deleting with xcrun simctl",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Each runtime is unmounted, then deleted. This can take a minute.",
                }
                .draw(frame, area);
            }
            Stage::Done(deleted) => {
                let mut lines: Vec<Line> = deleted
                    .iter()
                    .map(|(name, result)| match result {
                        Ok(()) => Line::from(vec![
                            Span::raw("✓ ").green().bold(),
                            Span::raw(format!("{name} deleted")),
                        ]),
                        Err(reason) => Line::from(vec![
                            Span::raw("✗ ").red().bold(),
                            Span::raw(format!("{name} was not deleted: {reason}")),
                        ]),
                    })
                    .collect();
                lines.push(Line::default());
                lines.push(Line::from("Press Enter to go back to Home."));
                frame.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                        Block::bordered()
                            .title(format!(" {title} "))
                            .padding(Padding::horizontal(1)),
                    ),
                    area,
                );
                return;
            }
            Stage::Ready(runtimes) | Stage::Asking(runtimes) if runtimes.is_empty() => {
                return failed_box(
                    frame,
                    area,
                    title,
                    "There are no simulator runtimes on this Mac.",
                );
            }
            Stage::Ready(_) | Stage::Asking(_) => {}
        }
        let list_height = u16::try_from(self.runtimes().len() + 4).unwrap_or(u16::MAX);
        let [list, about] =
            Layout::vertical([Constraint::Length(list_height), Constraint::Fill(1)]).areas(area);
        self.draw_list(frame, list);
        simulators_about(frame, about);
        if matches!(self.stage, Stage::Asking(_)) {
            let picked = self.picked();
            let size: u64 = picked.iter().map(|runtime| runtime.size).sum();
            let names: Vec<String> = picked.iter().map(|runtime| runtime.name.clone()).collect();
            ask(
                frame,
                area,
                "Remove permanently",
                vec![
                    Line::from(format!(
                        "Permanently remove {}, {}?",
                        names.join(", "),
                        format::size(size)
                    ))
                    .bold(),
                    Line::default(),
                    Line::from(
                        "They do not go to the Trash. Simulators that use them stop working until Xcode downloads them again.",
                    ),
                ],
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match &self.stage {
            Stage::Asking(_) => {
                match key.code {
                    KeyCode::Char('y') => {
                        let picked: Vec<Runtime> = self.picked().into_iter().cloned().collect();
                        self.stage = Stage::Working(
                            Instant::now(),
                            spawn(move || {
                                picked
                                    .into_iter()
                                    .map(|runtime| {
                                        let result = tools::delete_runtime(&runtime.identifier)
                                            .map_err(|error| error.to_string());
                                        (runtime.name, result)
                                    })
                                    .collect()
                            }),
                        );
                    }
                    KeyCode::Char('n') => self.back_to_list(),
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
        let index = self.list.selected().unwrap_or(0);
        let last = self.runtimes().len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(index.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => self.list.select(Some((index + 1).min(last))),
            KeyCode::Char(' ') => {
                if self
                    .runtimes()
                    .get(index)
                    .is_some_and(|runtime| runtime.deletable)
                    && let Some(chosen) = self.chosen.get_mut(index)
                {
                    *chosen = !*chosen;
                }
            }
            KeyCode::Enter if !self.picked().is_empty() => {
                if let Stage::Ready(runtimes) =
                    std::mem::replace(&mut self.stage, Stage::Failed(String::new()))
                {
                    self.stage = Stage::Asking(runtimes);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        match self.stage {
            Stage::Asking(_) => {
                self.back_to_list();
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
            Stage::Working(..) => "deleting, please wait",
            Stage::Done(_) => "enter or esc home",
            _ => "↑↓ move · space select · enter remove · esc back · ? help",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("Space", "Select or clear a runtime"),
            ("Enter", "Ask before deleting the selected runtimes"),
            ("y", "In the question: delete them permanently"),
            ("n  Esc", "In the question: go back"),
        ]
    }

    fn is_dialog(&self) -> bool {
        self.stage.is_busy()
    }
}

impl Simulators {
    fn back_to_list(&mut self) {
        if let Stage::Asking(runtimes) =
            std::mem::replace(&mut self.stage, Stage::Failed(String::new()))
        {
            self.stage = Stage::Ready(runtimes);
        }
    }
}

/// Docker's images, containers, volumes, and build cache, to prune with
/// `docker system prune` after a red question
pub struct DockerSpace {
    stage: Stage<Docker, Result<String, String>>,
    note: Option<String>,
}

impl DockerSpace {
    pub fn new() -> Self {
        Self::with(Self::looking())
    }

    fn looking() -> Stage<Docker, Result<String, String>> {
        Stage::look(|| {
            tools::docker_usage()
                .map_err(|error| format!("Docker could not say what it holds: {error}"))
        })
    }

    fn with(stage: Stage<Docker, Result<String, String>>) -> Self {
        Self { stage, note: None }
    }

    fn draw_usage(&self, frame: &mut Frame, area: Rect, usage: &[tools::DockerUsage]) {
        let rows: Vec<Row> = usage
            .iter()
            .map(|line| {
                let reclaim = if line.reclaimable.starts_with("0B") {
                    Span::raw(line.reclaimable.clone())
                } else {
                    Span::raw(line.reclaimable.clone()).yellow().bold()
                };
                Row::new([
                    Cell::from(line.kind.clone()),
                    Cell::from(Line::from(line.total_count.clone()).right_aligned()),
                    Cell::from(Line::from(line.active.clone()).right_aligned()),
                    Cell::from(Line::from(line.size.clone()).right_aligned()),
                    Cell::from(reclaim),
                ])
            })
            .collect();
        let header = Row::new([
            Cell::from("Kind"),
            Cell::from(Line::from("Count").right_aligned()),
            Cell::from(Line::from("In use").right_aligned()),
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from("Can be freed"),
        ])
        .bold()
        .bottom_margin(1);
        let height = u16::try_from(usage.len() + 4).unwrap_or(u16::MAX);
        let [table, about] =
            Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(14),
                    Constraint::Length(6),
                    Constraint::Length(7),
                    Constraint::Length(10),
                    Constraint::Fill(1),
                ],
            )
            .header(header)
            .column_spacing(2)
            .block(
                Block::bordered()
                    .title(" Docker ")
                    .padding(Padding::horizontal(1)),
            ),
            table,
        );
        let mut lines = vec![
            Line::from("Enter runs docker system prune --all. It removes:").bold(),
            Line::from(vec![
                Span::raw("1. ").red(),
                Span::raw("Stopped containers."),
            ]),
            Line::from(vec![
                Span::raw("2. ").red(),
                Span::raw("Networks no container uses."),
            ]),
            Line::from(vec![
                Span::raw("3. ").red(),
                Span::raw(
                    "Every image no container uses. Docker downloads them again when needed.",
                ),
            ]),
            Line::from(vec![Span::raw("4. ").red(), Span::raw("The build cache.")]),
            Line::default(),
            Line::from(vec![
                Span::raw("Kept: ").green().bold(),
                Span::raw(
                    "running containers, the images they use, and every volume, where databases keep their data.",
                ),
            ]),
            Line::from(vec![
                Span::raw("Permanently: ").red().bold(),
                Span::raw("none of it goes to the Trash."),
            ]),
        ];
        if let Some(note) = &self.note {
            lines.insert(0, Line::from(note.clone()).yellow());
            lines.insert(1, Line::default());
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(Block::bordered().padding(Padding::horizontal(1))),
            about,
        );
    }
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
                note: "Nothing is changed while neet looks.",
            }
            .draw(frame, area),
            Stage::Failed(reason) => failed_box(frame, area, title, reason),
            Stage::Ready(Docker::NotInstalled) | Stage::Asking(Docker::NotInstalled) => {
                failed_box(frame, area, title, "Docker is not installed.");
            }
            Stage::Ready(Docker::NotRunning) | Stage::Asking(Docker::NotRunning) => {
                let mut lines = vec![
                    Line::from("Docker Desktop is not running, so Docker cannot say what it holds or remove anything.").yellow(),
                    Line::default(),
                    Line::from("1. Press o to open Docker Desktop."),
                    Line::from("2. Wait until it says it is running."),
                    Line::from("3. Press r to look again."),
                ];
                if let Some(note) = &self.note {
                    lines.push(Line::default());
                    lines.push(Line::from(note.clone()));
                }
                frame.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                        Block::bordered()
                            .title(format!(" {title} "))
                            .padding(Padding::horizontal(1)),
                    ),
                    area,
                );
            }
            Stage::Ready(Docker::Usage(usage)) => self.draw_usage(frame, area, usage),
            Stage::Asking(Docker::Usage(usage)) => {
                self.draw_usage(frame, area, usage);
                ask(
                    frame,
                    area,
                    "Remove permanently",
                    vec![
                        Line::from("Run docker system prune --all?").bold(),
                        Line::default(),
                        Line::from(
                            "Stopped containers, unused networks, every unused image, and the build cache are removed permanently. Volumes are kept.",
                        ),
                    ],
                );
            }
            Stage::Working(started, _) => Loading {
                title,
                doing: "Running docker system prune --all",
                progress: format!("{}s", started.elapsed().as_secs()),
                note: "This can take a minute.",
            }
            .draw(frame, area),
            Stage::Done(result) => {
                let lines = match result {
                    Ok(space) => vec![
                        Line::from(vec![
                            Span::raw("✓ ").green().bold(),
                            Span::raw("Docker freed "),
                            Span::raw(space.clone()).green().bold(),
                            Span::raw("."),
                        ]),
                        Line::default(),
                        Line::from(
                            "Docker's disk image can take a few minutes to give the space back to macOS.",
                        ),
                        Line::default(),
                        Line::from("Press Enter to go back to Home."),
                    ],
                    Err(reason) => vec![
                        Line::from(format!("✗ The prune failed: {reason}")).red(),
                        Line::default(),
                        Line::from("Press Enter to go back to Home."),
                    ],
                };
                frame.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                        Block::bordered()
                            .title(format!(" {title} "))
                            .padding(Padding::horizontal(1)),
                    ),
                    area,
                );
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        self.note = None;
        match &self.stage {
            Stage::Asking(_) => {
                match key.code {
                    KeyCode::Char('y') => {
                        self.stage = Stage::Working(
                            Instant::now(),
                            spawn(|| tools::docker_prune().map_err(|error| error.to_string())),
                        );
                    }
                    KeyCode::Char('n') => self.back_to_usage(),
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
            Stage::Looking(..) | Stage::Working(..) => return Action::None,
            Stage::Failed(_) | Stage::Ready(_) => {}
        }
        match key.code {
            KeyCode::Char('r') => self.stage = Self::looking(),
            KeyCode::Char('o') => {
                self.note = Some(match tools::start_docker() {
                    Ok(()) => "Opening Docker Desktop. Press r once it is running.".to_string(),
                    Err(error) => format!("Docker Desktop could not be opened: {error}"),
                });
            }
            KeyCode::Enter => {
                if matches!(self.stage, Stage::Ready(Docker::Usage(_)))
                    && let Stage::Ready(docker) =
                        std::mem::replace(&mut self.stage, Stage::Failed(String::new()))
                {
                    self.stage = Stage::Asking(docker);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        match self.stage {
            Stage::Asking(_) => {
                self.back_to_usage();
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
            Stage::Working(..) => "pruning, please wait",
            Stage::Done(_) => "enter or esc home",
            Stage::Ready(Docker::Usage(_)) => "enter prune · r look again · esc back · ? help",
            _ => "o open Docker Desktop · r look again · esc back · ? help",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("Enter", "Ask before running docker system prune --all"),
            ("o", "Open Docker Desktop"),
            ("r", "Ask Docker again"),
            ("y", "In the question: remove permanently"),
            ("n  Esc", "In the question: go back"),
        ]
    }

    fn is_dialog(&self) -> bool {
        self.stage.is_busy()
    }
}

impl DockerSpace {
    fn back_to_usage(&mut self) {
        if let Stage::Asking(docker) =
            std::mem::replace(&mut self.stage, Stage::Failed(String::new()))
        {
            self.stage = Stage::Ready(docker);
        }
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
        let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();
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
            name: name.to_string(),
            build: "22G86".to_string(),
            size,
            last_used: Some("2026-09-17T22:26:41Z".to_string()),
            deletable: true,
        }
    }

    #[test]
    fn nothing_is_deleted_without_a_selection_and_a_yes() {
        let mut screen = Simulators::with(Stage::Ready(vec![
            runtime("iOS 18.6", 8_800_000_000),
            runtime("iOS 26.5", 8_500_000_000),
        ]));

        let text = render(&mut screen);
        assert!(text.contains("iOS 18.6"));
        assert!(text.contains("8.8 GB"));
        assert!(text.contains("2026-09-17"));
        assert!(text.contains("Selected: 0 of 2"));

        // Nothing selected, so Enter does not ask.
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(screen.stage, Stage::Ready(_)));

        press(&mut screen, KeyCode::Char(' '));
        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen).contains("Permanently remove iOS 18.6, 8.8 GB?"));
        assert!(screen.is_dialog());

        // n goes back without deleting.
        press(&mut screen, KeyCode::Char('n'));
        assert!(matches!(screen.stage, Stage::Ready(_)));
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(screen.back(), Action::None));
        assert!(matches!(screen.stage, Stage::Ready(_)));
    }

    #[test]
    fn docker_shows_what_can_be_freed_and_asks_first() {
        let usage = tools::DockerUsage {
            kind: "Images".to_string(),
            total_count: "9".to_string(),
            active: "2".to_string(),
            size: "1.808GB".to_string(),
            reclaimable: "1.105GB (61%)".to_string(),
        };
        let mut screen = DockerSpace::with(Stage::Ready(Docker::Usage(vec![usage])));

        let text = render(&mut screen);
        assert!(text.contains("1.105GB (61%)"));
        assert!(text.contains("every volume"));

        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen).contains("Run docker system prune --all?"));
        press(&mut screen, KeyCode::Char('n'));
        assert!(matches!(screen.stage, Stage::Ready(_)));
    }

    #[test]
    fn docker_says_to_open_docker_desktop_when_it_is_not_running() {
        let mut screen = DockerSpace::with(Stage::Ready(Docker::NotRunning));

        assert!(render(&mut screen).contains("Press o to open Docker Desktop"));
        press(&mut screen, KeyCode::Enter);
        assert!(matches!(screen.stage, Stage::Ready(Docker::NotRunning)));
    }
}
