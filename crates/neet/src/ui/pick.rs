use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
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
        if let State::Planning { result, .. } = &self.state
            && let Ok(outcome) = result.try_recv()
        {
            self.state = match outcome {
                Ok(planned) => State::Ready(Arc::new(planned)),
                Err(reason) => State::Failed(reason),
            };
        }
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

fn item_row(
    rule: &RulePlan,
    home: &Path,
    now: SystemTime,
    columns: &super::visual::Columns<4>,
) -> Row<'static> {
    let width = columns.width(2);
    let Some(item) = rule.items.first() else {
        let (path, why) = rule.skipped.first().map_or_else(
            || (String::new(), String::new()),
            |skipped| {
                (
                    display_path(home, &skipped.path),
                    skip_reason(&skipped.reason, None),
                )
            },
        );
        return columns.row([
            Cell::from(checkbox(false, false)),
            Cell::from(""),
            Cell::from(format::shorten_path(&path, width)),
            Cell::from(Span::raw(why).yellow()),
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
        "JavaScript packages. Running npm install brings them back."
    } else if lower == "target" {
        "Rust build files. Your next cargo build makes them again."
    } else if [".dmg", ".pkg", ".iso"]
        .iter()
        .any(|end| lower.ends_with(end))
    {
        "An app installer. You rarely need it after installing."
    } else if [".zip", ".xip"].iter().any(|end| lower.ends_with(end)) {
        "A zip file an app came in. Download it again if you need it."
    } else {
        "An item this cleanup found."
    }
}

impl Pick {
    /// Every item with its checkbox, size, path, and age, and what is
    /// selected on the bottom edge
    fn draw_table(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        planned: &Planned,
        block: ratatui::widgets::Block<'static>,
    ) {
        let plan = &planned.plan;
        let selected = Span::raw(format!(
            " Selected: {} · {} ",
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

        let sizes = format::column_width(
            "Size",
            plan.rules
                .iter()
                .flat_map(|rule| &rule.items)
                .map(|item| format::size(item.size)),
        );
        // Paths are only as wide as the longest, so Changed sits beside them.
        let paths = format::column_width(
            "Path",
            plan.rules
                .iter()
                .flat_map(|rule| &rule.items)
                .map(|item| display_path(&planned.home, item.path.path())),
        );
        let mut columns = super::visual::Columns::new(
            area.width,
            [(3, 0), (sizes, 0), (16, 1), (14, 0)],
            &[],
            true,
        );
        if columns.width(2) > usize::from(paths) + 2 {
            columns = super::visual::Columns::new(
                area.width,
                [(3, 0), (sizes, 0), (paths + 2, 0), (14, 0)],
                &[],
                true,
            );
        }
        let now = SystemTime::now();
        let rows: Vec<Row> = plan
            .rules
            .iter()
            .map(|rule| item_row(rule, &planned.home, now, &columns))
            .collect();
        let header = columns
            .row([
                Cell::from(""),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from("Path"),
                Cell::from("Changed"),
            ])
            .style(super::visual::HEADING)
            .bottom_margin(1);
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.list);
    }

    /// The selected item: where it is, its size and age, and what it is
    fn item_lines(planned: &Planned, index: usize, width: usize) -> (String, Vec<Line<'static>>) {
        let field = |label: &str, value: Span<'static>| {
            Line::from(vec![Span::raw(format!("{label:<10}")).bold(), value])
        };
        let Some(rule) = planned.plan.rules.get(index) else {
            return (String::new(), Vec::new());
        };
        let Some(item) = rule.items.first() else {
            let Some(skipped) = rule.skipped.first() else {
                return (String::new(), Vec::new());
            };
            let path = display_path(&planned.home, &skipped.path);
            return (
                " Left in place ".to_string(),
                vec![
                    field(
                        "Path",
                        Span::raw(format::shorten_path(&path, width.saturating_sub(10)))
                            .fg(super::visual::ACCENT),
                    ),
                    field(
                        "Why",
                        Span::raw(skip_reason(&skipped.reason, None)).yellow(),
                    ),
                ],
            );
        };
        let name = item
            .path
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = place(&planned.home, item.path.path());
        let elapsed = SystemTime::now()
            .duration_since(item.changed)
            .unwrap_or_default();
        let mut changed = vec![field("Changed", Span::raw(format::age(elapsed)))];
        if elapsed < RECENT {
            changed = vec![field(
                "Changed",
                Span::raw(format!(
                    "{}, you may still be using it",
                    format::age(elapsed)
                ))
                .yellow(),
            )];
        }
        let mut lines = vec![
            field(
                "In",
                Span::raw(format::shorten_path(&folder, width.saturating_sub(10)))
                    .fg(super::visual::ACCENT),
            ),
            field(
                "Size",
                format::size_span(item.size, format::size(item.size)),
            ),
        ];
        lines.extend(changed);
        lines.push(field(
            "Selected",
            if rule.selected {
                Span::raw("yes").green()
            } else {
                Span::raw("no")
            },
        ));
        lines.push(Line::default());
        lines.push(Line::from(what_it_is(&name)));
        (format!(" {name} "), lines)
    }

    /// Everything found and selected, and how many changed this week
    fn totals_lines(planned: &Planned) -> Vec<Line<'static>> {
        let field = |label: &str, value: Span<'static>| {
            Line::from(vec![Span::raw(format!("{label:<10}")).bold(), value])
        };
        let plan = &planned.plan;
        let found: Vec<_> = plan.rules.iter().flat_map(|rule| &rule.items).collect();
        let size: u64 = found.iter().map(|item| item.size).sum();
        let now = SystemTime::now();
        let recent = found
            .iter()
            .filter(|item| now.duration_since(item.changed).unwrap_or_default() < RECENT)
            .count();
        let selected = Span::raw(format!(
            "{} · {}",
            format::size(plan.selected_size()),
            super::clean::items(plan.selected_count())
        ));
        let mut lines = vec![
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
        ];
        if recent > 0 {
            lines.push(field(
                "Recent",
                Span::raw(format!(
                    "{} changed this week. Unpick ones you still use.",
                    super::clean::items(recent)
                ))
                .yellow(),
            ));
        }
        lines
    }

    fn how_lines() -> Vec<Line<'static>> {
        let step = |number: usize, text: &'static str| {
            Line::from(vec![
                Span::raw(format!("{number}. "))
                    .fg(super::visual::ACCENT)
                    .bold(),
                Span::raw(text),
            ])
        };
        vec![
            Line::from("Steps").style(super::visual::HEADING),
            step(1, "Press Space to pick or unpick one, or a for all."),
            step(2, "Press Enter to check the list."),
            step(3, "Confirm to move them to the Trash."),
        ]
    }
}

