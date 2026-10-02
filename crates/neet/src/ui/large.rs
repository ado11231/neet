use std::cmp::Reverse;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use neet_core::large::{self, Filter};
use neet_core::tree::{NodeId, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::rows_used;
use super::disk::{Disk, display_path};
use super::format;
use super::loading::scanning;
use super::scan::ScanStatus;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// The smallest file sizes to choose from, with `s`
const SIZES: [(u64, &str); 6] = [
    (10_000_000, "10 MB"),
    (50_000_000, "50 MB"),
    (100_000_000, "100 MB"),
    (500_000_000, "500 MB"),
    (1_000_000_000, "1 GB"),
    (5_000_000_000, "5 GB"),
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

/// Width of the bar beside each size
const BAR_WIDTH: u16 = 8;

/// Width of the bar beside each folder on the right
const FOLDER_BAR: u16 = 6;

/// Width of the last changed column, such as `11 months ago`
const AGE_WIDTH: u16 = 13;

/// Space between columns
const GAP: u16 = 2;

/// From this width the selected file and where the files are show on the
/// right.
const MIN_SIDE_WIDTH: u16 = 120;

/// Width of the right side
const SIDE_WIDTH: u16 = 50;

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
            min_size: SIZES[self.size].0,
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

    /// The two filters, every choice shown and the current one marked, so
    /// `s` and `a` say what they do
    fn draw_filters(&self, frame: &mut Frame, area: Rect) {
        let choices = |labels: Vec<String>, current: usize, key: &'static str, label: &str| {
            let mut spans = vec![Span::raw(format!("{label:<11}")).bold()];
            for (index, text) in labels.into_iter().enumerate() {
                spans.push(if index == current {
                    Span::raw(format!("[{text}]")).green().bold()
                } else {
                    Span::raw(format!(" {text} "))
                });
                spans.push(Span::raw(" "));
            }
            spans.push(Span::raw("  "));
            spans.push(Span::raw(key).bold());
            spans.push(Span::raw(" to change"));
            Line::from(spans)
        };
        let lines = vec![
            choices(
                SIZES
                    .iter()
                    .map(|(_, label)| (*label).to_string())
                    .collect(),
                self.size,
                "s",
                "At least",
            ),
            choices(
                AGES.iter().map(|(_, label)| (*label).to_string()).collect(),
                self.age,
                "a",
                "Unchanged",
            ),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(" Large Files ")
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect, tree: &Tree, found: &[NodeId]) {
        let total: u64 = found.iter().map(|&id| tree.get(id).own_size).sum();
        let mut summary = vec![
            Span::raw(" "),
            Span::raw(format!(
                "{} {}",
                format::count(u64::try_from(found.len()).unwrap_or(u64::MAX)),
                if found.len() == 1 { "file" } else { "files" }
            ))
            .bold(),
            Span::raw(" · "),
            format::size_span(total, format::size(total)).bold(),
            Span::raw(" in all "),
        ];
        if found.len() > MAX_ROWS {
            summary.push(Span::raw(format!(
                "· the largest {} shown ",
                format::count(u64::try_from(MAX_ROWS).unwrap_or(u64::MAX))
            )));
        }
        let block = Block::bordered()
            .title(" Files, largest first ")
            .title_bottom(Line::from(summary).right_aligned())
            .padding(Padding::horizontal(1));
        if found.is_empty() {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            centered(
                frame,
                inner,
                "No files match. Press s for a smaller size, or a for any age.",
            );
            return;
        }

        // The name and folder share what is left after the fixed columns,
        // the border, the padding, and the selection arrow.
        let fixed = 2 + 2 + 2 + SIZE_WIDTH + BAR_WIDTH + AGE_WIDTH + GAP * 4;
        let rest = area.width.saturating_sub(fixed);
        let name = rest * 2 / 5;
        let folder = rest - name;
        let largest = found.first().map_or(0, |&id| tree.get(id).own_size);
        let now = SystemTime::now();
        let rows: Vec<Row> = found
            .iter()
            .take(MAX_ROWS)
            .map(|&id| {
                row(
                    tree,
                    id,
                    now,
                    largest,
                    usize::from(name),
                    usize::from(folder),
                )
            })
            .collect();
        let header = Row::new([
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from(""),
            Cell::from("Last changed"),
            Cell::from("Name"),
            Cell::from("Folder"),
        ])
        .bold()
        .bottom_margin(1);
        let table = Table::new(
            rows,
            [
                Constraint::Length(SIZE_WIDTH),
                Constraint::Length(BAR_WIDTH),
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

    /// The selected file's name, and lines on where it is, how big and old,
    /// and what it likely is. The folder is shortened to fit `width`.
    fn selected_details(
        &self,
        tree: &Tree,
        found: &[NodeId],
        width: usize,
    ) -> Option<(String, Vec<Line<'static>>)> {
        let id = self.selected()?;
        let node = tree.get(id);
        let path = display_path(tree, id);
        let folder = path.rsplit_once('/').map_or("~", |(parent, _)| parent);
        let total: u64 = found.iter().map(|&id| tree.get(id).own_size).sum();
        let changed = node
            .modified
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .map_or_else(|| "unknown".to_string(), format::age);
        let name = node.name.to_string_lossy().into_owned();
        let (kind, about) = kind_of(&name);
        let mut lines = vec![
            field(
                "Folder",
                Span::raw(format::shorten_path(folder, width.saturating_sub(9))),
            ),
            field(
                "Size",
                format::size_span(node.own_size, format::size(node.own_size)),
            ),
            field(
                "Share",
                Span::raw(format!(
                    "{}% of the files found",
                    format::percent(node.own_size, total)
                )),
            ),
            field("Changed", Span::raw(changed)),
            field("Type", Span::raw(kind)),
        ];
        if let Some(about) = about {
            lines.push(Line::default());
            lines.push(Line::from(about));
        }
        Some((name, lines))
    }

    /// The right side: the selected file, sized to fit, and where the files
    /// are below it
    fn draw_side(&self, frame: &mut Frame, area: Rect, tree: &Tree, found: &[NodeId]) {
        let width = usize::from(area.width.saturating_sub(4));
        let Some((name, lines)) = self.selected_details(tree, found, width) else {
            Self::draw_where(frame, area, tree, found);
            return;
        };
        let height = u16::try_from(rows_used(&lines, width) + 2).unwrap_or(u16::MAX);
        let [selected, places] =
            Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                Block::bordered()
                    .title(Line::from(format!(" {name} ")).bold())
                    .title_bottom(Line::from(" Enter shows it in Disk ").right_aligned())
                    .padding(Padding::horizontal(1)),
            ),
            selected,
        );
        Self::draw_where(frame, places, tree, found);
    }

    /// Where the files found are, by folder, largest first
    fn draw_where(frame: &mut Frame, area: Rect, tree: &Tree, found: &[NodeId]) {
        let block = Block::bordered()
            .title(" Where they are ")
            .padding(Padding::horizontal(1));
        let groups = by_folder(tree, found);
        if groups.is_empty() {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            centered(frame, inner, "Nothing to list.");
            return;
        }
        let width = usize::from(area.width.saturating_sub(4));
        let rows = usize::from(area.height.saturating_sub(2));
        let largest = groups.first().map_or(0, |(_, size)| *size);
        let mut lines: Vec<Line> = groups
            .iter()
            .take(rows)
            .map(|(folder, size)| {
                Line::from(vec![
                    format::size_span(*size, format!("{:>9} ", format::size(*size))),
                    format::size_bar(*size, largest, usize::from(FOLDER_BAR)),
                    Span::raw(" "),
                    Span::raw(format::shorten_path(
                        folder,
                        width.saturating_sub(11 + usize::from(FOLDER_BAR)),
                    )),
                ])
            })
            .collect();
        if groups.len() > lines.len() && !lines.is_empty() {
            lines.pop();
            lines.push(Line::from(format!(
                "{:>9} and {} more",
                "",
                format::count(u64::try_from(groups.len() - lines.len()).unwrap_or(u64::MAX))
            )));
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
}

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<9}")).bold(), value])
}

/// `text` in the middle of `area`
fn centered(frame: &mut Frame, area: Rect, text: &'static str) {
    let line = Line::from(text);
    let height = rows_used(std::slice::from_ref(&line), usize::from(area.width));
    let [middle] = Layout::vertical([Constraint::Length(
        u16::try_from(height).unwrap_or(u16::MAX),
    )])
    .flex(Flex::Center)
    .areas(area);
    frame.render_widget(
        Paragraph::new(line).centered().wrap(Wrap { trim: true }),
        middle,
    );
}

/// The files found added up by folder, largest first. Files in the home
/// folder count under it, and `~/Library` is split one level further, since
/// most of it is there.
fn by_folder(tree: &Tree, found: &[NodeId]) -> Vec<(String, u64)> {
    let mut sizes: HashMap<String, u64> = HashMap::new();
    for &id in found {
        let path = display_path(tree, id);
        let parts: Vec<&str> = path.split('/').collect();
        // The file itself is the last part, so it never names a folder.
        let depth = if parts.get(1) == Some(&"Library") {
            3
        } else {
            2
        };
        let folder = parts[..depth.min(parts.len() - 1)].join("/");
        *sizes.entry(folder).or_default() += tree.get(id).own_size;
    }
    let mut groups: Vec<(String, u64)> = sizes.into_iter().collect();
    groups.sort_by(|a, b| Reverse(a.1).cmp(&Reverse(b.1)).then_with(|| a.0.cmp(&b.0)));
    groups
}

/// What kind of file a name is, and a plain hint about it, from its
/// extension
fn kind_of(name: &str) -> (&'static str, Option<&'static str>) {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "dmg" | "pkg" | "iso" => (
            "Disk image or installer",
            Some("Rarely needed once the app is installed."),
        ),
        "zip" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "tar" | "zst" => (
            "Archive",
            Some("Often safe to delete once it has been unpacked."),
        ),
        "mov" | "mp4" | "m4v" | "mkv" | "avi" | "webm" => (
            "Video",
            Some("Your own media. Back it up before you remove it."),
        ),
        "raw" | "img" | "vmdk" | "vdi" | "qcow2" | "sparseimage" | "sparsebundle" => (
            "Virtual disk",
            Some("Used by a virtual machine or Docker. Free it from the app that made it."),
        ),
        "bin" | "safetensors" | "gguf" | "pt" | "onnx" | "mlmodel" => (
            "Data or model file",
            Some("Usually downloaded by an app, which may download it again."),
        ),
        "ipsw" => (
            "Device firmware",
            Some("Apple software for an iPhone or iPad. It can be downloaded again."),
        ),
        "sqlite" | "db" | "sqlite3" => (
            "Database",
            Some("An app's own data. Leave it unless you know the app."),
        ),
        "log" => (
            "Log",
            Some("A record an app wrote. Usually safe to remove."),
        ),
        _ => ("File", None),
    }
}

/// One file as a table row. `name` and `folder` are the widths of those
/// columns, so long text is shortened at its least useful end.
fn row(
    tree: &Tree,
    id: NodeId,
    now: SystemTime,
    largest: u64,
    name: usize,
    folder: usize,
) -> Row<'static> {
    let node = tree.get(id);
    let size = format::size_span(node.own_size, format::size(node.own_size));

    let elapsed = node
        .modified
        .and_then(|modified| now.duration_since(modified).ok());
    let age = Span::raw(elapsed.map_or_else(|| "unknown".to_string(), format::age));
    // Files left alone over a year stand out, since they are the likeliest
    // to be forgotten.
    let age = match elapsed {
        Some(elapsed) if elapsed >= DAY * 365 => age.magenta(),
        _ => age,
    };

    let path = display_path(tree, id);
    let (parent, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
    Row::new([
        Cell::from(Line::from(size).right_aligned()),
        Cell::from(format::size_bar(
            node.own_size,
            largest,
            usize::from(BAR_WIDTH),
        )),
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
        let (left, side) = if area.width >= MIN_SIDE_WIDTH {
            let [left, side] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDE_WIDTH)])
                    .areas(area);
            (left, Some(side))
        } else {
            (area, None)
        };
        let [filters, table] =
            Layout::vertical([Constraint::Length(4), Constraint::Fill(1)]).areas(left);
        self.draw_filters(frame, filters);
        self.draw_table(frame, table, tree, &found);
        if let Some(side) = side {
            self.draw_side(frame, side, tree, &found);
        }
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

        assert!(screen.contains("[100 MB]"));
        assert!(screen.contains("2 files · 2.3 GB in all"));
        assert!(screen.contains("Last changed"));
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
        assert!(screen.contains("[500 MB]"));
        assert!(screen.contains("1 file · 2.0 GB in all"));

        for _ in 0..4 {
            press(&mut large, &scan, KeyCode::Char('s'));
        }
        press(&mut large, &scan, KeyCode::Char('a'));
        let screen = render(&mut large, &scan);
        assert!(screen.contains("[50 MB]"));
        assert!(screen.contains("[30 days]"));
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

    #[test]
    fn a_wide_screen_shows_the_selected_file_and_where_files_are() {
        let scan = done();
        let mut terminal = Terminal::new(TestBackend::new(140, 20)).unwrap();
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        terminal
            .draw(|frame| LargeFiles::new().draw(frame, frame.area(), &context))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

        assert!(screen.contains("film.mov"));
        assert!(screen.contains("Type     Video"));
        assert!(screen.contains("86% of the files found"));
        assert!(screen.contains("Where they are"));
        assert!(screen.contains("~/Movies"));
        assert!(screen.contains("Enter shows it in Disk"));
    }

    #[test]
    fn nothing_matching_says_so_in_the_middle() {
        let scan = done();
        let mut large = LargeFiles::new();
        for _ in 0..3 {
            press(&mut large, &scan, KeyCode::Char('s'));
        }
        let screen = render(&mut large, &scan);

        assert!(screen.contains("[5 GB]"));
        assert!(screen.contains("No files match"));
    }

    #[test]
    fn files_add_up_by_folder_with_library_split() {
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let library = tree.add(root, "Library", NodeKind::Directory, 0);
        let caches = tree.add(library, "Caches", NodeKind::Directory, 0);
        let app = tree.add(caches, "app", NodeKind::Directory, 0);
        let cache_file = tree.add(app, "big.bin", NodeKind::File, 300);
        let movies = tree.add(root, "Movies", NodeKind::Directory, 0);
        let film = tree.add(movies, "film.mov", NodeKind::File, 500);
        let loose = tree.add(root, "loose.zip", NodeKind::File, 100);

        assert_eq!(
            by_folder(&tree, &[film, cache_file, loose]),
            [
                ("~/Movies".to_string(), 500),
                ("~/Library/Caches".to_string(), 300),
                ("~".to_string(), 100),
            ]
        );
    }

    #[test]
    fn file_kinds_come_from_the_extension() {
        assert_eq!(kind_of("Xcode.DMG").0, "Disk image or installer");
        assert_eq!(kind_of("Docker.raw").0, "Virtual disk");
        assert_eq!(kind_of("notes").0, "File");
        assert_eq!(kind_of("notes").1, None);
    }
}
