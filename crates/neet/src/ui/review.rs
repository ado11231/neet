use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::SystemTime;

use neet_core::apps;
use neet_core::clean::{self, Outcome};
use neet_core::safety::CleanupRoots;
use neet_core::trash;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Planned, count, display_path, skip_reason};
use super::format;

/// A scrolling position that stops at the last line once drawn.
#[derive(Default)]
struct Scroll(u16);

impl Scroll {
    fn handle(&mut self, code: KeyCode) {
        self.0 = match code {
            KeyCode::Up | KeyCode::Char('k') => self.0.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.0.saturating_add(1),
            KeyCode::PageUp => self.0.saturating_sub(10),
            KeyCode::PageDown => self.0.saturating_add(10),
            KeyCode::Char('g') | KeyCode::Home => 0,
            KeyCode::Char('G') | KeyCode::End => u16::MAX,
            _ => self.0,
        };
    }

    fn clamp(&mut self, lines: usize, area: Rect) -> u16 {
        let visible = area.height.saturating_sub(2);
        let last = u16::try_from(lines)
            .unwrap_or(u16::MAX)
            .saturating_sub(visible);
        self.0 = self.0.min(last);
        self.0
    }
}

fn scrolling(
    frame: &mut Frame,
    area: Rect,
    title: String,
    lines: Vec<Line<'static>>,
    scroll: &mut Scroll,
) {
    let offset = scroll.clamp(lines.len(), area);
    let body = Paragraph::new(lines).scroll((offset, 0)).block(
        Block::bordered()
            .title(title)
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(body, area);
}

const SCROLL_HELP: [(&str, &str); 3] = [
    ("↑ ↓  j k", "Scroll"),
    ("PgUp PgDn", "Scroll a page"),
    ("g  G", "Jump to the top or bottom"),
];

/// Every path the selected rules found. It can only be left by going back,
/// or by moving on to the question.
pub struct Review {
    planned: Arc<Planned>,
    scroll: Scroll,
}

impl Review {
    pub fn new(planned: Arc<Planned>) -> Self {
        Self {
            planned,
            scroll: Scroll::default(),
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let home = &self.planned.home;
        let mut lines = Vec::new();
        for rule in self.planned.plan.rules.iter().filter(|rule| rule.selected) {
            lines.push(Line::from(vec![
                Span::raw(rule.rule.name.clone()).bold(),
                Span::raw(format!(
                    "  {} items · {}",
                    count(rule.items.len()),
                    format::size(rule.size())
                ))
                .dark_gray(),
            ]));
            for item in &rule.items {
                lines.push(Line::from(vec![
                    Span::raw(format!("{:>10}  ", format::size(item.size))),
                    Span::raw(display_path(home, item.path.path())),
                ]));
            }
            lines.push(Line::default());
        }
        lines
    }
}

impl Screen for Review {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        let plan = &self.planned.plan;
        let title = format!(
            " Review: {} items, {} ",
            count(plan.selected_count()),
            format::size(plan.selected_size())
        );
        let lines = self.lines();
        scrolling(frame, area, title, lines, &mut self.scroll);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if key.code == KeyCode::Enter {
            return Action::Open(Box::new(Confirm::new(Arc::clone(&self.planned))));
        }
        self.scroll.handle(key.code);
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ scroll · enter continue · esc back · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            SCROLL_HELP[0],
            SCROLL_HELP[1],
            SCROLL_HELP[2],
            ("Enter", "Go on to the question"),
            ("Esc", "Go back to Clean"),
            ("q", "Quit"),
        ]
    }
}

/// The question before anything moves.
pub struct Confirm {
    planned: Arc<Planned>,
}

impl Confirm {
    pub fn new(planned: Arc<Planned>) -> Self {
        Self { planned }
    }
}

