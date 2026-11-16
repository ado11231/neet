use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use neet_core::clean::RulePlan;
use neet_core::clutter::{self, Kind};
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

/// Items changed more recently than this are marked in yellow
const RECENT: Duration = Duration::from_secs(7 * 24 * 60 * 60);

enum State {
    Planning {
        started: Instant,
        result: Receiver<Result<Planned, String>>,
    },
    Ready(Arc<Planned>),
    Failed(String),
}

/// Every project build folder, or every installer in Downloads, each with a
/// checkbox. All start selected. `Enter` opens the same review as Deep Clean.
pub struct Pick {
    title: &'static str,
    state: State,
    list: TableState,
}

impl Pick {
    /// Measures `paths` and checks each one on its own thread.
    pub fn start(kind: Kind, title: &'static str, paths: Vec<PathBuf>) -> Self {
        let (sender, result) = mpsc::channel();
        thread::spawn(move || {
            let planned = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| "HOME is not set.".to_string())
                .and_then(|home| {
                    CleanupRoots::new(&home)
                        .map_err(|error| format!("The home folder could not be read: {error}"))
                })
                .map(|roots| Planned {
                    plan: clutter::plan(&roots, kind, &paths),
                    errors: Vec::new(),
                    home: roots.home().to_path_buf(),
                });
            // The receiver is gone only when the screen was closed.
            let _ = sender.send(planned);
        });
        Self {
            title,
            state: State::Planning {
                started: Instant::now(),
                result,
            },
            list: TableState::default().with_selected(Some(0)),
        }
    }

    #[cfg(test)]
    fn ready(title: &'static str, planned: Planned) -> Self {
        Self {
            title,
            state: State::Ready(Arc::new(planned)),
            list: TableState::default().with_selected(Some(0)),
        }
    }

    fn poll(&mut self) {
        let State::Planning { result, .. } = &self.state else {
            return;
        };
        self.state = match result.try_recv() {
            Ok(Ok(planned)) => State::Ready(Arc::new(planned)),
            Ok(Err(reason)) => State::Failed(reason),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                State::Failed("No result. Open this list again.".to_string())
            }
        };
    }

    /// The plan, to change, while no review holds a copy
    fn plan_mut(&mut self) -> Option<&mut Planned> {
        match &mut self.state {
            State::Ready(planned) => Arc::get_mut(planned),
            _ => None,
        }
    }

    fn toggle(&mut self) {
        let index = self.list.selected().unwrap_or(0);
        if let Some(planned) = self.plan_mut()
            && let Some(rule) = planned.plan.rules.get_mut(index)
            && !rule.items.is_empty()
        {
            rule.selected = !rule.selected;
        }
    }

    /// Selects every item, or clears them all when all are selected
    fn toggle_all(&mut self) {
        if let Some(planned) = self.plan_mut() {
            let rules = &mut planned.plan.rules;
            let all = rules
                .iter()
                .filter(|rule| !rule.items.is_empty())
                .all(|rule| rule.selected);
            for rule in rules.iter_mut().filter(|rule| !rule.items.is_empty()) {
                rule.selected = !all;
            }
        }
    }
}

/// The project a build folder belongs to, or the folder an installer is in
fn place(home: &Path, path: &Path) -> String {
    path.parent()
        .map(|parent| display_path(home, parent))
        .unwrap_or_default()
}

/// Width of the size bars
const BAR: u16 = 10;

/// Width of the labels in the boxes
const LABEL: usize = 11;

fn field(label: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{label:<LABEL$}")).bold()];
    spans.extend(value);
    Line::from(spans)
}

fn titled(title: &str) -> ratatui::widgets::Block<'static> {
    super::visual::block()
        .title(format!(" {title} "))
        .padding(Padding::horizontal(1))
}

/// One item: its checkbox, size & bar against the largest, path, and age.
/// An item left in place keeps every cell, with `kept` for its size.
fn item_row(
    rule: &RulePlan,
    home: &Path,
    now: SystemTime,
    largest: u64,
    columns: &super::visual::Columns<5>,
) -> Row<'static> {
    let width = columns.width(3);
    let Some(item) = rule.items.first() else {
        let path = rule
            .skipped
            .first()
            .map(|skipped| display_path(home, &skipped.path))
            .unwrap_or_default();
        return columns.row([
            Cell::from(Span::raw("[–]").yellow()),
            Cell::from(Line::from(Span::raw("kept").yellow()).right_aligned()),
            Cell::from(format::size_bar(0, largest, usize::from(BAR))),
            Cell::from(format::shorten_path(&path, width)),
            Cell::from(""),
        ]);
    };
    let name = item
        .path
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = place(home, item.path.path());
    let room = width.saturating_sub(format::display_width(&name) + 1);
    let elapsed = now.duration_since(item.changed).unwrap_or_default();
    // Changed this week: you may be working in it
    let age = if elapsed < RECENT {
        Span::raw(format::age(elapsed)).yellow()
    } else {
        Span::raw(format::age(elapsed))
    };
    columns.row([
        Cell::from(checkbox(rule.selected, true)),
        Cell::from(
            Line::from(format::size_span(item.size, format::size(item.size))).right_aligned(),
        ),
        Cell::from(format::size_bar(item.size, largest, usize::from(BAR))),
        Cell::from(Line::from(vec![
            Span::raw(format!("{}/", format::shorten_path(&folder, room)))
                .fg(super::visual::ACCENT),
            Span::raw(format::shorten_middle(&name, width.saturating_sub(2))),
        ])),
        Cell::from(age),
    ])
}

