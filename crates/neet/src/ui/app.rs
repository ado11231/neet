use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use super::help::Help;
use super::home::Home;

/// What a screen asks the app to do after handling a key.
pub enum Action {
    None,
    Open(Box<dyn Screen>),
    Back,
    Quit,
}

/// One screen on the stack. Home is always at the bottom.
pub trait Screen {
    fn draw(&mut self, frame: &mut Frame, area: Rect);

    fn handle_key(&mut self, key: KeyEvent) -> Action;

    /// Key hints shown in the footer.
    fn hints(&self) -> &'static str;

    /// Help text shown by `?`.
    fn help(&self) -> &'static [(&'static str, &'static str)];

    /// A dialog blocks `q`, so a stray key cannot quit mid question.
    fn is_dialog(&self) -> bool {
        false
    }

    /// An overlay is drawn on top of the screen below it.
    fn is_overlay(&self) -> bool {
        false
    }
}

pub struct App {
    stack: Vec<Box<dyn Screen>>,
    quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            stack: vec![Box::new(Home::new())],
            quit: false,
        }
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
        for screen in &mut self.stack[base..] {
            screen.draw(frame, body);
        }
        let screen = self.top();
        let hints = Line::from(format!(" {}", screen.hints())).style(Style::new().dark_gray());
        frame.render_widget(hints, footer);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let screen = self.top();
        let action = match key.code {
            KeyCode::Char('q') if !screen.is_dialog() => Action::Quit,
            KeyCode::Esc => Action::Back,
            KeyCode::Char('?') => Action::Open(Box::new(Help::new(screen.help()))),
            _ => screen.handle_key(key),
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
        let mut app = App::new();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.depth(), 2);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.depth(), 1);
    }

    #[test]
    fn esc_on_home_stays_on_home() {
        let mut app = App::new();
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.depth(), 1);
        assert!(!app.should_quit());
    }

    #[test]
    fn q_quits() {
        let mut app = App::new();
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit());
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = App::new();
        press(&mut app, KeyCode::Char('?'));
        assert_eq!(app.depth(), 2);
    }
}
