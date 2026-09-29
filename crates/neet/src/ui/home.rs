use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::art::ART;
use super::disk::Disk;
use super::format;
use super::placeholder::Placeholder;
use super::scan::ScanStatus;

/// Below this width the art is hidden and the menu fills the screen.
const MIN_ART_WIDTH: u16 = 90;

enum Target {
    Screen(fn() -> Box<dyn Screen>),
    Soon,
    Quit,
}

struct Entry {
    label: &'static str,
    about: &'static str,
    target: Target,
}

impl Entry {
    fn is_selectable(&self) -> bool {
        !matches!(self.target, Target::Soon)
    }
}

const ENTRIES: &[Entry] = &[
    Entry {
        label: "Disk",
        about: "Browse your folders by size.",
        target: Target::Screen(|| Box::new(Disk::new())),
    },
    Entry {
        label: "Clean",
        about: "Move files your apps can make again to the Trash, after you review every path.",
        target: Target::Screen(|| Box::new(Placeholder::new("Clean"))),
    },
    Entry {
        label: "Large Files",
        about: "Find large and old files, and open them in Disk.",
        target: Target::Soon,
    },
    Entry {
        label: "Remove App",
        about: "Review an app and its files before removing it.",
        target: Target::Soon,
    },
    Entry {
        label: "Startup",
        about: "See programs that start on their own, and turn them off in a way you can undo.",
        target: Target::Soon,
    },
    Entry {
        label: "SSH",
        about: "See hosts and key details, fix permissions, and manage agent keys and known hosts.",
        target: Target::Soon,
    },
    Entry {
        label: "Dotfiles",
        about: "List, edit, check, back up, and export settings files.",
        target: Target::Soon,
    },
    Entry {
        label: "AI Tools",
        about: "View Claude Code and Codex settings, instruction files, and skills.",
        target: Target::Soon,
    },
    Entry {
        label: "Settings",
        about: "See what keeps the Mac awake, and change power and display settings.",
        target: Target::Soon,
    },
    Entry {
        label: "Quit",
        about: "Close neet.",
        target: Target::Quit,
    },
];

pub struct Home {
    list: ListState,
}

impl Home {
    pub fn new() -> Self {
        Self {
            list: ListState::default().with_selected(Some(0)),
        }
    }

    fn selected(&self) -> usize {
        self.list.selected().unwrap_or(0)
    }

    /// Moves to the next selectable row in `step` direction, wrapping around.
    fn step(&mut self, forward: bool) {
        let len = ENTRIES.len();
        let mut index = self.selected();
        for _ in 0..len {
            index = if forward {
                (index + 1) % len
            } else {
                (index + len - 1) % len
            };
            if ENTRIES[index].is_selectable() {
                break;
            }
        }
        self.list.select(Some(index));
    }

    fn open(index: usize) -> Action {
        match ENTRIES.get(index).map(|entry| &entry.target) {
            Some(Target::Screen(make)) => Action::Open(make()),
            Some(Target::Quit) => Action::Quit,
            Some(Target::Soon) | None => Action::None,
        }
    }

    fn draw_menu(&mut self, frame: &mut Frame, area: Rect) {
        let items = ENTRIES.iter().enumerate().map(|(index, entry)| {
            let number = match entry.target {
                Target::Quit => "  ".to_string(),
                _ => format!("{} ", index + 1),
            };
            let mut spans = vec![
                Span::raw(number).dark_gray(),
                Span::raw(format!("{:<14}", entry.label)),
            ];
            if matches!(entry.target, Target::Soon) {
                spans.push(Span::raw("soon").italic());
                ListItem::new(Line::from(spans)).dark_gray()
            } else {
                ListItem::new(Line::from(spans))
            }
        });
        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(" neet ")
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .highlight_style(Style::new().bold().cyan());
        frame.render_stateful_widget(list, area, &mut self.list);
    }

    fn draw_info(&self, frame: &mut Frame, area: Rect, scan: &ScanStatus) {
        let entry = &ENTRIES[self.selected()];
        let mut lines = vec![
            Line::from(entry.label).bold(),
            Line::from(entry.about),
            Line::default(),
        ];
        lines.extend(scan_lines(scan));
        let text = Text::from(lines);
        let info = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().padding(Padding::horizontal(1)));
        frame.render_widget(info, area);
    }
}

