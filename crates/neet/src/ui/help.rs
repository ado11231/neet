use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::app::{Action, Context, Screen};

/// A box listing the keys for the screen underneath.
pub struct Help {
    keys: &'static [(&'static str, &'static str)],
}

impl Help {
    pub fn new(keys: &'static [(&'static str, &'static str)]) -> Self {
        Self { keys }
    }
}

impl Screen for Help {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        let lines: Vec<Line> = self
            .keys
            .iter()
            .map(|(key, action)| Line::from(vec![format!("{key:<14}").bold(), (*action).into()]))
            .collect();
        let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(48)])
            .flex(Flex::Center)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Help ")),
            area,
        );
    }

    fn handle_key(&mut self, _key: KeyEvent, _context: &Context) -> Action {
        Action::None
    }

    fn hints(&self) -> &'static str {
        "esc close"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        self.keys
    }

    fn is_overlay(&self) -> bool {
        true
    }
}
