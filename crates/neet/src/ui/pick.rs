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
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

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

fn item_row(rule: &RulePlan, home: &Path, now: SystemTime, width: usize) -> Row<'static> {
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
        return Row::new([
            Cell::from(checkbox(false, false)),
            Cell::from(""),
            Cell::from(format::shorten_path(&path, width)),
            Cell::from(Span::raw(why).yellow()),
        ])
        .dark_gray();
    };
    let name = item
        .path
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = place(home, item.path.path());
    let room = width.saturating_sub(name.chars().count() + 1);
    let elapsed = now.duration_since(item.changed).unwrap_or_default();
    // Changed this week: you may be working in it
    let age = if elapsed < RECENT {
        Span::raw(format::age(elapsed)).yellow()
    } else {
        Span::raw(format::age(elapsed))
    };
    Row::new([
        Cell::from(checkbox(rule.selected, true)),
        Cell::from(
            Line::from(format::size_span(item.size, format::size(item.size))).right_aligned(),
        ),
        Cell::from(Line::from(vec![
            Span::raw(format!("{}/", format::shorten_path(&folder, room))).blue(),
            Span::raw(name),
        ])),
        Cell::from(age),
    ])
}

impl Screen for Pick {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        let block = Block::bordered()
            .title(format!(" {} ", self.title))
            .padding(Padding::horizontal(1));
        let planned = match &self.state {
            State::Planning { started, .. } => {
                Loading {
                    title: self.title,
                    doing: "Measuring each item and checking it is safe to move",
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
            .title_bottom(
                Line::from(" Everything goes to the Trash, where Put Back works ").right_aligned(),
            );

        // Checkbox, size, and age are fixed; the path takes the rest.
        let path_width = usize::from(area.width.saturating_sub(4 + 2 + 3 + 9 + 14 + 2 * 3));
        let now = SystemTime::now();
        let rows: Vec<Row> = plan
            .rules
            .iter()
            .map(|rule| item_row(rule, &planned.home, now, path_width))
            .collect();
        let header = Row::new([
            Cell::from(""),
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from("Path"),
            Cell::from("Changed"),
        ])
        .bold()
        .bottom_margin(1);
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(9),
                Constraint::Fill(1),
                Constraint::Length(14),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(block)
        .highlight_symbol("▸ ")
        .row_highlight_style(Style::new().bold());
        frame.render_stateful_widget(table, area, &mut self.list);
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
        "↑↓ move · space select · a all or none · enter review · esc back · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("Space", "Select or clear an item"),
            ("a", "Select all, or clear all"),
            ("Enter", "Review every selected path"),
            ("Esc", "Go back to Quick Clean"),
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
