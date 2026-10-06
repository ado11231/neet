use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::apps::RemoveApp;
use super::art;
use super::clean::Clean;
use super::disk::Disk;
use super::dotfiles::Dotfiles;
use super::format;
use super::large::LargeFiles;
use super::quick::QuickClean;
use super::scan::ScanStatus;
use super::skipped::Skipped;
use neet_core::disk::DiskSpace;

/// Below this width the art is hidden and the menu fills the screen.
const MIN_ART_WIDTH: u16 = 90;

/// Blank rows above and below the selected row's description, so its box
/// stands a little taller than the others.
const ABOUT_PADDING: u16 = 1;

/// Rows between the menu and each box under it
const BOX_GAP: u16 = 1;

/// Columns kept clear between the art and the menu.
const ART_GAP: u16 = 2;

/// The menu and its boxes are never wider than this. On a wide screen the
/// sky takes the rest.
const MAX_RIGHT_WIDTH: u16 = 72;

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
        about: "Browse folders by size.",
        target: Target::Screen(|| Box::new(Disk::new())),
    },
    Entry {
        label: "Quick Clean",
        about: "Find reclaimable space. Select a cleanup action.",
        target: Target::Screen(|| Box::new(QuickClean::new())),
    },
    Entry {
        label: "Deep Clean",
        about: "Select cache and log rules. Review paths before moving to Trash.",
        target: Target::Screen(|| Box::new(Clean::new())),
    },
    Entry {
        label: "Remove App",
        about: "Select an app. Review its files before removal.",
        target: Target::Screen(|| Box::new(RemoveApp::new())),
    },
    Entry {
        label: "Large Files",
        about: "Find large or old files. Inspect them in Disk.",
        target: Target::Screen(|| Box::new(LargeFiles::new())),
    },
    Entry {
        label: "Startup",
        about: "Manage startup programs.",
        target: Target::Soon,
    },
    Entry {
        label: "Dotfiles",
        about: "See your settings files and how they stand with chezmoi.",
        target: Target::Screen(|| Box::new(Dotfiles::new())),
    },
    Entry {
        label: "AI Tools",
        about: "View Claude Code and Codex settings, instruction files, and skills.",
        target: Target::Soon,
    },
    Entry {
        label: "Settings",
        about: "Manage power and display settings.",
        target: Target::Soon,
    },
    Entry {
        label: "SSH",
        about: "Manage hosts, keys, and permissions.",
        target: Target::Soon,
    },
    Entry {
        label: "Quit",
        about: "Close neet.",
        target: Target::Quit,
    },
];

/// The number key that opens the row named `label`
fn key_of(label: &str) -> usize {
    ENTRIES
        .iter()
        .position(|entry| entry.label == label)
        .map_or(0, |index| index + 1)
}

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
            // Only rows 1 to 9 have a number key.
            let number = match entry.target {
                Target::Quit => "  ".to_string(),
                _ if index >= 9 => "  ".to_string(),
                _ => format!("{} ", index + 1),
            };
            let mut spans = vec![
                Span::raw(number).fg(super::visual::ACCENT),
                Span::raw(format!("{:<14}", entry.label)),
            ];
            if matches!(entry.target, Target::Soon) {
                spans.push(Span::raw("soon").yellow().italic());
            }
            ListItem::new(Line::from(spans))
        });
        let list = List::new(items)
            .block(
                super::visual::block()
                    .title(" neet ")
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(list, area, &mut self.list);
    }

    /// The boxes under the menu: the selected row, the disk, and the home
    /// folder, each sized to its lines
    fn panels(&self, width: u16, context: &Context) -> Vec<(String, Vec<Line<'static>>)> {
        let entry = &ENTRIES[self.selected()];
        let inner = width.saturating_sub(4);
        vec![
            (format!(" {} ", entry.label), vec![Line::from(entry.about)]),
            (" This Mac ".to_string(), disk_lines(context.disk, inner)),
            (
                " Home folder ".to_string(),
                scan_lines(context.scan, context.cleanable),
            ),
        ]
    }
}

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{label:<9}")).bold()];
    spans.extend(value);
    Line::from(spans)
}

