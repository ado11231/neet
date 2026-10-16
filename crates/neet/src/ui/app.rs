use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use neet_core::disk::{self, DiskSpace};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};

use super::clean::Estimate;
use super::help::Help;
use super::home::Home;
use super::scan::{ScanStatus, ScanTask};

/// How often to reread the disk's free space.
const DISK_REFRESH: Duration = Duration::from_secs(5);

/// How often to ask macOS for purgeable space, which takes longer to read.
const PURGEABLE_REFRESH: Duration = Duration::from_secs(60);

/// What a screen asks the app to do after handling a key.
pub enum Action {
    None,
    Open(Box<dyn Screen>),
    Back,
    /// Close every screen above Home
    Home,
    /// Look again for what the cleanup rules cover
    Replan,
    /// Leave the screen to open this file in your editor, then tell the
    /// screen with [`Screen::edited`]
    Edit(PathBuf),
    /// Leave the screen to run these programs in turn, such as an assistant
    /// on a chat or `sudo pmset`, then tell the screen with [`Screen::ran`]
    Run(Vec<neet_core::program::Program>),
    Quit,
}

/// Shared state every screen can read while drawing.
pub struct Context<'a> {
    pub scan: &'a ScanStatus,
    /// How full the disk is, if it could be read.
    pub disk: Option<DiskSpace>,
    /// How much every cleanup rule found, once planned
    pub cleanable: Option<u64>,
    /// Every rule's plan, made in the background, for Clean to open on
    pub plan: Option<&'a Estimate>,
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

    /// Your editor closed after [`Action::Edit`], or could not open.
    fn edited(&mut self, _result: Result<(), String>) {}

    /// The program from [`Action::Run`] quit, or could not start.
    fn ran(&mut self, _result: Result<(), String>) {}
}

/// The disk's free space, reread every few seconds so the gauge stays current.
/// Purgeable space is asked for less often, on its own thread.
struct DiskWatch {
    root: Option<PathBuf>,
    space: Option<DiskSpace>,
    checked: Instant,
    purgeable: Option<u64>,
    asked: Option<Instant>,
    answer: Option<Receiver<Option<u64>>>,
}

impl DiskWatch {
    fn new(root: Option<PathBuf>) -> Self {
        let space = root.as_deref().and_then(|root| disk::disk_space(root).ok());
        let mut watch = Self {
            root,
            space,
            checked: Instant::now(),
            purgeable: None,
            asked: None,
            answer: None,
        };
        watch.ask_purgeable();
        watch
    }

    /// The disk space, with the latest purgeable space.
    fn space(&self) -> Option<DiskSpace> {
        self.space.map(|space| DiskSpace {
            purgeable: self.purgeable,
            ..space
        })
    }

    /// Purgeable space is Finder's free space minus the free space now, both
    /// read on the same thread so they match.
    fn ask_purgeable(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let (sender, answer) = mpsc::channel();
        thread::spawn(move || {
            let purgeable = disk::disk_space(&root).ok().and_then(|space| {
                disk::finder_free(&root)
                    .ok()
                    .map(|free| free.saturating_sub(space.available))
            });
            let _ = sender.send(purgeable);
        });
        self.asked = Some(Instant::now());
        self.answer = Some(answer);
    }