/// What an item is, from its name, and how it comes back
fn what_it_is(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower == "node_modules" {
        "JavaScript packages. npm install brings them back."
    } else if lower == "target" {
        "Rust build files. cargo build makes them again."
    } else if [".dmg", ".pkg", ".iso"]
        .iter()
        .any(|end| lower.ends_with(end))
    {
        "An app installer. Rarely needed after installing."
    } else if [".zip", ".xip"].iter().any(|end| lower.ends_with(end)) {
        "A zip an app came in. Download it again if needed."
    } else {
        "A cleanup item."
    }
}

impl Pick {
    /// Every item with its checkbox, size, bar, path, and age, its rows
    /// `gap` blank rows apart
    fn draw_table(&mut self, frame: &mut Frame, area: Rect, planned: &Planned, gap: u16) {
        let plan = &planned.plan;
        let block = titled(self.title).title(Line::from(" Goes to the Trash ").right_aligned());
        let found = || plan.rules.iter().flat_map(|rule| &rule.items);
        let sizes = format::column_width("Size", found().map(|item| format::size(item.size)));
        let largest = found().map(|item| item.size).max().unwrap_or(0);
        // Paths are only as wide as the longest, so Changed sits beside them.
        let paths = format::column_width(
            "Path",
            plan.rules
                .iter()
                .flat_map(|rule| {
                    rule.items
                        .iter()
                        .map(|item| item.path.path().to_path_buf())
                        .chain(rule.skipped.iter().map(|skipped| skipped.path.clone()))
                })
                .map(|path| display_path(&planned.home, &path)),
        );
        let mut columns = super::visual::Columns::new(
            area.width,
            [(3, 0), (sizes, 0), (BAR, 0), (16, 1), (14, 0)],
            &[2],
            true,
        );
        if columns.width(3) > usize::from(paths) + 2 {
            columns = super::visual::Columns::new(
                area.width,
                [(3, 0), (sizes, 0), (BAR, 0), (paths + 2, 0), (14, 1)],
                &[2],
                true,
            );
        }
        let now = SystemTime::now();
        let rows: Vec<Row> = plan
            .rules
            .iter()
            .map(|rule| item_row(rule, &planned.home, now, largest, &columns).bottom_margin(gap))
            .collect();
        let header = columns
            .row([
                super::visual::check_header(),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from(""),
                Cell::from("Path"),
                Cell::from("Changed"),
            ])
            .style(super::visual::HEADING)
            .bottom_margin(u16::from(gap > 0));
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.list);
    }

    /// The selected item: where it is, its size and age, and what it is, or
    /// why it is left in place. Always the same number of rows, so the box
    /// keeps its height from row to row.
    fn item_lines(planned: &Planned, index: usize, width: usize) -> (String, Vec<Line<'static>>) {
        let Some(rule) = planned.plan.rules.get(index) else {
            return (String::new(), Vec::new());
        };
        let wrap = |text: &str, color: Option<ratatui::style::Color>| {
            let style = color.map_or_else(ratatui::style::Style::default, |color| {
                ratatui::style::Style::default().fg(color)
            });
            super::visual::hanging(Vec::new(), 0, text, style, width)
        };
        let path_of = |path: &Path| {
            Span::raw(format::shorten_path(
                &place(&planned.home, path),
                width.saturating_sub(LABEL),
            ))
            .fg(super::visual::ACCENT)
        };
        let (title, mut lines, note) = if let Some(item) = rule.items.first() {
            let name = item
                .path
                .path()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let elapsed = SystemTime::now()
                .duration_since(item.changed)
                .unwrap_or_default();
            let changed = if elapsed < RECENT {
                Span::raw(format::age(elapsed)).yellow()
            } else {
                Span::raw(format::age(elapsed))
            };
            let lines = vec![
                field("In", vec![path_of(item.path.path())]),
                field(
                    "Size",
                    vec![format::size_span(item.size, format::size(item.size))],
                ),
                field("Changed", vec![changed]),
            ];
            (name.clone(), lines, wrap(what_it_is(&name), None))
        } else if let Some(skipped) = rule.skipped.first() {
            let name = skipped
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let lines = vec![
                field("In", vec![path_of(&skipped.path)]),
                field("Size", vec![Span::raw("kept").yellow()]),
                field("Changed", vec![Span::raw("")]),
            ];
            let why = skip_reason(&skipped.reason, None);
            (name, lines, wrap(&why, Some(ratatui::style::Color::Yellow)))
        } else {
            return (String::new(), Vec::new());
        };
        lines.push(Line::default());
        let mut note = note;
        while note.len() < 2 {
            note.push(Line::default());
        }
        lines.extend(note);
        (title, lines)
    }

    /// Everything found and selected, and how many changed this week
    fn totals_lines(planned: &Planned) -> Vec<Line<'static>> {
        let plan = &planned.plan;
        let found: Vec<_> = plan.rules.iter().flat_map(|rule| &rule.items).collect();
        let size: u64 = found.iter().map(|item| item.size).sum();
        let now = SystemTime::now();
        let recent = found
            .iter()
            .filter(|item| now.duration_since(item.changed).unwrap_or_default() < RECENT)
            .count();
        let kept = plan
            .rules
            .iter()
            .filter(|rule| rule.items.is_empty())
            .count();
        let selected = Span::raw(format!(
            "{} · {}",
            format::size(plan.selected_size()),
            super::clean::items(plan.selected_count())
        ));
        vec![
            field(
                "Found",
                vec![Span::raw(format!(
                    "{} · {}",
                    format::size(size),
                    super::clean::items(found.len())
                ))],
            ),
            field(
                "Selected",
                vec![if plan.selected_count() == 0 {
                    selected
                } else {
                    selected.green().bold()
                }],
            ),
            field(
                "This week",
                vec![if recent > 0 {
                    Span::raw(super::clean::items(recent)).yellow()
                } else {
                    Span::raw(super::clean::items(0))
                }],
            ),
            field("Kept", vec![Span::raw(super::clean::items(kept))]),
        ]
    }
}