impl Screen for Confirm {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        let plan = &self.planned.plan;
        let lines = vec![
            Line::from(format!(
                "Move {} items, {}, to the Trash?",
                count(plan.selected_count()),
                format::size(plan.selected_size())
            ))
            .bold(),
            Line::default(),
            Line::from("Finder moves them, so Put Back can restore each one."),
            Line::from("Each item is checked again right before it moves."),
            Line::default(),
            Line::from(vec![
                Span::raw("y").bold().cyan(),
                Span::raw(" move to the Trash    "),
                Span::raw("n").bold(),
                Span::raw(" or "),
                Span::raw("Esc").bold(),
                Span::raw(" go back"),
            ]),
        ];
        let [area] = Layout::vertical([Constraint::Length(8)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(60)])
            .flex(Flex::Center)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" Move to the Trash ")
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Char('y') => Action::Open(Box::new(Cleanup::start(Arc::clone(&self.planned)))),
            KeyCode::Char('n') => Action::Back,
            _ => Action::None,
        }
    }

    fn hints(&self) -> &'static str {
        "y move · n back"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("y", "Move the items to the Trash"),
            ("n  Esc", "Go back to the review"),
        ]
    }

    fn is_dialog(&self) -> bool {
        true
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

enum Message {
    Progress(usize, usize),
    Done(Result<Outcome, String>),
}

enum State {
    Moving {
        done: usize,
        total: usize,
        messages: Receiver<Message>,
    },
    Done(Result<Outcome, String>),
}

/// Moves the items on its own thread, then shows what happened.
pub struct Cleanup {
    planned: Arc<Planned>,
    state: State,
    scroll: Scroll,
}

impl Cleanup {
    pub fn start(planned: Arc<Planned>) -> Self {
        Self::start_with(planned, apps::is_running, trash::move_to_trash)
    }

    fn start_with(
        planned: Arc<Planned>,
        is_running: fn(&str) -> bool,
        move_item: fn(&Path) -> io::Result<()>,
    ) -> Self {
        let (sender, messages) = mpsc::channel();
        let shared = Arc::clone(&planned);
        thread::spawn(move || {
            let result = CleanupRoots::new(&shared.home)
                .map_err(|error| format!("The home folder could not be read: {error}"))
                .map(|roots| {
                    clean::run(
                        &shared.plan,
                        &roots,
                        SystemTime::now(),
                        is_running,
                        move_item,
                        |done, total| {
                            let _ = sender.send(Message::Progress(done, total));
                        },
                    )
                });
            let _ = sender.send(Message::Done(result));
        });
        Self {
            state: State::Moving {
                done: 0,
                total: planned.plan.selected_count(),
                messages,
            },
            planned,
            scroll: Scroll::default(),
        }
    }

    fn poll(&mut self) {
        let State::Moving {
            done,
            total,
            messages,
        } = &mut self.state
        else {
            return;
        };
        let mut finished = None;
        while let Ok(message) = messages.try_recv() {
            match message {
                Message::Progress(now, of) => (*done, *total) = (now, of),
                Message::Done(result) => finished = Some(result),
            }
        }
        if let Some(result) = finished {
            self.state = State::Done(result);
        }
    }

    fn is_moving(&self) -> bool {
        matches!(self.state, State::Moving { .. })
    }

    fn result_lines(&self, outcome: &Outcome) -> Vec<Line<'static>> {
        let home = &self.planned.home;
        let mut lines = vec![
            Line::from(format!(
                "Moved {} items to the Trash.",
                count(outcome.moved.len())
            ))
            .bold()
            .green(),
            Line::from(format!(
                "They take up {} in the Trash. Emptying the Trash frees that space.",
                format::size(outcome.moved_size())
            )),
            Line::from("To restore an item, select it in the Trash and choose Put Back.")
                .dark_gray(),
        ];
        if !outcome.skipped.is_empty() {
            lines.push(Line::default());
            lines.push(
                Line::from(format!(
                    "Skipped {} items, left where they were:",
                    count(outcome.skipped.len())
                ))
                .yellow()
                .bold(),
            );
            for skipped in &outcome.skipped {
                lines.push(Line::from(vec![
                    Span::raw(display_path(home, &skipped.path)),
                    Span::raw(format!("  {}", skip_reason(&skipped.reason, None))).dark_gray(),
                ]));
            }
        }
        lines
    }
}

impl Screen for Cleanup {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        match &self.state {
            State::Moving { done, total, .. } => {
                let [text, gauge, rest] = Layout::vertical([
                    Constraint::Length(4),
                    Constraint::Length(3),
                    Constraint::Fill(1),
                ])
                .areas(area);
                let lines = vec![
                    Line::from("Moving items to the Trash…").cyan().bold(),
                    Line::from(
                        "The first time, macOS asks whether your terminal may control Finder.",
                    )
                    .dark_gray(),
                ];
                frame.render_widget(
                    Paragraph::new(lines).block(
                        Block::bordered()
                            .title(" Cleanup ")
                            .padding(Padding::horizontal(1)),
                    ),
                    text,
                );
                let ratio = if *total == 0 {
                    1.0
                } else {
                    #[allow(clippy::cast_precision_loss)] // Only drawn as a bar.
                    let ratio = *done as f64 / *total as f64;
                    ratio
                };
                frame.render_widget(
                    Gauge::default()
                        .block(Block::bordered())
                        .ratio(ratio.clamp(0.0, 1.0))
                        .label(format!("{} of {}", count(*done), count(*total))),
                    gauge,
                );
                frame.render_widget(Block::default(), rest);
            }
            State::Done(Err(reason)) => {
                let body = Paragraph::new(format!("Nothing was moved. {reason}"))
                    .red()
                    .block(Block::bordered().title(" Cleanup "));
                frame.render_widget(body, area);
            }
            State::Done(Ok(outcome)) => {
                let lines = self.result_lines(outcome);
                scrolling(
                    frame,
                    area,
                    " Cleanup done ".to_string(),
                    lines,
                    &mut self.scroll,
                );
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if self.is_moving() {
            return Action::None;
        }
        if key.code == KeyCode::Enter {
            return Action::Home;
        }
        self.scroll.handle(key.code);
        Action::None
    }

