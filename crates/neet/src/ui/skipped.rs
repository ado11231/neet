use std::path::Path;

use neet_core::scan::Scan;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::format;
use super::scan::ScanStatus;

/// Lists what the scan could not read, and the other disks it did not enter.
pub struct Skipped {
    scroll: u16,
}

impl Skipped {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }
}

/// `path` with the scan root shown as `~`.
fn display_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn heading(text: String) -> Line<'static> {
    Line::from(text).bold()
}

fn lines(scan: &Scan) -> Vec<Line<'static>> {
    let root = scan.tree.path(scan.tree.root());
    let mut lines = Vec::new();

    if scan.errors.iter().any(|error| error.permission_denied) {
        lines.push(Line::from("macOS blocked some folders.").yellow().bold());
        lines.push(Line::from(
            "To let neet read them, open System Settings, go to Privacy & Security, \
             then Full Disk Access, and turn it on for your terminal. Then restart the \
             terminal and neet.",
        ));
        lines.push(Line::default());
    }

    if scan.errors.is_empty() && scan.other_disks.is_empty() {
        lines.push(Line::from(
            "Nothing was skipped. The scan read every folder.",
        ));
        return lines;
    }

    if !scan.errors.is_empty() {
        let count = u64::try_from(scan.errors.len()).unwrap_or(u64::MAX);
        lines.push(heading(format!(
            "Could not read ({})",
            format::count(count)
        )));
        for error in &scan.errors {
            let path = error.path.as_deref().map_or_else(
                || "Unknown path".to_string(),
                |path| display_path(&root, path),
            );
            lines.push(Line::from(vec![
                Span::raw(path),
                Span::raw(format!("  {}", error.message)).dark_gray(),
            ]));
        }
        lines.push(Line::default());
    }

    if !scan.other_disks.is_empty() {
        lines.push(heading(format!(
            "On other disks, not scanned ({})",
            scan.other_disks.len()
        )));
        for path in &scan.other_disks {
            lines.push(Line::from(display_path(&root, path)));
        }
    }
    lines
}

impl Screen for Skipped {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let text = match context.scan {
            ScanStatus::Done { scan, .. } => lines(scan),
            ScanStatus::Running(_) => {
                vec![Line::from(
                    "The scan is still running. Check back when it finishes.",
                )]
            }
            ScanStatus::Failed(reason) => vec![Line::from(format!("The scan failed. {reason}"))],
        };
        // Keep the last line on screen when scrolling to the end.
        let visible = area.height.saturating_sub(2);
        let last = u16::try_from(text.len())
            .unwrap_or(u16::MAX)
            .saturating_sub(visible);
        self.scroll = self.scroll.min(last);

        let block = Block::bordered()
            .title(" Skipped ")
            .padding(Padding::horizontal(1));
        let body = Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .block(block);
        frame.render_widget(body, area);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Char('g') | KeyCode::Home => self.scroll = 0,
            // Clamped to the last line when drawn.
            KeyCode::Char('G') | KeyCode::End => self.scroll = u16::MAX,
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ scroll · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Scroll"),
            ("PgUp PgDn", "Scroll a page"),
            ("g  G", "Jump to the top or bottom"),
            ("Esc", "Go back to Home"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neet_core::scan::ScanError;
    use neet_core::tree::Tree;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn done(errors: Vec<ScanError>, other_disks: Vec<PathBuf>) -> ScanStatus {
        ScanStatus::Done {
            scan: Scan {
                tree: Tree::new("/Users/test"),
                errors,
                other_disks,
            },
            elapsed: std::time::Duration::ZERO,
        }
    }

    fn render(scan: &ScanStatus) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let context = Context { scan, disk: None };
        terminal
            .draw(|frame| Skipped::new().draw(frame, frame.area(), &context))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn lists_unreadable_paths_with_a_full_disk_access_hint() {
        let scan = done(
            vec![ScanError {
                path: Some(PathBuf::from("/Users/test/Library/Mail")),
                message: "Operation not permitted (os error 1)".to_string(),
                permission_denied: true,
            }],
            vec![PathBuf::from("/Users/test/Volumes/USB")],
        );

        let screen = render(&scan);

        assert!(screen.contains("Full Disk Access"));
        assert!(screen.contains("Could not read (1)"));
        assert!(screen.contains("~/Library/Mail"));
        assert!(screen.contains("Operation not permitted"));
        assert!(screen.contains("On other disks, not scanned (1)"));
        assert!(screen.contains("~/Volumes/USB"));
    }

    #[test]
    fn says_so_when_nothing_was_skipped() {
        let screen = render(&done(Vec::new(), Vec::new()));

        assert!(screen.contains("Nothing was skipped"));
        assert!(!screen.contains("Full Disk Access"));
    }
}
