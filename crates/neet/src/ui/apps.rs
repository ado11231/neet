use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use neet_core::apps;
use neet_core::clean::RulePlan;
use neet_core::removal::{self, App};
use neet_core::safety::CleanupRoots;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Planned, display_path, skip_reason};
use super::format;
use super::loading::Loading;
use super::review::Review;

fn step(list: &mut ListState, rows: usize, code: KeyCode) {
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

/// The apps in the Applications folders. Apps neet will not remove are
/// dimmed with the reason.
pub struct RemoveApp {
    roots: Option<CleanupRoots>,
    apps: Vec<App>,
    list: ListState,
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
                Self::from_apps(roots, apps, apps::is_running)
            }
            Err(reason) => Self {
                roots: None,
                apps: Vec::new(),
                list: ListState::default(),
                is_running: apps::is_running,
                note: None,
                failed: Some(reason),
            },
        }
    }

    fn from_apps(roots: CleanupRoots, apps: Vec<App>, is_running: fn(&str) -> bool) -> Self {
        Self {
            roots: Some(roots),
            apps,
            list: ListState::default().with_selected(Some(0)),
            is_running,
            note: None,
            failed: None,
        }
    }

    fn open(&mut self) -> Action {
        let (Some(roots), Some(app)) = (
            &self.roots,
            self.list.selected().and_then(|index| self.apps.get(index)),
        ) else {
            return Action::None;
        };
        if let Some(refusal) = app.refused {
            self.note = Some(format!("neet does not remove {}: {refusal}.", app.name));
            return Action::None;
        }
        if app.bundle_id.as_deref().is_some_and(self.is_running) {
            self.note = Some(format!("Quit {} first, then open it here.", app.name));
            return Action::None;
        }
        Action::Open(Box::new(AppFiles::start(roots.clone(), app.clone())))
    }
}

impl Screen for RemoveApp {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        let block = Block::bordered()
            .title(" Remove App ")
            .padding(Padding::horizontal(1));
        if let Some(reason) = &self.failed {
            frame.render_widget(Paragraph::new(reason.clone()).red().block(block), area);
            return;
        }
        let [list_area, note_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(area);
        let rows: Vec<ListItem> = self
            .apps
            .iter()
            .map(|app| {
                let id = app.bundle_id.clone().unwrap_or_default();
                let mut spans = vec![
                    Span::raw(format!("{:<30}", app.name)),
                    Span::raw(format!("{id:<40}")).dark_gray(),
                ];
                match app.refused {
                    Some(refusal) => {
                        spans.push(Span::raw(refusal.to_string()).italic());
                        ListItem::new(Line::from(spans)).dark_gray()
                    }
                    None => ListItem::new(Line::from(spans)),
                }
            })
            .collect();
        let list = List::new(rows)
            .block(block)
            .highlight_symbol("▸ ")
            .highlight_style(Style::new().bold().cyan());
        frame.render_stateful_widget(list, list_area, &mut self.list);
        let note = self.note.clone().map_or_else(
            || {
                Line::from("Only apps directly in /Applications and ~/Applications are listed.")
                    .dark_gray()
            },
            |note| Line::from(note).yellow(),
        );
        frame.render_widget(
            Paragraph::new(note).block(Block::bordered().padding(Padding::horizontal(1))),
            note_area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        self.note = None;
        match key.code {
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.open(),
            code => {
                step(&mut self.list, self.apps.len(), code);
                Action::None
            }
        }
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · enter open · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("g  G", "Jump to the first or last app"),
            ("Enter  →  l", "List the app and its files"),
            ("Esc", "Go back to Home"),
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

/// One app and the files that belong to it, each selected as SAFETY.md
/// says. `Enter` opens the same review as Clean.
pub struct AppFiles {
    name: String,
    bundle_id: String,
    state: State,
    list: ListState,
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
            list: ListState::default().with_selected(Some(0)),
        }
    }

    #[cfg(test)]
    fn ready(name: &str, bundle_id: &str, planned: Planned) -> Self {
        Self {
            name: name.to_string(),
            bundle_id: bundle_id.to_string(),
            state: State::Ready(Arc::new(planned)),
            list: ListState::default().with_selected(Some(0)),
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

fn file_row(rule: &RulePlan, home: &Path) -> ListItem<'static> {
    let mark = if rule.selected { "[x] " } else { "[ ] " };
    let (path, size, note) = match (rule.items.first(), rule.skipped.first()) {
        (Some(item), _) => (
            display_path(home, item.path.path()),
            format::size(item.size),
            rule.rule.description.clone(),
        ),
        (None, Some(skipped)) => (
            display_path(home, &skipped.path),
            String::new(),
            format!("skipped: {}", skip_reason(&skipped.reason, None)),
        ),
        (None, None) => (String::new(), String::new(), String::new()),
    };
    let line = Line::from(vec![
        Span::raw(mark),
        Span::raw(format!("{size:>10}  ")),
        Span::raw(path),
        Span::raw(if note.is_empty() {
            String::new()
        } else {
            format!("  {note}")
        })
        .yellow(),
    ]);
    if rule.items.is_empty() {
        ListItem::new(line).dark_gray()
    } else {
        ListItem::new(line)
    }
}

impl Screen for AppFiles {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        let title = format!(" {} ({}) ", self.name, self.bundle_id);
        let block = Block::bordered()
            .title(title)
            .padding(Padding::horizontal(1));
        let planned = match &self.state {
            State::Planning { started, .. } => {
                Loading {
                    title: &self.name,
                    doing: "Finding the app's files and measuring them",
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
        let block = block.title_bottom(
            Line::from(format!(
                " Selected: {} items, {} · they go to the Trash, where Put Back works ",
                format::count(u64::try_from(plan.selected_count()).unwrap_or(u64::MAX)),
                format::size(plan.selected_size())
            ))
            .bold(),
        );
        let rows: Vec<ListItem> = plan
            .rules
            .iter()
            .map(|rule| file_row(rule, &planned.home))
            .collect();
        let list = List::new(rows)
            .block(block)
            .highlight_symbol("▸ ")
            .highlight_style(Style::new().bold().cyan());
        frame.render_stateful_widget(list, area, &mut self.list);
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
            ("↑ ↓  j k", "Move the selection"),
            ("Space", "Select or clear a file"),
            ("Enter", "Review every selected path"),
            ("Esc", "Go back to the app list"),
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
    fn lists_apps_and_dims_refused_ones() {
        let (_home, _shared, roots, apps) = setup();
        let mut screen = RemoveApp::from_apps(roots, apps, |_| false);

        let text = render(&mut screen);

        assert!(text.contains("Example"));
        assert!(text.contains("com.example.app"));
        assert!(text.contains("Apple app"));
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
        assert!(text.contains("[x]"));
        assert!(text.contains("~/Library/Caches/com.example.app"));
        assert!(text.contains("[ ]"));
        assert!(text.contains("may be your data"));
        assert!(text.contains("Selected: 2 items"));

        press(&mut files, KeyCode::Down);
        press(&mut files, KeyCode::Down);
        press(&mut files, KeyCode::Char(' '));
        assert!(render(&mut files).contains("Selected: 3 items"));

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
            if text.contains("Selected:") {
                break;
            }
            assert!(Instant::now() < deadline, "planning should finish");
            thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