/// The disk gauge is never narrower or wider than this.
const GAUGE_WIDTH: (usize, usize) = (16, 40);

/// Purgeable space smaller than this is not worth a line.
const MIN_PURGEABLE: u64 = 100_000_000;

/// A gauge of how full the disk is, as wide as the box allows, then how much
/// is free, colored by how little is left. When macOS can clear space on its
/// own, a last line says why Finder shows more free.
fn disk_lines(disk: Option<DiskSpace>, width: u16) -> Vec<Line<'static>> {
    let Some(disk) = disk else {
        return vec![Line::from("Disk space unavailable.").yellow()];
    };
    let used = format::percent(disk.used(), disk.total);
    let color = match used {
        90.. => ratatui::style::Color::Red,
        75..90 => ratatui::style::Color::Yellow,
        _ => super::visual::ACCENT,
    };
    let gauge_width = usize::from(width)
        .saturating_sub(10)
        .clamp(GAUGE_WIDTH.0, GAUGE_WIDTH.1);
    let free = Span::raw(format::size(disk.available)).bold();
    let free = match used {
        90.. => free.red(),
        75..90 => free.yellow(),
        _ => free.green(),
    };
    let mut lines = vec![
        Line::from(vec![
            Span::raw(format::bar(disk.used(), disk.total, gauge_width)).fg(color),
            Span::raw(format!(" {used}% used")).fg(color).bold(),
        ]),
        field(
            "Free",
            vec![free, Span::raw(format!(" of {}", format::size(disk.total)))],
        ),
        field("Used", vec![Span::raw(format::size(disk.used()))]),
    ];
    if let Some(purgeable) = disk.purgeable.filter(|size| *size >= MIN_PURGEABLE) {
        lines.push(field(
            "Finder",
            vec![Span::raw(format!(
                "{} free · {} purgeable by macOS",
                format::size(disk.available.saturating_add(purgeable)),
                format::size(purgeable)
            ))],
        ));
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

/// The home folder scan, and what Deep Clean found in it
fn scan_lines(scan: &ScanStatus, cleanable: Option<u64>) -> Vec<Line<'static>> {
    let mut lines = match scan {
        ScanStatus::Running(progress) => vec![
            field(
                "Size",
                vec![Span::raw("scanning…").fg(super::visual::ACCENT)],
            ),
            field(
                "Items",
                vec![Span::raw(format!(
                    "{} so far · {}",
                    format::count(progress.entries),
                    format::size(progress.bytes)
                ))],
            ),
        ],
        ScanStatus::Done { scan, elapsed } => {
            let entries = u64::try_from(scan.tree.node_count() - 1).unwrap_or(u64::MAX);
            let total = scan.tree.get(scan.tree.root()).total_size;
            // An incomplete scan missed whatever it could not read.
            let at_least = if scan.is_complete() { "" } else { "at least " };
            let mut lines = vec![
                field(
                    "Size",
                    vec![Span::raw(at_least), Span::raw(format::size(total)).bold()],
                ),
                field(
                    "Items",
                    vec![Span::raw(format!(
                        "{} · scanned in {}s",
                        format::count(entries),
                        elapsed.as_secs()
                    ))],
                ),
            ];
            let blocked = scan
                .errors
                .iter()
                .filter(|error| error.permission_denied)
                .count();
            let unreadable = scan.errors.len() - blocked;
            if blocked > 0 {
                lines.push(field(
                    "Blocked",
                    vec![Span::raw(format!("{} · s shows them", paths(blocked))).yellow()],
                ));
            }
            if unreadable > 0 {
                lines.push(field(
                    "Unread",
                    vec![Span::raw(format!("{} · s shows them", paths(unreadable))).yellow()],
                ));
            }
            if !scan.other_disks.is_empty() {
                lines.push(field(
                    "Skipped",
                    vec![Span::raw(format!(
                        "{} folders on other disks · s shows them",
                        scan.other_disks.len()
                    ))],
                ));
            }
            lines
        }
        ScanStatus::Failed(reason) => vec![Line::from(format!("Scan failed: {reason}")).red()],
    };
    lines.push(field(
        "Can free",
        match cleanable {
            None => vec![Span::raw("finding…").fg(super::visual::ACCENT)],
            Some(size) => vec![
                Span::raw(format!("~{}", format::size(size))).green().bold(),
                Span::raw(format!(
                    " in caches & logs · {} Deep Clean",
                    key_of("Deep Clean")
                )),
            ],
        },
    ));
    lines
}

