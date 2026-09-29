use std::time::{SystemTime, UNIX_EPOCH};

use neet_core::scan::Progress;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use super::format;

/// Frames of the spinner, one every 100 ms
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The widest the box gets
const WIDTH: u16 = 60;

/// What a screen shows while its slow work runs
pub struct Loading<'a> {
    /// The screen's name, shown on the box's border
    pub title: &'a str,
    /// What neet is doing, such as `Scanning your home folder`
    pub doing: &'a str,
    /// How far it has got, such as `812,400 items · 48.2 GB`
    pub progress: String,
    /// What happens next, or a reassurance
    pub note: &'a str,
}

impl Loading<'_> {
    /// Draws a small box in the middle of `area`.
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let width = WIDTH.min(area.width);
        // The note wraps inside the border and padding.
        let inner = usize::from(width.saturating_sub(4)).max(1);
        let note_lines = self.note.chars().count().div_ceil(inner).max(1);
        // Border and padding, then doing, progress, a gap, and the note
        let height = u16::try_from(4 + 3 + note_lines)
            .unwrap_or(u16::MAX)
            .min(area.height);
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .areas(area);

        let lines = vec![
            Line::from(vec![
                Span::raw(format!("{} ", spinner())).cyan(),
                Span::raw(self.doing).bold(),
            ]),
            Line::from(format!("  {}", self.progress)).cyan(),
            Line::default(),
            Line::from(self.note).dark_gray(),
        ];
        let block = Block::bordered()
            .title(format!(" {} ", self.title))
            .padding(Padding::uniform(1));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
    }
}

/// The loading box for a screen that waits for the home folder scan
pub fn scanning(title: &'static str, note: &'static str, progress: Progress) -> Loading<'static> {
    Loading {
        title,
        doing: "Scanning your home folder",
        progress: format!(
            "{} items · {} so far",
            format::count(progress.entries),
            format::size(progress.bytes)
        ),
        note,
    }
}

/// The spinner frame for now, so it turns without keeping any state.
fn spinner() -> &'static str {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let frame = usize::try_from(millis / 100 % 10).unwrap_or(0);
    SPINNER[frame]
}
