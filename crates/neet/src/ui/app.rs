use std::path::PathBuf;
use std::time::{Duration, Instant};

use neet_core::disk::{self, DiskSpace};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use super::clean::Estimate;
use super::help::Help;
use super::home::Home;
use super::scan::{ScanStatus, ScanTask};

/// How often to reread the disk's free space.
const DISK_REFRESH: Duration = Duration::from_secs(5);

/// What a screen asks the app to do after handling a key.
pub enum Action {
    None,
    Open(Box<dyn Screen>),
    Back,
    /// Close every screen above Home
    Home,
    Quit,
}

/// Shared state every screen can read while drawing.
pub struct Context<'a> {
    pub scan: &'a ScanStatus,
    /// How full the disk is, if it could be read.
    pub disk: Option<DiskSpace>,
    /// How much every cleanup rule found, once planned
    pub cleanable: Option<u64>,
}

/// One screen on the stack. Home is always at the bottom.
pub trait Screen {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context);

    fn handle_key(&mut self, key: KeyEvent, context: &Context) -> Action;

    /// Key hints shown in the footer.
    fn hints(&self) -> &'static str;

    /// Help text shown by `?`.
    fn help(&self) -> &'static [(&'static str, &'static str)];

    /// What `Esc` does. Most screens go back one step.
    fn back(&mut self) -> Action {
        Action::Back
    }

    /// While a screen takes typed text, every key but `Esc` goes to it, so
    /// `q` and `?` can be typed.
    fn takes_text(&self) -> bool {
        false
    }

    /// A dialog blocks `q`, so a stray key cannot quit mid question.
    fn is_dialog(&self) -> bool {
        false
    }

    /// An overlay is drawn on top of the screen below it.
    fn is_overlay(&self) -> bool {
        false
    }
}

/// The disk's free space, reread every few seconds so the gauge stays current.
struct DiskWatch {
    root: Option<PathBuf>,
    space: Option<DiskSpace>,
    checked: Instant,
}

impl DiskWatch {
    fn new(root: Option<PathBuf>) -> Self {
        let space = root.as_deref().and_then(|root| disk::disk_space(root).ok());
        Self {
            root,
            space,
            checked: Instant::now(),
        }
    }

    fn poll(&mut self) {
        if self.checked.elapsed() < DISK_REFRESH {
            return;
        }
        self.checked = Instant::now();
        if let Some(root) = &self.root {
            self.space = disk::disk_space(root).ok();
        }
    }
}

pub struct App {
    stack: Vec<Box<dyn Screen>>,
    scan: ScanTask,
    /// How much Clean can free, for Home. Planned again after a cleanup.
    estimate: Estimate,
    home: Option<PathBuf>,
    disk: DiskWatch,
    quit: bool,
}

impl App {
    /// `disk_root` is the home folder: the disk to show in the gauge, and
    /// where Clean looks.
    pub fn new(scan: ScanTask, disk_root: Option<PathBuf>) -> Self {
        Self {
            stack: vec![Box::new(Home::new())],
            scan,
            estimate: Estimate::start(disk_root.clone()),
            home: disk_root.clone(),
            disk: DiskWatch::new(disk_root),
            quit: false,
        }
    }

    /// Picks up progress from the background scan, and rereads the disk now and then.
    pub fn poll(&mut self) {
        self.scan.poll();
        self.estimate.poll();
        self.disk.poll();
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    fn top(&mut self) -> &mut dyn Screen {
        self.stack
            .last_mut()
            .expect("Home is never popped")
            .as_mut()
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let [body, footer] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        // Draw from the topmost full screen up, so overlays sit on what is below them.
        let base = self
            .stack
            .iter()
            .rposition(|screen| !screen.is_overlay())
            .unwrap_or(0);
        let context = Context {
            scan: self.scan.status(),
            disk: self.disk.space,
            cleanable: self.estimate.size(),
        };
        for screen in &mut self.stack[base..] {
            screen.draw(frame, body, &context);
        }
        let screen = self.top();
        let hints = Line::from(format!(" {}", screen.hints())).style(Style::new().dark_gray());
        frame.render_widget(hints, footer);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let context = Context {
            scan: self.scan.status(),
            disk: self.disk.space,
            cleanable: self.estimate.size(),
        };
        let screen = self.stack.last_mut().expect("Home is never popped");
        let action = match key.code {
            KeyCode::Esc => screen.back(),
            _ if screen.takes_text() => screen.handle_key(key, &context),
            KeyCode::Char('q') if !screen.is_dialog() => Action::Quit,
            KeyCode::Char('?') => Action::Open(Box::new(Help::new(screen.help()))),
            _ => screen.handle_key(key, &context),
        };
        self.apply(action);
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Open(screen) => self.stack.push(screen),
            Action::Back => {
                if self.stack.len() > 1 {
                    self.stack.pop();
                }
            }
            Action::Home => {
                self.stack.truncate(1);
                // A cleanup may have just run, so the old amount is stale.
                self.estimate = Estimate::start(self.home.clone());
            }
            Action::Quit => self.quit = true,
        }
    }

    #[cfg(test)]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn enter_opens_a_screen_and_esc_goes_back() {
        let mut app = App::new(ScanTask::failed("not scanned in tests"), None);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.depth(), 2);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.depth(), 1);
    }

    #[test]
    fn esc_on_home_stays_on_home() {
        let mut app = App::new(ScanTask::failed("not scanned in tests"), None);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.depth(), 1);
        assert!(!app.should_quit());
    }

    /// A screen that takes typed text, to check `q` and `?` reach it.
    struct Field(String);

    impl Screen for Field {
        fn draw(&mut self, _: &mut Frame, _: Rect, _: &Context) {}
        fn handle_key(&mut self, key: KeyEvent, _: &Context) -> Action {
            if let KeyCode::Char(c) = key.code {
                self.0.push(c);
            }
            Action::None
        }
        fn hints(&self) -> &'static str {
            ""
        }
        fn help(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
        fn takes_text(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_text_field_gets_q_and_question_mark() {
        let mut app = App::new(ScanTask::failed("not scanned in tests"), None);
        app.apply(Action::Open(Box::new(Field(String::new()))));
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Char('?'));
        assert!(!app.should_quit());
        assert_eq!(app.depth(), 2);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.depth(), 1);
    }

    #[test]
    fn q_quits() {
        let mut app = App::new(ScanTask::failed("not scanned in tests"), None);
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit());
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = App::new(ScanTask::failed("not scanned in tests"), None);
        press(&mut app, KeyCode::Char('?'));
        assert_eq!(app.depth(), 2);
    }
}