    fn poll(&mut self) {
        if let Some(answer) = &self.answer {
            match answer.try_recv() {
                Ok(purgeable) => {
                    self.purgeable = purgeable;
                    self.answer = None;
                }
                // The worker died without answering, so the old value is
                // stale. Ask again after the usual wait.
                Err(TryRecvError::Disconnected) => {
                    self.purgeable = None;
                    self.answer = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.answer.is_none()
            && self
                .asked
                .is_some_and(|asked| asked.elapsed() >= PURGEABLE_REFRESH)
        {
            self.ask_purgeable();
        }
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
    /// A file to open in your editor, once the screen is left
    edit: Option<PathBuf>,
    /// A program to run, once the screen is left
    run: Option<Vec<neet_core::program::Program>>,
    quit: bool,
}

impl App {
    /// `disk_root` is the home folder: the disk to show in the gauge, and
    /// where Clean looks.
    pub fn new(scan: ScanTask, disk_root: Option<PathBuf>) -> Self {
        if let Some(home) = &disk_root {
            super::apps::measure_at_start(home.clone());
        }
        Self {
            stack: vec![Box::new(Home::new())],
            scan,
            estimate: Estimate::start(disk_root.clone()),
            home: disk_root.clone(),
            disk: DiskWatch::new(disk_root),
            edit: None,
            run: None,
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

    /// The file a screen asked to open in your editor, if any
    pub fn take_edit(&mut self) -> Option<PathBuf> {
        self.edit.take()
    }

    /// Tells the screen that asked how the editor went.
    pub fn edited(&mut self, result: Result<(), String>) {
        self.top().edited(result);
    }

    /// The program a screen asked to run, if any
    pub fn take_run(&mut self) -> Option<Vec<neet_core::program::Program>> {
        self.run.take()
    }

    /// Tells the screen that asked how the program went.
    pub fn ran(&mut self, result: Result<(), String>) {
        self.top().ran(result);
    }

    fn top(&mut self) -> &mut dyn Screen {
        self.stack
            .last_mut()
            .expect("Home is never popped")
            .as_mut()
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let hints = super::visual::hints(self.top().hints());
        let footer_height =
            super::visual::wrapped_rows(std::slice::from_ref(&hints), frame.area().width);
        let [body, footer] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_height)])
                .areas(frame.area());
        // Draw from the topmost full screen up, so overlays sit on what is below them.
        let base = self
            .stack
            .iter()
            .rposition(|screen| !screen.is_overlay())
            .unwrap_or(0);
        let context = Context {
            scan: self.scan.status(),
            disk: self.disk.space(),
            cleanable: self.estimate.size(),
            plan: Some(&self.estimate),
        };
        super::loading::start_frame();
        for screen in &mut self.stack[base..] {
            screen.draw(frame, body, &context);
        }
        if super::loading::shown() {
            return;
        }
        frame.render_widget(
            ratatui::widgets::Paragraph::new(hints).wrap(ratatui::widgets::Wrap { trim: false }),
            footer,
        );
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let busy = self.stack.iter().any(|screen| screen.is_dialog());
        let context = Context {
            scan: self.scan.status(),
            disk: self.disk.space(),
            cleanable: self.estimate.size(),
            plan: Some(&self.estimate),
        };
        let screen = self.stack.last_mut().expect("Home is never popped");
        let action = match key.code {
            KeyCode::Esc => screen.back(),
            _ if screen.takes_text() => screen.handle_key(key, &context),
            KeyCode::Char('q') if !busy => Action::Quit,
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
            Action::Replan => self.estimate = Estimate::start(self.home.clone()),
            Action::Edit(path) => self.edit = Some(path),
            Action::Run(programs) => self.run = Some(programs),
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
    fn a_lost_disk_result_clears_the_old_value_and_allows_retry() {
        let mut watch = DiskWatch::new(None);
        let (sender, answer) = mpsc::channel();
        watch.answer = Some(answer);
        watch.purgeable = Some(500);
        watch.asked = Some(Instant::now());
        watch.poll();
        assert!(watch.answer.is_some());
        assert_eq!(watch.purgeable, Some(500));
        drop(sender);
        watch.poll();
        assert!(watch.answer.is_none());
        assert_eq!(watch.purgeable, None);

        let dir = tempfile::tempdir().unwrap();
        watch.root = Some(dir.path().join("missing"));
        watch.asked = Instant::now().checked_sub(PURGEABLE_REFRESH);
        watch.poll();
        assert!(watch.answer.is_some());
        assert!(watch.asked.unwrap().elapsed() < PURGEABLE_REFRESH);
    }

    #[test]
    fn queued_disk_results_survive_the_worker_exiting() {
        for result in [Some(123), None] {
            let mut watch = DiskWatch::new(None);
            watch.purgeable = Some(500);
            let (sender, answer) = mpsc::channel();
            watch.answer = Some(answer);
            sender.send(result).unwrap();
            drop(sender);
            watch.poll();
            watch.poll();
            assert_eq!(watch.purgeable, result);
            assert!(watch.answer.is_none());
        }
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

    struct Busy(std::rc::Rc<std::cell::Cell<bool>>);

    impl Screen for Busy {
        fn draw(&mut self, _: &mut Frame, _: Rect, _: &Context) {}
        fn handle_key(&mut self, _: KeyEvent, _: &Context) -> Action {
            Action::None
        }
        fn hints(&self) -> &'static str {
            ""
        }
        fn help(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
        fn is_dialog(&self) -> bool {
            self.0.get()
        }
    }

    #[test]
    fn help_keeps_quit_blocked_until_the_underlying_work_finishes() {
        let busy = std::rc::Rc::new(std::cell::Cell::new(true));
        let mut app = App::new(ScanTask::failed("not scanned in tests"), None);
        app.apply(Action::Open(Box::new(Busy(busy.clone()))));
        for depth in 2..=4 {
            assert_eq!(app.depth(), depth);
            press(&mut app, KeyCode::Char('q'));
            assert!(!app.should_quit());
            press(&mut app, KeyCode::Char('?'));
        }
        busy.set(false);
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit());
    }
}
