use std::cmp::Reverse;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use neet_core::clutter::{self, Finding, Kind};
use neet_core::tree::{NodeId, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Clean, display_path, rows_used};
use super::disk::Disk;
use super::format;
use super::loading::scanning;
use super::pick::Pick;
use super::scan::ScanStatus;
use super::tools::{DockerSpace, Simulators};

/// Below this width the largest items are left out.
const MIN_SIDE_WIDTH: u16 = 130;

/// Width of the table and the box under it, when the largest items are beside them
const TABLE_WIDTH: u16 = 86;

/// What the background work found, once it is done
enum Asked {
    Waiting(Receiver<Option<Finding>>),
    Done(Option<Finding>),
}

impl Asked {
    fn start(work: impl FnOnce() -> Option<Finding> + Send + 'static) -> Self {
        let (sender, answer) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(work());
        });
        Self::Waiting(answer)
    }

    fn poll(&mut self) {
        if let Self::Waiting(answer) = self
            && let Ok(found) = answer.try_recv()
        {
            *self = Self::Done(found);
        }
    }
}

/// One row of the table, in a fixed order so the selection never jumps
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    /// What every cleanup rule found, cleaned in the Deep Clean screen
    Rules,
    Clutter(Kind),
}

/// What neet clears first, then what you clear, then what macOS clears
const ITEMS: [Item; 7] = [
    Item::Rules,
    Item::Clutter(Kind::BuildFolders),
    Item::Clutter(Kind::Installers),
    Item::Clutter(Kind::Trash),
    Item::Clutter(Kind::DockerImage),
    Item::Clutter(Kind::SimulatorRuntimes),
    Item::Clutter(Kind::TempFiles),
];

/// Width of the bar that shows each row's share of everything found
const SHARE_BAR: usize = 14;

/// What a row shows: found, still looking, or nothing
enum Status {
    Found { size: u64, count: Option<usize> },
    Looking,
    Nothing,
}

fn name(item: Item) -> &'static str {
    match item {
        Item::Rules => "Caches and logs",
        Item::Clutter(Kind::Trash) => "Trash",
        Item::Clutter(Kind::Installers) => "Installers in Downloads",
        Item::Clutter(Kind::BuildFolders) => "Project build folders",
        Item::Clutter(Kind::DockerImage) => "Docker disk image",
        Item::Clutter(Kind::SimulatorRuntimes) => "Simulators",
        Item::Clutter(Kind::TempFiles) => "Temporary files",
    }
}

/// Who clears an item
#[derive(Clone, Copy, PartialEq, Eq)]
enum Who {
    /// neet moves it to the Trash
    Neet,
    /// neet asks the item's own tool to remove it, which is permanent
    Tool,
    You,
    /// macOS clears it on its own
    Mac,
}

fn who(item: Item) -> Who {
    match item {
        Item::Rules => Who::Neet,
        Item::Clutter(kind) if clutter::neet_removes(kind) => Who::Neet,
        Item::Clutter(Kind::DockerImage | Kind::SimulatorRuntimes) => Who::Tool,
        Item::Clutter(Kind::TempFiles) => Who::Mac,
        Item::Clutter(_) => Who::You,
    }
}

fn who_span(who: Who) -> Span<'static> {
    match who {
        Who::Neet => Span::raw("neet").green(),
        Who::Tool => Span::raw("neet, permanently").red(),
        Who::You => Span::raw("you").yellow(),
        Who::Mac => Span::raw("macOS").light_blue(),
    }
}

/// What the item is
fn about(item: Item) -> &'static str {
    match item {
        Item::Rules => "Caches and logs the cleanup rules found. Apps make them again when needed.",
        Item::Clutter(Kind::Trash) => {
            "Items already in the Trash still take space until it is emptied."
        }
        Item::Clutter(Kind::Installers) => {
            "Disk images and installers in Downloads. Once an app is installed, its installer is rarely needed."
        }
        Item::Clutter(Kind::BuildFolders) => {
            "Project node_modules and Rust target folders. Recreated on install or build."
        }
        Item::Clutter(Kind::DockerImage) => {
            "The disk image Docker Desktop keeps containers and images in. It does not shrink on its own."
        }
        Item::Clutter(Kind::SimulatorRuntimes) => {
            "Xcode runtimes and their simulators, outside the home folder."
        }
        Item::Clutter(Kind::TempFiles) => "Temporary app files in /private/var/folders.",
    }
}

