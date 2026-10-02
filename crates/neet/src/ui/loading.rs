use std::time::{SystemTime, UNIX_EPOCH};

use neet_core::scan::Progress;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph, Wrap};

use super::format;

/// Frames of the spinner, one every 100 ms
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The widest the box gets
const WIDTH: u16 = 60;

/// What a screen shows while its slow work runs
pub struct Loading<'a> {
    /// The screen's name, shown on the box's border
    pub title: &'a str,
    /// What neet is doing, such as `Scanning home`
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
        let lines = vec![
            Line::from(vec![
                Span::raw(format!("{} ", spinner())).fg(super::visual::ACCENT),
                Span::raw(self.doing).bold(),
            ]),
            Line::from(format!("  {}", self.progress)).fg(super::visual::ACCENT),
            Line::default(),
            Line::from(self.note),
        ];
        let height = super::visual::wrapped_rows(&lines, width.saturating_sub(4))
            .saturating_add(4)
            .min(area.height);
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .areas(area);
        let block = super::visual::block()
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
        doing: "Scanning home",
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn wrapped_progress_and_notes_keep_the_last_line_visible() {
        for width in [40, 60, 80] {
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
            let loading = Loading {
                title: "Loading",
                doing: "Measuring folders and checking cleanup permissions",
                progress: "1,234,567 items · 123.4 GB · 日本語のフォルダ".into(),
                note: "Wait for all folders to finish scanning. Read-only scan.",
            };
            terminal
                .draw(|frame| loading.draw(frame, frame.area()))
                .unwrap();
            let text = crate::ui::visual::tests::text(terminal.backend().buffer());
            assert!(text.contains("Read-only scan."), "{text}");
        }
    }
}
