use std::sync::mpsc::{self, Receiver};
use std::thread;

use neet_core::clutter::{self, Finding, Kind};
use neet_core::tree::Tree;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::Clean;
use super::disk::Disk;
use super::format;
use super::loading::scanning;
use super::scan::ScanStatus;

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
    /// What every cleanup rule found, cleaned in the Clean screen
    Rules,
    Clutter(Kind),
}

const ITEMS: [Item; 7] = [
    Item::Rules,
    Item::Clutter(Kind::Trash),
    Item::Clutter(Kind::Installers),
    Item::Clutter(Kind::BuildFolders),
    Item::Clutter(Kind::DockerImage),
    Item::Clutter(Kind::SimulatorRuntimes),
    Item::Clutter(Kind::TempFiles),
];

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
        Item::Clutter(Kind::SimulatorRuntimes) => "Simulator runtimes",
        Item::Clutter(Kind::TempFiles) => "Temporary files",
    }
}

/// Who clears an item
#[derive(Clone, Copy, PartialEq, Eq)]
enum Who {
    Neet,
    You,
    /// macOS clears it on its own
    Mac,
}

fn who(item: Item) -> Who {
    match item {
        Item::Rules => Who::Neet,
        Item::Clutter(Kind::TempFiles) => Who::Mac,
        Item::Clutter(_) => Who::You,
    }
}

/// The step that frees an item, in a few words
fn step(item: Item) -> &'static str {
    match item {
        Item::Rules => "Enter, then review in Clean",
        Item::Clutter(Kind::Trash) => "Empty the Trash",
        Item::Clutter(Kind::Installers) => "Delete in Finder",
        Item::Clutter(Kind::BuildFolders) => "cargo clean, or rm -rf node_modules",
        Item::Clutter(Kind::DockerImage) => "docker system prune",
        Item::Clutter(Kind::SimulatorRuntimes) => "Xcode, Settings, Components",
        Item::Clutter(Kind::TempFiles) => "Restart the Mac",
    }
}

/// What the item is, and how to remove it
fn explain(item: Item) -> [&'static str; 2] {
    match item {
        Item::Rules => [
            "Caches and logs the cleanup rules found. Apps make them again when needed.",
            "Press Enter to choose them in Clean, review every path, and move them to the Trash.",
        ],
        Item::Clutter(Kind::Trash) => [
            "Items already in the Trash still take space.",
            "Empty the Trash in Finder to free it. neet never empties the Trash.",
        ],
        Item::Clutter(Kind::Installers) => [
            "Disk images and installers in Downloads. Once an app is installed, its installer is rarely needed.",
            "Downloads is protected, so delete them in Finder. Enter shows the largest in Disk.",
        ],
        Item::Clutter(Kind::BuildFolders) => [
            "node_modules folders, and Rust target folders, in your projects. They come back when you install or build again.",
            "Delete them in the project, such as with rm -rf node_modules or cargo clean. Enter shows the largest in Disk.",
        ],
        Item::Clutter(Kind::DockerImage) => [
            "The disk image Docker Desktop keeps containers and images in. It does not shrink on its own.",
            "Remove what you do not use in Docker Desktop, or with docker system prune. Enter shows it in Disk.",
        ],
        Item::Clutter(Kind::SimulatorRuntimes) => [
            "iOS and other simulator runtimes Xcode downloaded. They live outside your home folder.",
            "Delete the ones you do not use in Xcode, Settings, Components, or with xcrun simctl runtime delete.",
        ],
        Item::Clutter(Kind::TempFiles) => [
            "Your temporary files and caches in /private/var/folders. Apps use them while they run.",
            "macOS removes old ones on its own, and restarting the Mac clears more. Do not delete them by hand.",
        ],
    }
}