/// The steps that free an item
fn steps(item: Item) -> &'static [&'static str] {
    match item {
        Item::Rules => &[
            "Enter: open Deep Clean.",
            "Space: select rules. Review every path.",
            "Confirm: move files to Trash.",
        ],
        Item::Clutter(Kind::BuildFolders) => &[
            "Enter: list build folders.",
            "All selected. Deselect active projects.",
            "Review and confirm: move to Trash.",
        ],
        Item::Clutter(Kind::Installers) => &[
            "Enter: list installers.",
            "All selected. Deselect installers to keep.",
            "Review and confirm: move to Trash.",
        ],
        Item::Clutter(Kind::Trash) => &[
            "Empty the Trash in Finder, or right click it in the Dock.",
            "neet never empties the Trash, so Put Back always works.",
        ],
        Item::Clutter(Kind::DockerImage) => &[
            "Enter: review Docker usage and unused volumes.",
            "Confirm: run docker system prune --all.",
            "x: reset Docker. Move its disk image to Trash.",
        ],
        Item::Clutter(Kind::SimulatorRuntimes) => &[
            "Enter: list runtimes and orphaned simulators. None selected.",
            "Select runtimes to remove with their simulators.",
            "Confirm permanent removal. Skips Trash; cannot undo.",
        ],
        Item::Clutter(Kind::TempFiles) => &[
            "macOS removes old ones on its own.",
            "Restarting the Mac clears more.",
            "Do not delete them by hand.",
        ],
    }
}

/// Everything taking space that can be cleared, in one table: what neet
/// clears itself, and what you remove with the right tool.
pub struct QuickClean {
    table: TableState,
    from_tree: Option<Vec<Finding>>,
    /// Build folders and installers that are still on disk, largest first
    removable: Option<Vec<(Kind, Vec<NodeId>)>>,
    simulators: Asked,
    temp: Asked,
}

impl QuickClean {
    pub fn new() -> Self {
        Self::with(
            Asked::start(|| clutter::simulator_runtimes().ok().flatten()),
            Asked::start(|| clutter::temp_files().ok()),
        )
    }

    fn with(simulators: Asked, temp: Asked) -> Self {
        Self {
            table: TableState::default().with_selected(Some(0)),
            from_tree: None,
            removable: None,
            simulators,
            temp,
        }
    }

    fn poll(&mut self, tree: &Tree) {
        self.simulators.poll();
        self.temp.poll();
        if self.from_tree.is_none() {
            self.from_tree = Some(clutter::from_tree(tree));
        }
        if self.removable.is_none() {
            // Empty ones free nothing. The scan is from when neet opened, so
            // leave out anything already moved since.
            self.removable = Some(
                [Kind::BuildFolders, Kind::Installers]
                    .into_iter()
                    .map(|kind| {
                        let ids = clutter::items(tree, kind)
                            .into_iter()
                            .filter(|&id| tree.get(id).total_size > 0)
                            .filter(|&id| fs::symlink_metadata(tree.path(id)).is_ok())
                            .collect();
                        (kind, ids)
                    })
                    .collect(),
            );
        }
    }

    /// The items neet can move of `kind`, largest first
    fn removable(&self, kind: Kind) -> &[NodeId] {
        self.removable
            .as_ref()
            .and_then(|lists| lists.iter().find(|(found, _)| *found == kind))
            .map_or(&[], |(_, ids)| ids.as_slice())
    }

    fn finding(&self, kind: Kind) -> Option<&Finding> {
        match kind {
            Kind::SimulatorRuntimes => match &self.simulators {
                Asked::Done(found) => found.as_ref(),
                Asked::Waiting(_) => None,
            },
            Kind::TempFiles => match &self.temp {
                Asked::Done(found) => found.as_ref(),
                Asked::Waiting(_) => None,
            },
            _ => self
                .from_tree
                .as_ref()?
                .iter()
                .find(|finding| finding.kind == kind),
        }
    }