    fn back(&self) -> Action {
        if self.is_moving() {
            Action::None
        } else {
            Action::Home
        }
    }

    fn hints(&self) -> &'static str {
        if self.is_moving() {
            "moving to the Trash, please wait"
        } else {
            "↑↓ scroll · enter or esc home · q quit"
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            SCROLL_HELP[0],
            SCROLL_HELP[1],
            SCROLL_HELP[2],
            ("Enter  Esc", "Go back to Home when done"),
            ("q", "Quit, when done"),
        ]
    }

    /// Blocks `q` until every item is dealt with.
    fn is_dialog(&self) -> bool {
        self.is_moving()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use neet_core::clean::plan;
    use neet_core::rules::{self, Tier};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use std::fs;
    use std::time::{Duration, Instant};
    use tempfile::{TempDir, tempdir};

    fn planned() -> (TempDir, Arc<Planned>) {
        let dir = tempdir().expect("temporary directory should be created");
        for file in [
            "Library/Developer/Xcode/DerivedData/App-abc/out",
            ".npm/_cacache/content-v2/aa",
        ] {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().expect("path should have a parent"))
                .expect("folder should be created");
            fs::write(path, vec![1; 5000]).expect("file should be written");
        }
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        let set = rules::load(&roots, None);
        let plan = plan(&set.rules, &roots, SystemTime::now());
        let planned = Planned {
            plan,
            errors: Vec::new(),
            home: roots.home().to_path_buf(),
        };
        (dir, Arc::new(planned))
    }

    fn render(screen: &mut dyn Screen) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
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
            },
        )
    }

    fn finish(cleanup: &mut Cleanup) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while cleanup.is_moving() {
            assert!(Instant::now() < deadline, "cleanup should finish");
            thread::sleep(Duration::from_millis(5));
            cleanup.poll();
        }
    }

    #[test]
    fn review_lists_every_path_of_the_selected_rules_only() {
        let (_dir, planned) = planned();
        let screen = render(&mut Review::new(planned));

        assert!(screen.contains("Review: 1 items"));
        assert!(screen.contains("Xcode DerivedData"));
        assert!(screen.contains("~/Library/Developer/Xcode/DerivedData/App-abc"));
        assert!(!screen.contains("npm cache"));
    }

    #[test]
    fn review_leads_to_the_question_and_n_goes_back() {
        let (_dir, planned) = planned();
        let mut review = Review::new(Arc::clone(&planned));

        assert!(matches!(
            press(&mut review, KeyCode::Enter),
            Action::Open(_)
        ));
        let mut confirm = Confirm::new(planned);
        assert!(render(&mut confirm).contains("Move 1 items"));
        assert!(matches!(
            press(&mut confirm, KeyCode::Char('n')),
            Action::Back
        ));
        assert!(matches!(press(&mut confirm, KeyCode::Enter), Action::None));
        assert!(confirm.is_dialog());
    }

    #[test]
    fn cleanup_moves_items_then_goes_home() {
        let (_dir, planned) = planned();
        let mut cleanup = Cleanup::start_with(planned, |_| false, |_| Ok(()));
        assert!(matches!(cleanup.back(), Action::None) || !cleanup.is_moving());

        finish(&mut cleanup);

        let screen = render(&mut cleanup);
        assert!(screen.contains("Moved 1 items to the Trash."));
        assert!(screen.contains("Emptying the Trash frees that space"));
        assert!(matches!(cleanup.back(), Action::Home));
        assert!(!cleanup.is_dialog());
    }

    #[test]
    fn cleanup_lists_what_it_skipped() {
        let (_dir, planned) = planned();
        let mut cleanup = Cleanup::start_with(planned, |_| true, |_| Ok(()));

        finish(&mut cleanup);

        let screen = render(&mut cleanup);
        assert!(screen.contains("Moved 0 items"));
        assert!(screen.contains("Skipped 1 items"));
        assert!(screen.contains("com.apple.dt.Xcode is open"));
    }

    #[test]
    fn a_selected_caution_rule_is_reviewed_too() {
        let (_dir, mut planned) = planned();
        let planned_mut = Arc::get_mut(&mut planned).expect("only owner");
        for rule in &mut planned_mut.plan.rules {
            if rule.rule.tier == Tier::Caution && !rule.items.is_empty() {
                rule.selected = true;
            }
        }

        let screen = render(&mut Review::new(planned));

        assert!(screen.contains("Review: 2 items"));
        assert!(screen.contains("npm cache"));
    }
}