impl Screen for Pick {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        self.poll();
        let block = super::visual::block()
            .title(format!(" {} ", self.title))
            .padding(Padding::horizontal(1));
        let planned = match &self.state {
            State::Planning { started, .. } => {
                Loading {
                    title: self.title,
                    doing: "Measuring",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "View only.",
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
        // As Disk: the table on the left, and the selected item, the
        // totals, and the disk after cleanup stacked on the right. A narrow
        // screen puts them under the table.
        let wide = area.width >= super::visual::SIDE_BY_SIDE;
        let (table, side) = if wide {
            let [table, side] =
                Layout::horizontal([Constraint::Percentage(60), Constraint::Fill(1)]).areas(area);
            (table, side)
        } else {
            let rows = u16::try_from(planned.plan.rules.len()).unwrap_or(u16::MAX) + 3;
            let [table, side] = Layout::vertical([
                Constraint::Length(rows.min(area.height)),
                Constraint::Fill(1),
            ])
            .areas(area);
            (table, side)
        };
        self.draw_table(frame, table, &planned, 0);
        if side.height > 2 {
            let width = side.width.saturating_sub(4);
            let (title, item) = Self::item_lines(
                &planned,
                self.list.selected().unwrap_or(0),
                usize::from(width),
            );
            let mut totals = vec![Line::from("Selected").style(super::visual::HEADING)];
            totals.extend(Self::totals_lines(&planned));
            super::visual::sections(
                frame,
                side,
                &title,
                vec![
                    item,
                    totals,
                    super::visual::after_cleanup(
                        context.disk.as_ref(),
                        planned.plan.selected_size(),
                        "Selected",
                        width,
                    ),
                ],
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        let len = match &self.state {
            State::Ready(planned) => planned.plan.rules.len(),
            _ => 0,
        };
        let end = len.saturating_sub(1);
        let index = self.list.selected().unwrap_or(0);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(index.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => self.list.select(Some((index + 1).min(end))),
            KeyCode::Char('g') | KeyCode::Home => self.list.select(Some(0)),
            KeyCode::Char('G') | KeyCode::End => self.list.select(Some(end)),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Char('a') => self.toggle_all(),
            KeyCode::Enter => {
                if let State::Ready(planned) = &self.state
                    && planned.plan.selected_count() > 0
                {
                    return Action::Open(Box::new(Review::new(Arc::clone(planned))));
                }
            }
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · space pick · a all · enter review · esc back · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("Space", "Select or clear an item"),
            ("a", "Select all, or clear all"),
            ("Enter", "Review every selected path"),
            ("Esc", "Back to Quick Clean"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use std::fs;

    fn render(screen: &mut Pick) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 16)).unwrap();
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

    fn press(screen: &mut Pick, code: KeyCode) -> Action {
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

    #[test]
    fn a_missing_picker_plan_stops_loading() {
        let (sender, result) = mpsc::channel();
        let mut pick = Pick {
            title: "Project build folders",
            state: State::Planning {
                started: Instant::now(),
                result,
            },
            list: TableState::default(),
        };
        pick.poll();
        assert!(matches!(pick.state, State::Planning { .. }));
        drop(sender);
        pick.poll();
        pick.poll();
        assert!(render(&mut pick).contains("No result"));
        assert!(matches!(press(&mut pick, KeyCode::Enter), Action::None));
        assert!(matches!(pick.back(), Action::Back));
    }

    #[test]
    fn queued_picker_results_survive_the_worker_exiting() {
        let home = tempfile::tempdir().unwrap();
        let planned = Planned {
            plan: neet_core::clean::Plan::default(),
            errors: Vec::new(),
            home: home.path().to_path_buf(),
        };
        for outcome in [Ok(planned), Err("Cannot read the home folder.".to_string())] {
            let succeeded = outcome.is_ok();
            let (sender, result) = mpsc::channel();
            sender.send(outcome).unwrap();
            drop(sender);
            let mut pick = Pick {
                title: "Installers",
                state: State::Planning {
                    started: Instant::now(),
                    result,
                },
                list: TableState::default(),
            };
            pick.poll();
            pick.poll();
            if succeeded {
                assert!(matches!(pick.state, State::Ready(_)));
            } else {
                assert!(render(&mut pick).contains("Cannot read the home folder."));
            }
        }
    }

    #[test]
    fn lists_every_folder_selected_and_leads_to_the_review() {
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        let home = fs::canonicalize(dir.path()).expect("home should resolve");
        for project in ["web", "app"] {
            let modules = home.join("Documents").join(project).join("node_modules");
            fs::create_dir_all(&modules).expect("folder should be made");
            fs::write(modules.join("x.js"), "x").expect("file should be written");
        }
        let roots = CleanupRoots::new(&home).expect("roots should be made");
        let plan = clutter::plan(
            &roots,
            Kind::BuildFolders,
            &[
                home.join("Documents/web/node_modules"),
                home.join("Documents/app/node_modules"),
            ],
        );
        let mut pick = Pick::ready(
            "Project build folders",
            Planned {
                plan,
                errors: Vec::new(),
                home,
            },
        );

        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        for (width, height) in crate::ui::visual::tests::SIZES {
            let buffer =
                crate::ui::visual::tests::render("pick", &mut pick, &context, width, height);
            assert!(crate::ui::visual::tests::text(&buffer).contains("node_modules"));
            assert!(crate::ui::visual::tests::text(&buffer).contains("node_modules"));
        }
        // A tall screen: under the table, the selected folder, the totals,
        // and the disk after cleanup
        let buffer = crate::ui::visual::tests::render("pick", &mut pick, &context, 120, 40);
        let tall = crate::ui::visual::tests::text(&buffer);
        for text in [
            "┌ node_modules",
            "JavaScript packages",
            "┌ Selected",
            "┌ After cleanup",
        ] {
            assert!(tall.contains(text), "{text}:\n{tall}");
        }
        // The Selected line of the totals box
        let selected = |pick: &mut Pick| {
            let buffer = crate::ui::visual::tests::render("pick", pick, &context, 120, 40);
            crate::ui::visual::tests::text(&buffer)
                .lines()
                .find(|line| line.contains("│ Selected   "))
                .unwrap_or_default()
                .to_string()
        };
        let text = render(&mut pick);
        assert!(text.contains("web/node_modules"), "{text}");
        assert!(selected(&mut pick).contains("· 2 items"));

        press(&mut pick, KeyCode::Char(' '));
        assert!(selected(&mut pick).contains("· 1 item "));
        press(&mut pick, KeyCode::Char('a'));
        assert!(selected(&mut pick).contains("· 2 items"));
        press(&mut pick, KeyCode::Char('a'));
        assert!(selected(&mut pick).contains("· 0 items"));
        assert!(matches!(press(&mut pick, KeyCode::Enter), Action::None));

        press(&mut pick, KeyCode::Char(' '));
        assert!(matches!(press(&mut pick, KeyCode::Enter), Action::Open(_)));
    }
}