/// Everything taking space that can be cleared, in one table: what neet
/// cleans itself, and what you remove with the right tool.
pub struct QuickClean {
    table: TableState,
    from_tree: Option<Vec<Finding>>,
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
        let mut neet_total = 0;
        let mut your_total = 0;
        let rows: Vec<Row> = ITEMS
            .iter()
            .map(|&item| {
                let status = self.status(item, context);
                let by_neet = item == Item::Rules;
                let cleared_by = match who(item) {
                    Who::Neet => Span::raw("neet").green(),
                    Who::You => Span::raw("you").yellow(),
                    Who::Mac => Span::raw("macOS").dark_gray(),
                };
                let (size, found) = match status {
                    Status::Found { size, count } => {
                        match who(item) {
                            Who::Neet => neet_total += size,
                            Who::You => your_total += size,
                            Who::Mac => {}
                        }
                        let text = format!("{:>9}", format::size(size));
                        let size = if by_neet {
                            Span::raw(format!("~{}", text.trim_start())).bold()
                        } else {
                            format::size_span(size, text)
                        };
                        let found = count.map_or_else(String::new, |count| {
                            format::count(u64::try_from(count).unwrap_or(u64::MAX))
                        });
                        (size, Span::raw(found))
                    }
                    Status::Looking => (Span::raw("looking…").dark_gray(), Span::raw("")),
                    Status::Nothing => (Span::raw("none").dark_gray(), Span::raw("")),
                };
                Row::new([
                    Cell::from(name(item)),
                    Cell::from(Line::from(size).right_aligned()),
                    Cell::from(Line::from(found.dark_gray()).right_aligned()),
                    Cell::from(cleared_by),
                    Cell::from(Span::raw(step(item)).dark_gray()),
                ])
            })
            .collect();
        let header = Row::new([
            Cell::from("Item"),
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from(Line::from("Found").right_aligned()),
            Cell::from("Cleared by"),
            Cell::from("How"),
        ])
        .bold()
        .bottom_margin(1);
        let summary = Line::from(vec![
            Span::raw(" neet can clean "),
            Span::raw(format!("~{}", format::size(neet_total)))
                .green()
                .bold(),
            Span::raw(" · you can free "),
            Span::raw(format::size(your_total)).yellow().bold(),
            Span::raw(" more "),
        ]);
        let table = Table::new(
            rows,
            [
                Constraint::Length(24),
                Constraint::Length(10),
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(
            Block::bordered()
                .title(" Quick Clean ")
                .title_bottom(summary.right_aligned())
                .padding(Padding::horizontal(1)),
        )
        .highlight_symbol("▸ ")
        .row_highlight_style(Style::new().bold().cyan());
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    fn draw_details(&self, frame: &mut Frame, area: Rect) {
        let item = self.selected();
        let [what, how] = explain(item);
        let lines = vec![
            Line::from(name(item)).bold(),
            Line::from(what),
            Line::default(),
            Line::from(how).cyan(),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(Block::bordered().padding(Padding::horizontal(1))),
            area,
        );
    }
}

impl Screen for QuickClean {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let tree = match context.scan {
            ScanStatus::Done { scan, .. } => &scan.tree,
            ScanStatus::Running(progress) => {
                scanning(
                    "Quick Clean",
                    "What can be cleared shows here when the scan finishes.",
                    *progress,
                )
                .draw(frame, area);
                return;
            }
            ScanStatus::Failed(reason) => {
                frame.render_widget(
                    Paragraph::new(format!(
                        "The scan failed, so there is nothing to show. {reason}"
                    ))
                    .red()
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::bordered()
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
        let [table, details] =
            Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)]).areas(area);
        self.draw_table(frame, table, context);
        self.draw_details(frame, details);
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
                    if let ScanStatus::Done { scan, .. } = context.scan
                        && let Some(node) = self.finding(kind).and_then(|found| found.node)
                    {
                        return Action::Open(Box::new(Disk::showing(&scan.tree, node)));
                    }
                }
            },
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · enter clean or show in Disk · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("g  G", "Jump to the first or last row"),
            (
                "Enter  →  l",
                "Caches and logs: choose them in Clean. Others: show in Disk",
            ),
            ("Esc", "Go back to Home"),
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

    fn done() -> ScanStatus {
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let trash = tree.add(root, ".Trash", NodeKind::Directory, 0);
        let _ = tree.add(trash, "old.zip", NodeKind::File, 2_000_000_000);
        let downloads = tree.add(root, "Downloads", NodeKind::Directory, 0);
        let _ = tree.add(downloads, "App.dmg", NodeKind::File, 300_000_000);
        ScanStatus::Done {
            scan: Scan {
                tree,
                errors: Vec::new(),
                other_disks: Vec::new(),
            },
            elapsed: std::time::Duration::from_secs(1),
        }
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

    fn render(screen: &mut QuickClean, scan: &ScanStatus) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
        let context = Context {
            scan,
            disk: None,
            cleanable: Some(8_600_000_000),
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

    #[test]
    fn lists_what_neet_cleans_and_what_you_remove() {
        let scan = done();
        let screen = render(&mut quick(), &scan);

        assert!(screen.contains("~8.6 GB"));
        assert!(screen.contains("Trash"));
        assert!(screen.contains("2.0 GB"));
        assert!(screen.contains("300.0 MB"));
        assert!(screen.contains("17.0 GB"));
        assert!(screen.contains("looking…"));
        assert!(screen.contains("none"));
        assert!(screen.contains("you can free 19.3 GB more"));
        assert!(screen.contains("macOS"));
        assert!(screen.contains("Empty the Trash"));
    }

    #[test]
    fn enter_opens_clean_then_disk_for_a_finding() {
        let scan = done();
        let mut screen = quick();
        let _ = render(&mut screen, &scan);
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

        assert!(matches!(
            screen.handle_key(enter, &context),
            Action::Open(_)
        ));
        let _ = screen.handle_key(down, &context);
        assert!(matches!(
            screen.handle_key(enter, &context),
            Action::Open(_)
        ));
        // Simulator runtimes are outside the scan, so there is nothing to show.
        for _ in 0..4 {
            let _ = screen.handle_key(down, &context);
        }
        assert!(matches!(screen.handle_key(enter, &context), Action::None));
    }
}
