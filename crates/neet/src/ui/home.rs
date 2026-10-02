use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::apps::RemoveApp;
use super::art;
use super::clean::Clean;
use super::disk::Disk;
use super::format;
use super::large::LargeFiles;
use super::quick::QuickClean;
use super::scan::ScanStatus;
use super::skipped::Skipped;
use neet_core::disk::DiskSpace;

/// Below this width the art is hidden and the menu fills the screen.
const MIN_ART_WIDTH: u16 = 90;

/// Columns kept clear between the art and the menu.
const ART_GAP: u16 = 2;

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
        label: "Quick Clean",
        about: "Start here: everything taking space that can be cleared, and how to clear it.",
        target: Target::Screen(|| Box::new(QuickClean::new())),
    },
    Entry {
        label: "Disk",
        about: "Browse your folders by size.",
        target: Target::Screen(|| Box::new(Disk::new())),
    },
    Entry {
        label: "Clean",
        about: "Move files your apps can make again to the Trash, after you review every path.",
        target: Target::Screen(|| Box::new(Clean::new())),
    },
    Entry {
        label: "Large Files",
        about: "Find large and old files, and open them in Disk.",
        target: Target::Screen(|| Box::new(LargeFiles::new())),
    },
    Entry {
        label: "Remove App",
        about: "Review an app and its files before removing it.",
        target: Target::Screen(|| Box::new(RemoveApp::new())),
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

    fn draw_menu(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let items = ENTRIES.iter().enumerate().map(|(index, entry)| {
            // Only rows 1 to 9 have a number key.
            let number = match entry.target {
                Target::Quit => "  ".to_string(),
                _ if index >= 9 => "  ".to_string(),
                _ => format!("{} ", index + 1),
            };
            let mut spans = vec![
                Span::raw(number).dark_gray(),
                Span::raw(format!("{:<14}", entry.label)),
            ];
            if let Some(summary) = summary(entry.label, context) {
                spans.push(Span::raw(summary).dark_gray());
            }
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

    fn draw_info(&self, frame: &mut Frame, area: Rect, context: &Context) {
        let scan = context.scan;
        let disk = context.disk;
        let entry = &ENTRIES[self.selected()];
        let mut lines = vec![
            Line::from(entry.label).bold(),
            Line::from(entry.about),
            Line::default(),
        ];
        lines.extend(disk_lines(disk));
        lines.push(Line::default());
        lines.extend(scan_lines(scan));
        let text = Text::from(lines);
        let info = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().padding(Padding::horizontal(1)));
        frame.render_widget(info, area);
    }
}

/// A short note beside a menu row, such as how much Clean can free.
fn summary(label: &str, context: &Context) -> Option<String> {
    match label {
        "Disk" => context
            .disk
            .map(|disk| format!("{} used", format::size(disk.used()))),
        "Quick Clean" => Some("start here".to_string()),
        "Clean" => Some(context.cleanable.map_or_else(
            || "finding…".to_string(),
            |size| format!("~{} found", format::size(size)),
        )),
        _ => None,
    }
}

/// How many characters wide the disk gauge is.
const GAUGE_WIDTH: usize = 16;

/// Purgeable space smaller than this is not worth a line.
const MIN_PURGEABLE: u64 = 100_000_000;

/// A gauge of how full the disk is, from the disk's own totals, then the
/// free space on its own line, so neither wraps in a narrow panel. When macOS
/// can clear space on its own, a last line says why Finder shows more free.
fn disk_lines(disk: Option<DiskSpace>) -> Vec<Line<'static>> {
    let Some(disk) = disk else {
        return vec![Line::from("Disk space could not be read.").dark_gray()];
    };
    let used = format::percent(disk.used(), disk.total);
    let gauge = Span::raw(format::bar(disk.used(), disk.total, GAUGE_WIDTH));
    let gauge = match used {
        90.. => gauge.red(),
        75..90 => gauge.yellow(),
        _ => gauge.cyan(),
    };
    let mut lines = vec![
        Line::from(vec![gauge, Span::raw(format!(" {used}% used")).bold()]),
        Line::from(format!(
            "{} free of {}",
            format::size(disk.available),
            format::size(disk.total)
        ))
        .dark_gray(),
    ];
    if let Some(purgeable) = disk.purgeable.filter(|size| *size >= MIN_PURGEABLE) {
        lines.push(
            Line::from(format!(
                "Finder shows {} free, counting {} macOS clears when needed.",
                format::size(disk.available.saturating_add(purgeable)),
                format::size(purgeable)
            ))
            .dark_gray(),
        );
    }
    lines
}

/// A count of paths, such as `1 path` or `147 paths`
fn paths(count: usize) -> String {
    let noun = if count == 1 { "path" } else { "paths" };
    format!(
        "{} {noun}",
        format::count(u64::try_from(count).unwrap_or(u64::MAX))
    )
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
            // An incomplete scan missed whatever it could not read.
            let at_least = if scan.is_complete() { "" } else { "at least " };
            let mut lines = vec![Line::from(format!(
                "Home folder: {at_least}{} in {} items, scanned in {}s.",
                format::size(total),
                format::count(entries),
                elapsed.as_secs()
            ))];
            let blocked = scan
                .errors
                .iter()
                .filter(|error| error.permission_denied)
                .count();
            let unreadable = scan.errors.len() - blocked;
            if blocked > 0 {
                lines.push(
                    Line::from(format!(
                        "Incomplete: macOS blocked {}. Press s to see how to allow them.",
                        paths(blocked)
                    ))
                    .yellow(),
                );
            }
            if unreadable > 0 {
                lines.push(
                    Line::from(format!(
                        "Incomplete: {} could not be read. Press s to see them.",
                        paths(unreadable)
                    ))
                    .yellow(),
                );
            }
            if !scan.other_disks.is_empty() {
                lines.push(
                    Line::from(format!(
                        "Skipped {} folders on other disks. Press s to see them.",
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
    frame.render_widget(art::Night, area);
}

impl Screen for Home {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let right = if area.width >= MIN_ART_WIDTH {
            // About 45% of the width, but always a little wider than the art,
            // so it never touches the menu.
            let left_width = (area.width * 45 / 100).max(art::size().0 + ART_GAP);
            let [left, right] =
                Layout::horizontal([Constraint::Length(left_width), Constraint::Fill(1)])
                    .areas(area);
            draw_art(frame, left);
            right
        } else {
            area
        };
        let menu_height = u16::try_from(ENTRIES.len() + 2).unwrap_or(u16::MAX);
        let [menu, info] =
            Layout::vertical([Constraint::Length(menu_height), Constraint::Fill(1)]).areas(right);
        self.draw_menu(frame, menu, context);
        self.draw_info(frame, info, context);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                return Self::open(self.selected());
            }
            KeyCode::Char('s') => return Action::Open(Box::new(Skipped::new())),
            KeyCode::Char(c @ '1'..='9') => {
                let index = c as usize - '1' as usize;
                return Self::open(index);
            }
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · enter open · s skipped · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("Enter  →  l", "Open the selected screen"),
            ("1 to 9", "Open that row"),
            ("s", "See what the scan skipped"),
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
            &Context {
                scan: &scan,
                disk: None,
                cleanable: None,
                plan: None,
            },
        )
    }

    fn render(home: &mut Home, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let scan = ScanStatus::Running(neet_core::scan::Progress {
            entries: 1_234,
            bytes: 5_000_000,
        });
        let context = Context {
            scan: &scan,
            disk: Some(neet_core::disk::DiskSpace {
                total: 500_000_000_000,
                available: 100_000_000_000,
                purgeable: Some(7_400_000_000),
            }),
            cleanable: Some(18_400_000_000),
            plan: None,
        };
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
        assert_eq!(ENTRIES[home.selected()].label, "Quick Clean");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Disk");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Clean");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Large Files");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Remove App");
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
        assert!(matches!(press(&mut home, KeyCode::Char('6')), Action::None));
    }

    #[test]
    fn wide_terminal_shows_art_and_menu() {
        let screen = render(&mut Home::new(), 120, 30);
        assert!(screen.contains("███"));
        assert!(screen.contains("⣿"));
        assert!(screen.contains("Disk"));
        assert!(screen.contains("soon"));
        assert!(screen.contains("1,234 items · 5.0 MB"));
        assert!(screen.contains("80% used"));
        assert!(screen.contains("100.0 GB free of 500.0 GB"));
        assert!(screen.contains("Finder shows 107.4 GB free, counting 7.4 GB"));
        assert!(screen.contains("400.0 GB used"));
        assert!(screen.contains("~18.4 GB found"));
    }

    #[test]
    fn purgeable_space_is_explained_only_when_it_matters() {
        let text = |purgeable| {
            disk_lines(Some(DiskSpace {
                total: 500_000_000_000,
                available: 100_000_000_000,
                purgeable,
            }))
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
        };

        assert!(text(Some(7_400_000_000)).contains("counting 7.4 GB macOS clears when needed"));
        assert!(!text(Some(50_000_000)).contains("Finder"));
        assert!(!text(None).contains("Finder"));
    }

    #[test]
    fn narrow_terminal_hides_art() {
        let screen = render(&mut Home::new(), 60, 30);
        assert!(!screen.contains("⣿"));
        assert!(!screen.contains("╚═╝"));
        assert!(screen.contains("Clean"));
    }

    #[test]
    fn an_incomplete_scan_says_why_and_that_the_size_is_a_minimum() {
        let error = |permission_denied| neet_core::scan::ScanError {
            path: Some(std::path::PathBuf::from("/Users/test/Library/Mail")),
            message: String::new(),
            permission_denied,
        };
        let scan = ScanStatus::Done {
            scan: neet_core::scan::Scan {
                tree: neet_core::tree::Tree::new("/Users/test"),
                errors: vec![error(true), error(true), error(false)],
                other_disks: Vec::new(),
            },
            elapsed: std::time::Duration::ZERO,
        };

        let text: Vec<String> = scan_lines(&scan).iter().map(ToString::to_string).collect();

        assert!(text[0].starts_with("Home folder: at least "));
        assert!(text[1].contains("macOS blocked 2 paths"));
        assert!(text[2].contains("1 path could not be read"));
    }
}
