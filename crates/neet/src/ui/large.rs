use std::time::{Duration, SystemTime};

use neet_core::large::{self, Filter};
use neet_core::tree::{NodeId, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::disk::{Disk, display_path};
use super::format;
use super::loading::scanning;
use super::scan::ScanStatus;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// The smallest file sizes to choose from, with `s`
const SIZES: [u64; 6] = [
    10_000_000,
    50_000_000,
    100_000_000,
    500_000_000,
    1_000_000_000,
    5_000_000_000,
];

/// How long files must have gone unchanged, with `a`, in days. `0` means
/// any age.
const AGES: [(u32, &str); 6] = [
    (0, "any age"),
    (30, "30 days"),
    (90, "3 months"),
    (180, "6 months"),
    (365, "1 year"),
    (730, "2 years"),
];

/// At most this many files are listed, largest first.
const MAX_ROWS: usize = 1000;

/// Width of the size column, such as `179.0 MB`
const SIZE_WIDTH: u16 = 9;

/// Width of the last changed column, such as `11 months ago`
const AGE_WIDTH: u16 = 13;

/// Space between columns
const GAP: u16 = 2;

/// Lists files above a size, and optionally unchanged for a while, from the
/// home folder scan. Finding a file does not make it a cleanup target.
pub struct LargeFiles {
    size: usize,
    age: usize,
    /// Found files, worked out again when a filter changes
    found: Option<Vec<NodeId>>,
    table: TableState,
}

impl LargeFiles {
    pub fn new() -> Self {
        Self {
            size: 2,
            age: 0,
            found: None,
            table: TableState::default().with_selected(Some(0)),
        }
    }

    fn filter(&self) -> Filter {
        let days = AGES[self.age].0;
        Filter {
            min_size: SIZES[self.size],
            unchanged_for: (days > 0).then(|| DAY * days),
        }
    }

    fn found(&mut self, tree: &Tree) -> &[NodeId] {
        let filter = self.filter();
        self.found
            .get_or_insert_with(|| large::find(tree, filter, SystemTime::now()))
    }

    fn refilter(&mut self) {
        self.found = None;
        self.table.select(Some(0));
    }

    fn selected(&self) -> Option<NodeId> {
        let index = self.table.selected()?;
        self.found.as_ref()?.get(index).copied()
    }

    fn title(&self, found: &[NodeId], tree: &Tree) -> String {
        let total: u64 = found.iter().map(|&id| tree.get(id).own_size).sum();
        let age = match AGES[self.age] {
            (0, _) => String::new(),
            (_, label) => format!(", unchanged for {label}"),
        };
        format!(
            " Large Files: {} {} of {} or more{age}, {} in all ",
            format::count(u64::try_from(found.len()).unwrap_or(u64::MAX)),
            if found.len() == 1 { "file" } else { "files" },
            format::size(SIZES[self.size]),
            format::size(total)
        )
    }
}

/// One file as a table row. `name` and `folder` are the widths of those
/// columns, so long text is shortened at its least useful end.
fn row(tree: &Tree, id: NodeId, now: SystemTime, name: usize, folder: usize) -> Row<'static> {
    let node = tree.get(id);
    let size = format::size_span(node.own_size, format::size(node.own_size));

    let elapsed = node
        .modified
        .and_then(|modified| now.duration_since(modified).ok());
    let age = Span::raw(elapsed.map_or_else(|| "unknown".to_string(), format::age));
    // Recent files are dimmed. Files left alone over a year stand out, since
    // they are the likeliest to be forgotten.
    let age = match elapsed {
        Some(elapsed) if elapsed >= DAY * 365 => age.magenta(),
        Some(elapsed) if elapsed >= DAY * 30 => age,
        _ => age,
    };

    let path = display_path(tree, id);
    let (parent, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
    Row::new([
        Cell::from(Line::from(size).right_aligned()),
        Cell::from(age),
        Cell::from(format::shorten_middle(file, name)),
        Cell::from(Span::raw(format::shorten_path(parent, folder))),
    ])
}

impl Screen for LargeFiles {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let scan = match context.scan {
            ScanStatus::Done { scan, .. } => scan,
            ScanStatus::Running(progress) => {
                scanning(
                    "Large Files",
                    "Large files show here when the scan finishes.",
                    *progress,
                )
                .draw(frame, area);
                return;
            }
            ScanStatus::Failed(reason) => {
                let block = Block::bordered()
                    .title(" Large Files ")
                    .padding(Padding::horizontal(1));
                frame.render_widget(
                    Paragraph::new(format!(
                        "The scan failed, so there is nothing to show. {reason}"
                    ))
                    .red()
                    .wrap(Wrap { trim: true })
                    .block(block),
                    area,
                );
                return;
            }
        };
        let tree = &scan.tree;
        let found = self.found(tree).to_vec();
        let title = self.title(&found, tree);
        let now = SystemTime::now();
        let block = Block::bordered()
            .title(title)
            .padding(Padding::horizontal(1));
        if found.is_empty() {
            frame.render_widget(
                Paragraph::new("No files match. Press s for a smaller size, or a for any age.")
                    .block(block),
                area,
            );
            return;
        }
        let block = if found.len() > MAX_ROWS {
            block.title_bottom(Line::from(format!(
                " and {} more. Press s for a larger size to see fewer. ",
                format::count(u64::try_from(found.len() - MAX_ROWS).unwrap_or(u64::MAX))
            )))
        } else {
            block
        };

        // The name and folder share what is left after the fixed columns,
        // the border, the padding, and the selection arrow.
        let fixed = 2 + 2 + 2 + SIZE_WIDTH + AGE_WIDTH + GAP * 3;
        let rest = area.width.saturating_sub(fixed);
        let name = rest * 2 / 5;
        let folder = rest - name;
        let rows: Vec<Row> = found
            .iter()
            .take(MAX_ROWS)
            .map(|&id| row(tree, id, now, usize::from(name), usize::from(folder)))
            .collect();
        let header = Row::new([
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from("Last Changed"),
            Cell::from("Name"),
            Cell::from("Folder"),
        ])
        .bold()
        .bottom_margin(1);
        let table = Table::new(
            rows,
            [
                Constraint::Length(SIZE_WIDTH),
                Constraint::Length(AGE_WIDTH),
                Constraint::Length(name),
                Constraint::Length(folder),
            ],
        )
        .header(header)
        .column_spacing(GAP)
        .block(block)
        .highlight_symbol("▸ ")
        .row_highlight_style(Style::new().bold());
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    fn handle_key(&mut self, key: KeyEvent, context: &Context) -> Action {
        let ScanStatus::Done { scan, .. } = context.scan else {
            return Action::None;
        };
        let tree = &scan.tree;
        let rows = self.found(tree).len().min(MAX_ROWS);
        let last = rows.saturating_sub(1);
        let index = self.table.selected().unwrap_or(0);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.table.select(Some(index.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => self.table.select(Some((index + 1).min(last))),
            KeyCode::Char('g') | KeyCode::Home => self.table.select(Some(0)),
            KeyCode::Char('G') | KeyCode::End => self.table.select(Some(last)),
            KeyCode::Char('s') => {
                self.size = (self.size + 1) % SIZES.len();
                self.refilter();
            }
            KeyCode::Char('a') => {
                self.age = (self.age + 1) % AGES.len();
                self.refilter();
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(id) = self.selected() {
                    return Action::Open(Box::new(Disk::showing(tree, id)));
                }
            }
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · s size · a age · enter show in Disk · esc home · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("g  G", "Jump to the first or last file"),
            ("s", "Change the smallest size: 10 MB to 5 GB"),
            ("a", "Change how long files must be unchanged"),
            ("Enter  →  l", "Show the file in Disk"),
            ("Esc", "Go back"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neet_core::scan::Scan;
    use neet_core::tree::NodeKind;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use ratatui::style::Color;

    fn done() -> ScanStatus {
        let now = SystemTime::now();
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let movies = tree.add(root, "Movies", NodeKind::Directory, 0);
        let film = tree.add(movies, "film.mov", NodeKind::File, 2_000_000_000);
        tree.set_modified(film, now - DAY * 400);
        let backup = tree.add(root, "backup.zip", NodeKind::File, 300_000_000);
        tree.set_modified(backup, now - DAY * 2);
        let _small = tree.add(root, "notes.txt", NodeKind::File, 5_000);
        ScanStatus::Done {
            scan: Scan {
                tree,
                errors: Vec::new(),
                other_disks: Vec::new(),
            },
            elapsed: Duration::ZERO,
        }
    }

    fn render(screen: &mut LargeFiles, scan: &ScanStatus) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 12)).unwrap();
        let context = Context {
            scan,
            disk: None,
            cleanable: None,
            plan: None,
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

    fn press(screen: &mut LargeFiles, scan: &ScanStatus, code: KeyCode) -> Action {
        screen.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            &Context {
                scan,
                disk: None,
                cleanable: None,
                plan: None,
            },
        )
    }

    #[test]
    fn lists_large_files_largest_first_with_their_age() {
        let scan = done();
        let screen = render(&mut LargeFiles::new(), &scan);

        assert!(screen.contains("2 files of 100.0 MB or more"));
        assert!(screen.contains("Last Changed"));
        assert!(screen.contains("film.mov"));
        assert!(screen.contains("~/Movies"));
        assert!(screen.contains("1 year ago"));
        assert!(screen.contains("backup.zip"));
        assert!(!screen.contains("notes.txt"));
        assert!(screen.find("film.mov") < screen.find("backup.zip"));
    }

    #[test]
    fn filters_change_with_s_and_a() {
        let scan = done();
        let mut large = LargeFiles::new();

        press(&mut large, &scan, KeyCode::Char('s'));
        let screen = render(&mut large, &scan);
        assert!(screen.contains("1 file of 500.0 MB or more"));

        for _ in 0..4 {
            press(&mut large, &scan, KeyCode::Char('s'));
        }
        press(&mut large, &scan, KeyCode::Char('a'));
        let screen = render(&mut large, &scan);
        assert!(screen.contains("unchanged for 30 days"));
        assert!(screen.contains("film.mov"));
        assert!(!screen.contains("backup.zip"));
    }

    #[test]
    fn enter_shows_the_file_in_disk() {
        let scan = done();
        let mut large = LargeFiles::new();
        render(&mut large, &scan);

        assert!(matches!(
            press(&mut large, &scan, KeyCode::Enter),
            Action::Open(_)
        ));
    }

    /// The color of the first cell showing `text`
    fn color_of(screen: &mut LargeFiles, scan: &ScanStatus, text: &str) -> Color {
        let mut terminal = Terminal::new(TestBackend::new(110, 12)).unwrap();
        let context = Context {
            scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        terminal
            .draw(|frame| screen.draw(frame, frame.area(), &context))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let cells: Vec<&str> = buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        let index = cells
            .windows(text.chars().count())
            .position(|window| window.concat() == text)
            .expect("text should be on screen");
        buffer.content()[index].fg
    }

    #[test]
    fn sizes_and_ages_are_colored() {
        let scan = done();
        let mut large = LargeFiles::new();

        assert_eq!(color_of(&mut large, &scan, "300.0 MB"), Color::Reset);
        assert_eq!(color_of(&mut large, &scan, "2 days ago"), Color::Reset);
        // The selected row is bold, and keeps its colors.
        assert_eq!(color_of(&mut large, &scan, "2.0 GB"), Color::Yellow);
        assert_eq!(color_of(&mut large, &scan, "1 year ago"), Color::Magenta);

        press(&mut large, &scan, KeyCode::Down);
        assert_eq!(color_of(&mut large, &scan, "2.0 GB"), Color::Yellow);
        assert_eq!(color_of(&mut large, &scan, "300.0 MB"), Color::Reset);
    }

    #[test]
    fn waits_for_the_scan() {
        let scan = ScanStatus::Running(neet_core::scan::Progress::default());
        let screen = render(&mut LargeFiles::new(), &scan);

        assert!(screen.contains("Large files show here when the scan finishes"));
    }
}
