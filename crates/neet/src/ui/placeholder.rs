use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph};

use super::app::{Action, Screen};

/// Stands in for a feature screen until it is built.
pub struct Placeholder {
    title: &'static str,
}

impl Placeholder {
    pub fn new(title: &'static str) -> Self {
        Self { title }
    }
}

impl Screen for Placeholder {
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered().title(format!(" {} ", self.title));
        let body = Paragraph::new("This screen is not built yet.").block(block);
        frame.render_widget(body, area);
    }

    fn handle_key(&mut self, _key: KeyEvent) -> Action {
        Action::None
    }

    fn hints(&self) -> &'static str {
        "esc back · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[("Esc", "Go back to Home"), ("q", "Quit")]
    }
}