/// Describes the home folder scan for the info panel.
fn scan_lines(scan: &ScanStatus) -> Vec<Line<'static>> {
    match scan {
        ScanStatus::Running(progress) => vec![
            Line::from("Scanning your home folder…").cyan(),
            Line::from(format!(
                "{} items · {}",
                format::count(progress.entries),
                format::size(progress.bytes)
            ))
            .dark_gray(),
        ],
        ScanStatus::Done { scan, elapsed } => {
            let entries = u64::try_from(scan.tree.node_count() - 1).unwrap_or(u64::MAX);
            let total = scan.tree.get(scan.tree.root()).total_size;
            let mut lines = vec![Line::from(format!(
                "Home folder: {} in {} items, scanned in {}s.",
                format::size(total),
                format::count(entries),
                elapsed.as_secs()
            ))];
            if !scan.is_complete() {
                lines.push(
                    Line::from(format!(
                        "Incomplete: {} paths could not be read. Full Disk Access may be off.",
                        format::count(u64::try_from(scan.errors.len()).unwrap_or(u64::MAX))
                    ))
                    .yellow(),
                );
            }
            if !scan.other_disks.is_empty() {
                lines.push(
                    Line::from(format!(
                        "Skipped {} folders on other disks.",
                        scan.other_disks.len()
                    ))
                    .dark_gray(),
                );
            }
            lines
        }
        ScanStatus::Failed(reason) => vec![Line::from(format!("Scan failed: {reason}")).red()],
    }
}

fn draw_art(frame: &mut Frame, area: Rect) {
    let lines: Vec<&str> = ART.trim_matches('\n').lines().collect();
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let width = lines
        .iter()
        .map(|line| u16::try_from(line.chars().count()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let art = Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>()).cyan();
    frame.render_widget(art, area);
}

impl Screen for Home {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let right = if area.width >= MIN_ART_WIDTH {
            let [left, right] =
                Layout::horizontal([Constraint::Percentage(45), Constraint::Fill(1)]).areas(area);
            draw_art(frame, left);
            right
        } else {
            area
        };
        let menu_height = u16::try_from(ENTRIES.len() + 2).unwrap_or(u16::MAX);
        let [menu, info] =
            Layout::vertical([Constraint::Length(menu_height), Constraint::Fill(1)]).areas(right);
        self.draw_menu(frame, menu);
        self.draw_info(frame, info, context.scan);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                return Self::open(self.selected());
            }
            KeyCode::Char(c @ '1'..='9') => {
                let index = c as usize - '1' as usize;
                return Self::open(index);
            }
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · enter open · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("Enter  →  l", "Open the selected screen"),
            ("1 to 9", "Open that row"),
            ("Esc", "Go back one step"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;

    fn press(home: &mut Home, code: KeyCode) -> Action {
        let scan = ScanStatus::Failed(String::new());
        home.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            &Context { scan: &scan },
        )
    }

    fn render(home: &mut Home, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let scan = ScanStatus::Running(neet_core::scan::Progress {
            entries: 1_234,
            bytes: 5_000_000,
        });
        let context = Context { scan: &scan };
        terminal
            .draw(|frame| home.draw(frame, frame.area(), &context))
            .unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn down_skips_rows_that_are_not_built() {
        let mut home = Home::new();
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Clean");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Quit");
    }

    #[test]
    fn up_from_the_top_wraps_to_quit() {
        let mut home = Home::new();
        press(&mut home, KeyCode::Up);
        assert_eq!(ENTRIES[home.selected()].label, "Quit");
    }

    #[test]
    fn number_keys_open_rows() {
        let mut home = Home::new();
        assert!(matches!(
            press(&mut home, KeyCode::Char('1')),
            Action::Open(_)
        ));
        assert!(matches!(press(&mut home, KeyCode::Char('5')), Action::None));
    }

    #[test]
    fn wide_terminal_shows_art_and_menu() {
        let screen = render(&mut Home::new(), 120, 30);
        assert!(screen.contains("███"));
        assert!(screen.contains("Disk"));
        assert!(screen.contains("soon"));
        assert!(screen.contains("1,234 items · 5.0 MB"));
    }

    #[test]
    fn narrow_terminal_hides_art() {
        let screen = render(&mut Home::new(), 60, 30);
        assert!(!screen.contains("███"));
        assert!(screen.contains("Clean"));
    }
}