impl Screen for Pick {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        let block = super::visual::block()
            .title(format!(" {} ", self.title))
            .padding(Padding::horizontal(1));
        let planned = match &self.state {
            State::Planning { started, .. } => {
                Loading {
                    title: self.title,
                    doing: "Measuring items and checking paths",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Read-only scan.",
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
        // The table, only as tall as its rows, then a box about the selected
        // item below, when there is room for both
        let rows = u16::try_from(planned.plan.rules.len()).unwrap_or(u16::MAX);
        let (area, below) = if area.height >= rows + 4 + 8 {
            let [table, below] = Layout::vertical([
                Constraint::Length((rows + 4).min(area.height - 8)),
                Constraint::Fill(1),
            ])
            .areas(area);
            (table, Some(below))
        } else {
            (area, None)
        };
        self.draw_table(frame, area, &planned, block);
        if let Some(below) = below {
            let width = usize::from(below.width.saturating_sub(4));
            let (title, item) =
                Self::item_lines(&planned, self.list.selected().unwrap_or(0), width);
            super::visual::sections(
                frame,
                below,
                &title,
                vec![item, Self::totals_lines(&planned), Self::how_lines()],
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
        "↑↓ move · space select · a toggle all · enter review · esc back · ? help"
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
            assert!(crate::ui::visual::tests::text(&buffer).contains("Selected: 2 items"));
        }
        // A tall screen: the table fits its rows, and the box below says
        // what the selected folder is, the totals, and the steps.
        let buffer = crate::ui::visual::tests::render("pick", &mut pick, &context, 120, 40);
        let tall = crate::ui::visual::tests::text(&buffer);
        for text in [
            "┌ node_modules",
            "JavaScript packages",
            "┌ Total",
            "┌ Steps",
        ] {
            assert!(tall.contains(text), "{text}:\n{tall}");
        }
        let text = render(&mut pick);
        assert!(text.contains("~/Documents/web/node_modules"));
        assert!(text.contains("Selected: 2 items"));

        press(&mut pick, KeyCode::Char(' '));
        assert!(render(&mut pick).contains("Selected: 1 item "));
        press(&mut pick, KeyCode::Char('a'));
        assert!(render(&mut pick).contains("Selected: 2 items"));
        press(&mut pick, KeyCode::Char('a'));
        assert!(render(&mut pick).contains("Selected: 0 items"));
        assert!(matches!(press(&mut pick, KeyCode::Enter), Action::None));

        press(&mut pick, KeyCode::Char(' '));
        assert!(matches!(press(&mut pick, KeyCode::Enter), Action::Open(_)));
    }
}