impl Screen for Home {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let wide = area.width >= MIN_ART_WIDTH;
        let right = if wide {
            // About 45% of the width, more when the menu reaches its widest,
            // but always a little wider than the art, so it never touches
            // the menu.
            let left_width = (area.width * 45 / 100)
                .max(area.width.saturating_sub(MAX_RIGHT_WIDTH))
                .max(art::size().0 + ART_GAP);
            let [left, right] =
                Layout::horizontal([Constraint::Length(left_width), Constraint::Fill(1)])
                    .areas(area);
            // Stars across the whole screen; the boxes go on top.
            frame.render_widget(art::Night { art: left }, area);
            right
        } else {
            area
        };
        let menu_height = u16::try_from(ENTRIES.len() + 2).unwrap_or(u16::MAX);
        let mut panels = self.panels(right.width, context);
        // The first box is the selected row's, with its extra rows.
        let padding = |index: usize| if index == 0 { ABOUT_PADDING } else { 0 };
        let heights = |panels: &[(String, Vec<Line<'static>>)]| -> Vec<u16> {
            panels
                .iter()
                .enumerate()
                .map(|(index, (_, lines))| {
                    super::visual::wrapped_rows(lines, right.width.saturating_sub(4))
                        + 2
                        + 2 * padding(index)
                })
                .collect()
        };
        let total = |panels: &[(String, Vec<Line<'static>>)], gap: u16| -> u16 {
            let gaps = u16::try_from(panels.len()).unwrap_or(u16::MAX) * gap;
            menu_height + gaps + heights(panels).iter().sum::<u16>()
        };
        // When room is short, the gaps go first, then boxes, the last first.
        while panels.len() > 1 && total(&panels, 0) > right.height {
            panels.pop();
        }
        let gap = if total(&panels, BOX_GAP) > right.height {
            0
        } else {
            BOX_GAP
        };
        let mut constraints = vec![Constraint::Length(menu_height)];
        constraints.extend(heights(&panels).into_iter().map(Constraint::Length));
        // Centred, like the art beside it.
        let areas = Layout::vertical(constraints)
            .flex(Flex::Center)
            .spacing(gap)
            .split(right);
        if wide {
            // No stars between or right beside the boxes.
            let last = areas.last().copied().unwrap_or(areas[0]);
            let x = right.x.saturating_sub(1);
            let stack = Rect::new(x, areas[0].y, right.right() - x, last.bottom() - areas[0].y);
            frame.render_widget(Clear, stack);
        }
        self.draw_menu(frame, areas[0]);
        for (index, ((title, lines), area)) in
            panels.into_iter().zip(areas.iter().skip(1)).enumerate()
        {
            let rows = padding(index);
            frame.render_widget(
                Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                    super::visual::block()
                        .title(title)
                        .padding(Padding::new(1, 1, rows, rows)),
                ),
                *area,
            );
        }
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
            ("↑ ↓  j k", "Move selection"),
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
        assert_eq!(ENTRIES[home.selected()].label, "Disk");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Quick Clean");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Deep Clean");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Remove App");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Large Files");
        press(&mut home, KeyCode::Down);
        assert_eq!(ENTRIES[home.selected()].label, "Dotfiles");
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
        assert!(screen.contains("1,234 so far · 5.0 MB"));
        assert!(screen.contains("80% used"));
        assert!(screen.contains("Free     100.0 GB of 500.0 GB"));
        assert!(screen.contains("Finder   107.4 GB free · 7.4 GB"));
        assert!(screen.contains("Can free ~18.4 GB"));
        // The selected row's box has a blank row above and below its text.
        let rows: Vec<String> = screen
            .chars()
            .collect::<Vec<_>>()
            .chunks(120)
            .map(|row| row.iter().collect())
            .collect();
        let title = rows
            .iter()
            .position(|row| row.contains("┌ Disk ─"))
            .unwrap();
        assert!(rows[title + 1].trim_end().ends_with('│'));
        assert!(!rows[title + 1].contains("Browse"));
        assert!(rows[title + 2].contains("Browse folders by size."));
        // The menu says only which rows come later.
        assert!(!screen.contains("overview"));
        assert!(!screen.contains("found"));
    }

    #[test]
    fn purgeable_space_is_explained_only_when_it_matters() {
        let text = |purgeable| {
            disk_lines(
                Some(DiskSpace {
                    total: 500_000_000_000,
                    available: 100_000_000_000,
                    purgeable,
                }),
                40,
            )
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
        };

        assert!(text(Some(7_400_000_000)).contains("7.4 GB purgeable by macOS"));
        assert!(!text(Some(50_000_000)).contains("Finder"));
        assert!(!text(None).contains("Finder"));
    }

    #[test]
    fn narrow_terminal_hides_art() {
        let screen = render(&mut Home::new(), 60, 30);
        assert!(!screen.contains("⣿"));
        assert!(!screen.contains("╚═╝"));
        assert!(screen.contains("Deep Clean"));
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

        let text: Vec<String> = scan_lines(&scan, None)
            .iter()
            .map(ToString::to_string)
            .collect();

        assert!(text[0].starts_with("Size     at least "), "{text:?}");
        assert!(text[2].contains("Blocked  2 paths"), "{text:?}");
        assert!(text[3].contains("Unread   1 path"), "{text:?}");
        assert!(text[4].contains("Can free finding…"), "{text:?}");
    }
    #[test]
    fn a_tall_screen_centres_the_boxes_instead_of_stretching_them() {
        let screen = render(&mut Home::new(), 200, 60);
        let rows: Vec<String> = screen
            .chars()
            .collect::<Vec<_>>()
            .chunks(200)
            .map(|row| row.iter().collect())
            .collect();
        let top = rows.iter().position(|row| row.contains("┌ neet")).unwrap();
        let bottom = rows.iter().rposition(|row| row.contains('└')).unwrap();
        assert!(top > 5, "menu starts at row {top}");
        assert!(bottom < 54, "boxes end at row {bottom}");
        // Stars above the menu, on the right side too, but none between the
        // boxes, which stand a row apart.
        let start = rows[top].find('┌').unwrap();
        let above: String = rows[top - 2]
            .chars()
            .skip(rows[top][..start].chars().count())
            .collect();
        assert!(above.contains(['·', '*', '✦', '+']), "{above:?}");
        let gap = rows.iter().position(|row| row.contains("┌ Disk")).unwrap() - 1;
        let between: String = rows[gap]
            .chars()
            .skip(rows[top][..start].chars().count())
            .collect();
        assert!(between.trim().is_empty(), "{between:?}");
        // Never wider than the cap, so the sky takes the rest.
        let width = rows[top].trim_end().chars().count() - rows[top].find('┌').unwrap();
        assert!(width <= usize::from(MAX_RIGHT_WIDTH) + 2, "{width}");
    }

    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let scan = crate::ui::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let mut screen = Home::new();
        for (width, height) in view::SIZES {
            let buffer = view::render("home", &mut screen, &context, width, height);
            assert!(view::text(&buffer).contains("Quick Clean"));
            let (x, y) = view::position(&buffer, "soon");
            assert_eq!(buffer[(x, y)].fg, ratatui::style::Color::Yellow);
        }
    }
}