    fn status(&self, item: Item, context: &Context) -> Status {
        match item {
            Item::Rules => match context.cleanable {
                Some(0) => Status::Nothing,
                Some(size) => Status::Found { size, count: None },
                None => Status::Looking,
            },
            Item::Clutter(kind) if clutter::neet_removes(kind) => {
                let ScanStatus::Done { scan, .. } = context.scan else {
                    return Status::Looking;
                };
                let ids = self.removable(kind);
                let size = ids.iter().map(|&id| scan.tree.get(id).total_size).sum();
                if size == 0 {
                    Status::Nothing
                } else {
                    Status::Found {
                        size,
                        count: Some(ids.len()),
                    }
                }
            }
            Item::Clutter(kind) => {
                let waiting = match kind {
                    Kind::SimulatorRuntimes => matches!(self.simulators, Asked::Waiting(_)),
                    Kind::TempFiles => matches!(self.temp, Asked::Waiting(_)),
                    _ => self.from_tree.is_none(),
                };
                match self.finding(kind) {
                    Some(found) if found.size > 0 => Status::Found {
                        size: found.size,
                        count: Some(found.count),
                    },
                    _ if waiting => Status::Looking,
                    _ => Status::Nothing,
                }
            }
        }
    }

    fn selected(&self) -> Item {
        ITEMS[self.table.selected().unwrap_or(0).min(ITEMS.len() - 1)]
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let statuses: Vec<Status> = ITEMS
            .iter()
            .map(|&item| self.status(item, context))
            .collect();
        let size_of = |status: &Status| match status {
            Status::Found { size, .. } => *size,
            _ => 0,
        };
        let largest = statuses.iter().map(size_of).max().unwrap_or(0);
        let columns = quick_columns(&statuses, area.width);
        let mut neet_total = 0;
        let mut your_total = 0;
        let rows: Vec<Row> = ITEMS
            .iter()
            .zip(&statuses)
            .map(|(&item, status)| {
                let (size, bar, found) = match *status {
                    Status::Found { size, count } => {
                        match who(item) {
                            Who::Neet | Who::Tool => neet_total += size,
                            Who::You => your_total += size,
                            Who::Mac => {}
                        }
                        let text = if item == Item::Rules {
                            format!("~{}", format::size(size))
                        } else {
                            format::size(size)
                        };
                        let found = count.map_or_else(String::new, |count| {
                            format::count(u64::try_from(count).unwrap_or(u64::MAX))
                        });
                        (
                            format::size_span(size, text),
                            format::size_bar(size, largest, SHARE_BAR),
                            found,
                        )
                    }
                    Status::Looking => (Span::raw("scanning").cyan(), Span::raw(""), String::new()),
                    Status::Nothing => (Span::raw("none"), Span::raw(""), String::new()),
                };
                columns.row([
                    Cell::from(format::shorten_middle(name(item), columns.width(0))),
                    Cell::from(Line::from(size).right_aligned()),
                    Cell::from(bar),
                    Cell::from(Line::from(found).right_aligned()),
                    Cell::from(who_span(who(item))),
                ])
            })
            .collect();
        let header = columns
            .row([
                Cell::from("Item"),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from(""),
                Cell::from(Line::from("Found").right_aligned()),
                Cell::from("Cleared by"),
            ])
            .style(super::visual::HEADING)
            .bottom_margin(1);
        let summary = Line::from(vec![
            Span::raw(" Cleanup: "),
            Span::raw(format!("~{}", format::size(neet_total)))
                .green()
                .bold(),
            Span::raw(" · Manual: "),
            Span::raw(format::size(your_total)).yellow().bold(),
            Span::raw(" "),
        ]);
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(
                super::visual::block()
                    .title(" Quick Clean ")
                    .title_bottom(summary.right_aligned())
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// What the selected row is and how to clear it, numbered
    fn draw_about(&self, frame: &mut Frame, area: Rect) {
        let item = self.selected();
        let mut lines = Vec::new();
        let color = match who(item) {
            Who::Neet => Color::Green,
            Who::Tool => Color::Red,
            Who::You => Color::Yellow,
            Who::Mac => Color::LightBlue,
        };
        for (number, step) in steps(item).iter().enumerate() {
            lines.push(Line::from(vec![
                Span::raw(format!("{}. ", number + 1)).fg(color),
                Span::raw(*step),
            ]));
        }
        let mut expanded = lines.clone();
        expanded.push(Line::default());
        expanded.push(Line::from(about(item)));
        if super::visual::wrapped_rows(&expanded, area.width.saturating_sub(4))
            <= area.height.saturating_sub(2)
        {
            lines = expanded;
        }
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                super::visual::block()
                    .title(format!(" {} ", name(item)))
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
    }

    /// The largest items of the selected row, when there is a list to show
    fn draw_largest(&self, frame: &mut Frame, area: Rect, context: &Context) {
        let item = self.selected();
        let width = usize::from(area.width.saturating_sub(4));
        let rows = usize::from(area.height.saturating_sub(2));
        let entries: Vec<(u64, String)> = match item {
            Item::Rules => context
                .plan
                .and_then(|estimate| estimate.planned.as_ref())
                .map(|planned| {
                    let mut rules: Vec<_> = planned
                        .plan
                        .rules
                        .iter()
                        .filter(|rule| rule.size() > 0)
                        .map(|rule| (rule.size(), rule.rule.name.clone()))
                        .collect();
                    rules.sort_by_key(|(size, _)| Reverse(*size));
                    rules
                })
                .unwrap_or_default(),
            Item::Clutter(kind) => match context.scan {
                ScanStatus::Done { scan, .. } => {
                    let home = scan.tree.path(scan.tree.root());
                    let ids: Vec<NodeId> = if clutter::neet_removes(kind) {
                        self.removable(kind).to_vec()
                    } else {
                        self.finding(kind)
                            .and_then(|found| found.node)
                            .into_iter()
                            .collect()
                    };
                    ids.into_iter()
                        .map(|id| {
                            (
                                scan.tree.get(id).total_size,
                                display_path(&home, &scan.tree.path(id)),
                            )
                        })
                        .collect()
                }
                _ => Vec::new(),
            },
        };
        let mut lines: Vec<Line> = entries
            .iter()
            .take(rows)
            .map(|(size, path)| {
                Line::from(vec![
                    format::size_span(*size, format!("{:>9}  ", format::size(*size))),
                    Span::raw(format::shorten_path(path, width.saturating_sub(11))),
                ])
            })
            .collect();
        if entries.len() > lines.len() && !lines.is_empty() {
            lines.pop();
            lines.push(Line::from(format!(
                "{:>9}  and {} more",
                "",
                format::count(u64::try_from(entries.len() - lines.len()).unwrap_or(u64::MAX))
            )));
        }
        let block = super::visual::block()
            .title(" Largest ")
            .padding(Padding::horizontal(1));
        if lines.is_empty() {
            // With no list, the reason sits in the middle of the box
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let why = Line::from(match item {
                Item::Clutter(Kind::SimulatorRuntimes | Kind::TempFiles) => {
                    "Outside home. No scan details."
                }
                _ => "Nothing to list.",
            });
            let height = rows_used(std::slice::from_ref(&why), usize::from(inner.width));
            let [middle] = Layout::vertical([Constraint::Length(
                u16::try_from(height).unwrap_or(u16::MAX),
            )])
            .flex(Flex::Center)
            .areas(inner);
            let why = Paragraph::new(why).centered().wrap(Wrap { trim: true });
            frame.render_widget(why, middle);
            return;
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
}

fn quick_columns(statuses: &[Status], width: u16) -> super::visual::Columns<5> {
    let sizes = format::column_width(
        "Size",
        statuses.iter().map(|status| {
            let size = format::size(match status {
                Status::Found { size, .. } => *size,
                _ => 0,
            });
            format!("~{size}")
        }),
    )
    .max(8);
    let counts = format::column_width(
        "Found",
        statuses.iter().filter_map(|status| match status {
            Status::Found {
                count: Some(count), ..
            } => Some(format::count(u64::try_from(*count).unwrap_or(u64::MAX))),
            _ => None,
        }),
    );
    super::visual::Columns::new(
        width,
        [(18, 1), (sizes, 0), (14, 0), (counts, 0), (17, 0)],
        &[2],
        true,
    )
}

impl Screen for QuickClean {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let tree = match context.scan {
            ScanStatus::Done { scan, .. } => &scan.tree,
            ScanStatus::Running(progress) => {
                scanning("Quick Clean", "Finding cleanup candidates.", *progress).draw(frame, area);
                return;
            }
            ScanStatus::Failed(reason) => {
                frame.render_widget(
                    Paragraph::new(format!("Scan failed: {reason}"))
                        .red()
                        .wrap(Wrap { trim: true })
                        .block(
                            super::visual::block()
                                .title(" Quick Clean ")
                                .padding(Padding::horizontal(1)),
                        ),
                    area,
                );
                return;
            }
        };
        self.poll(tree);
        // Border, header, gap, then one row per item
        let table_height = u16::try_from(ITEMS.len()).unwrap_or(u16::MAX) + 4;
        if area.width >= MIN_SIDE_WIDTH {
            // The table and how to clear the row on the left, the largest
            // items down the whole right side
            let [left, largest] =
                Layout::horizontal([Constraint::Length(TABLE_WIDTH), Constraint::Fill(1)])
                    .areas(area);
            let [table, about] =
                Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)])
                    .areas(left);
            self.draw_table(frame, table, context);
            self.draw_about(frame, about);
            self.draw_largest(frame, largest, context);
        } else {
            let [table, about] =
                Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)])
                    .areas(area);
            self.draw_table(frame, table, context);
            self.draw_about(frame, about);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, context: &Context) -> Action {
        let index = self.table.selected().unwrap_or(0);
        let last = ITEMS.len() - 1;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.table.select(Some(index.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => self.table.select(Some((index + 1).min(last))),
            KeyCode::Char('g') | KeyCode::Home => self.table.select(Some(0)),
            KeyCode::Char('G') | KeyCode::End => self.table.select(Some(last)),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => match self.selected() {
                Item::Rules => return Action::Open(Box::new(Clean::new())),
                Item::Clutter(kind) => {
                    let ScanStatus::Done { scan, .. } = context.scan else {
                        return Action::None;
                    };
                    if kind == Kind::SimulatorRuntimes {
                        return Action::Open(Box::new(Simulators::new()));
                    }
                    if kind == Kind::DockerImage {
                        return Action::Open(Box::new(DockerSpace::new()));
                    }
                    if clutter::neet_removes(kind) {
                        let paths: Vec<PathBuf> = self
                            .removable(kind)
                            .iter()
                            .map(|&id| scan.tree.path(id))
                            .collect();
                        if !paths.is_empty() {
                            return Action::Open(Box::new(Pick::start(
                                kind,
                                name(Item::Clutter(kind)),
                                paths,
                            )));
                        }
                    } else if let Some(node) = self.finding(kind).and_then(|found| found.node) {
                        return Action::Open(Box::new(Disk::showing(&scan.tree, node)));
                    }
                }
            },
            KeyCode::Char('d') => {
                if let (Item::Clutter(kind), ScanStatus::Done { scan, .. }) =
                    (self.selected(), context.scan)
                    && let Some(node) = self.finding(kind).and_then(|found| found.node)
                {
                    return Action::Open(Box::new(Disk::showing(&scan.tree, node)));
                }
            }
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · enter clear it · d show in Disk · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("g  G", "Jump to the first or last row"),
            (
                "Enter  →  l",
                "Rows neet clears: pick what to remove. Trash: show in Disk",
            ),
            ("d", "Show the largest item in Disk"),
            ("Esc", "Back to Home"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neet_core::scan;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use tempfile::TempDir;

    /// A real home folder, scanned, so items can be checked on disk
    fn done() -> (TempDir, ScanStatus) {
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        let home = fs::canonicalize(dir.path()).expect("home should resolve");
        for (file, size) in [
            (".Trash/old.zip", 2_000_000),
            ("Downloads/App.dmg", 300_000),
            ("code/web/node_modules/x.js", 1_000_000),
        ] {
            let path = home.join(file);
            fs::create_dir_all(path.parent().unwrap()).expect("folder should be made");
            fs::write(path, vec![7u8; size]).expect("file should be written");
        }
        let scan = scan::scan(&home, |_| {}).expect("scan should finish");
        (
            dir,
            ScanStatus::Done {
                scan,
                elapsed: std::time::Duration::from_secs(1),
            },
        )
    }

    fn quick() -> QuickClean {
        let simulator = Finding {
            kind: Kind::SimulatorRuntimes,
            size: 17_000_000_000,
            count: 2,
            node: None,
        };
        QuickClean::with(
            Asked::Done(Some(simulator)),
            Asked::Waiting(mpsc::channel().1),
        )
    }

    fn context(scan: &ScanStatus) -> Context<'_> {
        Context {
            scan,
            disk: None,
            cleanable: Some(8_600_000_000),
            plan: None,
        }
    }

    fn render(screen: &mut QuickClean, scan: &ScanStatus) -> String {
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        terminal
            .draw(|frame| screen.draw(frame, frame.area(), &context(scan)))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    fn press(screen: &mut QuickClean, scan: &ScanStatus, code: KeyCode) -> Action {
        screen.handle_key(KeyEvent::new(code, KeyModifiers::NONE), &context(scan))
    }

    #[test]
    fn lists_what_neet_clears_and_what_you_remove() {
        let (_dir, scan) = done();
        let screen = render(&mut quick(), &scan);

        assert!(screen.contains("~8.6 GB"));
        assert!(screen.contains("Trash"));
        assert!(screen.contains("2.0 MB"));
        assert!(screen.contains("1.0 MB"));
        assert!(screen.contains("300.0 KB") || screen.contains("303.1 KB"));
        assert!(screen.contains("17.0 GB"));
        assert!(screen.contains("scanning"));
        assert!(screen.contains("Manual: 2.0 MB"));
        assert!(screen.contains("neet, permanently"));
        assert!(screen.contains("macOS"));
        assert!(screen.contains("Enter: open Deep Clean"));
    }

    #[test]
    fn build_folders_show_their_paths_and_open_the_picker() {
        let (_dir, scan) = done();
        let mut screen = quick();
        let _ = render(&mut screen, &scan);

        press(&mut screen, &scan, KeyCode::Down);
        let text = render(&mut screen, &scan);
        assert!(text.contains("~/code/web/node_modules"));
        assert!(text.contains("Enter: list build folders"));
        assert!(matches!(
            press(&mut screen, &scan, KeyCode::Enter),
            Action::Open(_)
        ));
    }

    #[test]
    fn items_already_moved_are_left_out() {
        let (dir, scan) = done();
        fs::remove_dir_all(dir.path().join("code/web/node_modules")).expect("folder should go");
        let mut screen = quick();
        let _ = render(&mut screen, &scan);

        press(&mut screen, &scan, KeyCode::Down);
        assert!(matches!(
            press(&mut screen, &scan, KeyCode::Enter),
            Action::None
        ));
    }

    #[test]
    fn enter_opens_deep_clean_disk_and_the_tool_screens() {
        let (_dir, scan) = done();
        let mut screen = quick();
        let _ = render(&mut screen, &scan);

        assert!(matches!(
            press(&mut screen, &scan, KeyCode::Enter),
            Action::Open(_)
        ));
        for _ in 0..3 {
            press(&mut screen, &scan, KeyCode::Down);
        }
        assert!(matches!(
            press(&mut screen, &scan, KeyCode::Enter),
            Action::Open(_)
        ));
        // Simulator runtimes open their own screen, which asks simctl.
        for _ in 0..2 {
            press(&mut screen, &scan, KeyCode::Down);
        }
        assert!(matches!(
            press(&mut screen, &scan, KeyCode::Enter),
            Action::Open(_)
        ));
    }
    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let (_dir, scan) = done();
        let mut screen = quick();
        for (width, height) in view::SIZES {
            let buffer = view::render("quick", &mut screen, &context(&scan), width, height);
            view::aligned(&buffer, "Size", "~8.6 GB");
            assert!(view::text(&buffer).contains("neet, permanently"));
            assert!(view::text(&buffer).contains("macOS"));
        }
    }
}
