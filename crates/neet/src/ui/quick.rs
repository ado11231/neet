use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use neet_core::clutter::{self, Finding, Kind};
use neet_core::tree::{NodeId, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Clean, Estimate, display_path};
use super::disk::Disk;
use super::format;
use super::loading::scanning;
use super::pick::Pick;
use super::scan::ScanStatus;
use super::tools::{DockerSpace, Simulators, Trash};

/// What the background work found, once it is done
enum Asked {
    Waiting(Receiver<io::Result<Option<Finding>>>),
    Done(Option<Finding>),
    Failed,
}

impl Asked {
    fn start(work: impl FnOnce() -> io::Result<Option<Finding>> + Send + 'static) -> Self {
        let (sender, answer) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(work());
        });
        Self::Waiting(answer)
    }

    fn poll(&mut self) {
        let Self::Waiting(answer) = self else {
            return;
        };
        *self = match answer.try_recv() {
            Ok(Ok(found)) => Self::Done(found),
            Ok(Err(_)) | Err(TryRecvError::Disconnected) => Self::Failed,
            Err(TryRecvError::Empty) => return,
        };
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
const ITEMS: [Item; 8] = [
    Item::Rules,
    Item::Clutter(Kind::BuildFolders),
    Item::Clutter(Kind::Installers),
    Item::Clutter(Kind::Trash),
    Item::Clutter(Kind::DockerImage),
    Item::Clutter(Kind::SimulatorRuntimes),
    Item::Clutter(Kind::TempFiles),
    Item::Clutter(Kind::SimulatorDownloads),
];

/// Width of the bar that shows each row's share of everything found
const SHARE_BAR: u16 = 10;

/// What a row shows: found, still looking, or nothing
enum Status {
    Found {
        size: u64,
        count: Option<usize>,
    },
    Looking,
    Nothing,
    /// macOS would not let the scan read it, so its size is unknown
    NoAccess,
    Failed,
}

fn name(item: Item) -> &'static str {
    match item {
        Item::Rules => "Caches & logs",
        Item::Clutter(Kind::Trash) => "Trash",
        Item::Clutter(Kind::Installers) => "Installers in Downloads",
        Item::Clutter(Kind::BuildFolders) => "Project build folders",
        Item::Clutter(Kind::DockerImage) => "Docker disk image",
        Item::Clutter(Kind::SimulatorRuntimes) => "Simulators",
        Item::Clutter(Kind::TempFiles) => "Temporary files",
        Item::Clutter(Kind::SimulatorDownloads) => "Simulator downloads",
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
    /// macOS keeps it, and nobody can remove it
    Nobody,
}

fn who(item: Item) -> Who {
    match item {
        Item::Rules => Who::Neet,
        Item::Clutter(kind) if clutter::neet_removes(kind) => Who::Neet,
        Item::Clutter(Kind::DockerImage | Kind::SimulatorRuntimes) => Who::Tool,
        Item::Clutter(Kind::TempFiles) => Who::Mac,
        Item::Clutter(Kind::SimulatorDownloads) => Who::Nobody,
        Item::Clutter(_) => Who::You,
    }
}

/// How an item is cleared, short, in the color the screen uses for it
fn who_span(who: Who) -> Span<'static> {
    match who {
        Who::Neet => Span::raw("to Trash").green(),
        Who::Tool => Span::raw("deleted").red(),
        Who::You => Span::raw("by you").yellow(),
        Who::Mac => Span::raw("by macOS").fg(super::visual::ACCENT),
        Who::Nobody => Span::raw("kept").magenta(),
    }
}

/// What the item is
fn about(item: Item) -> &'static str {
    match item {
        Item::Rules => "App and tool caches and logs.",
        Item::Clutter(Kind::Trash) => "Files in the Trash still use space.",
        Item::Clutter(Kind::Installers) => "App installers in Downloads.",
        Item::Clutter(Kind::BuildFolders) => "node_modules and target folders in your projects.",
        Item::Clutter(Kind::DockerImage) => "Docker's disk. It never shrinks by itself.",
        Item::Clutter(Kind::SimulatorRuntimes) => "iPhone and iPad simulators from Xcode.",
        Item::Clutter(Kind::TempFiles) => "Temp files apps left behind.",
        Item::Clutter(Kind::SimulatorDownloads) => {
            "Downloads macOS kept after a runtime was removed."
        }
    }
}

/// Where the item's files are
fn place(item: Item) -> &'static str {
    match item {
        Item::Rules => "~/Library/Caches, ~/Library/Logs, tool caches",
        Item::Clutter(Kind::Trash) => "~/.Trash",
        Item::Clutter(Kind::Installers) => "~/Downloads",
        Item::Clutter(Kind::BuildFolders) => "Your code projects",
        Item::Clutter(Kind::DockerImage) => "~/Library/Containers/com.docker.docker",
        Item::Clutter(Kind::SimulatorRuntimes) => "/Library/Developer/CoreSimulator",
        Item::Clutter(Kind::TempFiles) => "/private/var/folders",
        Item::Clutter(Kind::SimulatorDownloads) => "/System/Library/AssetsV2",
    }
}

/// What changes once the item is cleared
fn afterwards(item: Item) -> &'static str {
    match item {
        Item::Rules => "Apps rebuild them. Some open slower at first.",
        Item::Clutter(Kind::Trash) => "Emptying is permanent.",
        Item::Clutter(Kind::Installers) => "Download one again if needed.",
        Item::Clutter(Kind::BuildFolders) => "The next build takes longer.",
        Item::Clutter(Kind::DockerImage) => "Prune deletes unused data. Reset deletes all.",
        Item::Clutter(Kind::SimulatorRuntimes) => "Xcode downloads them again if needed.",
        Item::Clutter(Kind::TempFiles) => "macOS clears them. A restart clears more.",
        Item::Clutter(Kind::SimulatorDownloads) => "macOS protects them. They can't be deleted.",
    }
}

/// What Enter does on the item
fn action(item: Item) -> &'static str {
    match item {
        Item::Rules => "Enter opens Deep Clean.",
        Item::Clutter(Kind::BuildFolders | Kind::Installers) => "Enter lists them, all picked.",
        Item::Clutter(Kind::Trash) => "Enter opens the Trash.",
        Item::Clutter(Kind::DockerImage) => "Enter opens Docker.",
        Item::Clutter(Kind::SimulatorRuntimes) => "Enter lists them.",
        Item::Clutter(Kind::TempFiles) => "Nothing to do.",
        Item::Clutter(Kind::SimulatorDownloads) => "Enter shows how to reuse one.",
    }
}

/// Whether clearing the item can be undone, in the color that says so
fn undo(item: Item) -> Span<'static> {
    match who(item) {
        Who::Neet => Span::raw("✓ from the Trash").green(),
        Who::Tool => Span::raw("✗ no").red(),
        // Only the Trash is left to you, and emptying it cannot be undone.
        Who::You => Span::raw("✗ no, once emptied").red(),
        Who::Mac => Span::raw("not needed").fg(super::visual::ACCENT),
        Who::Nobody => Span::raw("nothing removed").magenta(),
    }
}

/// Width of the labels in the boxes
const LABEL: usize = 11;

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{label:<LABEL$}")).bold()];
    spans.extend(value);
    Line::from(spans)
}

/// How tall a box is with `lines` in it, borders included
fn height_of(lines: &[Line<'_>], width: u16) -> u16 {
    super::visual::wrapped_rows(lines, width.saturating_sub(4)) + 2
}

/// `sections` one after another, a blank row apart
fn joined(sections: Vec<Vec<Line<'static>>>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, section) in sections.into_iter().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        lines.extend(section);
    }
    lines
}

/// A box with `title`, as the other boxes on the screen
fn titled(title: &str) -> ratatui::widgets::Block<'static> {
    super::visual::block()
        .title(format!(" {title} "))
        .padding(Padding::horizontal(1))
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
    downloads: Asked,
}

impl QuickClean {
    pub fn new() -> Self {
        let mut screen = Self::with(
            Asked::start(clutter::simulator_runtimes),
            Asked::start(|| clutter::temp_files().map(Some)),
        );
        screen.downloads = Asked::start(clutter::simulator_downloads);
        screen
    }

    fn with(simulators: Asked, temp: Asked) -> Self {
        Self {
            table: TableState::default().with_selected(Some(0)),
            from_tree: None,
            removable: None,
            simulators,
            temp,
            downloads: Asked::Done(None),
        }
    }

    fn poll(&mut self, tree: &Tree) {
        self.simulators.poll();
        self.temp.poll();
        self.downloads.poll();
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
                Asked::Waiting(_) | Asked::Failed => None,
            },
            Kind::TempFiles => match &self.temp {
                Asked::Done(found) => found.as_ref(),
                Asked::Waiting(_) | Asked::Failed => None,
            },
            Kind::SimulatorDownloads => match &self.downloads {
                Asked::Done(found) => found.as_ref(),
                Asked::Waiting(_) | Asked::Failed => None,
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
                Some(size) => Status::Found {
                    size,
                    count: context
                        .plan
                        .and_then(|estimate| estimate.planned.as_ref())
                        .map(|planned| {
                            planned.plan.rules.iter().map(|rule| rule.items.len()).sum()
                        }),
                },
                None if context.plan.is_some_and(Estimate::failed) => Status::Failed,
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
                if matches!(
                    (kind, &self.simulators, &self.temp, &self.downloads),
                    (Kind::SimulatorRuntimes, Asked::Failed, _, _)
                        | (Kind::TempFiles, _, Asked::Failed, _)
                        | (Kind::SimulatorDownloads, _, _, Asked::Failed)
                ) {
                    return Status::Failed;
                }
                let waiting = match kind {
                    Kind::SimulatorRuntimes => matches!(self.simulators, Asked::Waiting(_)),
                    Kind::TempFiles => matches!(self.temp, Asked::Waiting(_)),
                    Kind::SimulatorDownloads => matches!(self.downloads, Asked::Waiting(_)),
                    _ => self.from_tree.is_none(),
                };
                match self.finding(kind) {
                    Some(found) if found.size > 0 => Status::Found {
                        size: found.size,
                        count: Some(found.count),
                    },
                    _ if waiting => Status::Looking,
                    _ if kind == Kind::Trash && trash_blocked(context) => Status::NoAccess,
                    _ => Status::Nothing,
                }
            }
        }
    }

    /// What neet can clear, and what you clear yourself, from what was found
    fn totals(&self, context: &Context) -> (u64, u64) {
        let mut neet_total = 0;
        let mut your_total = 0;
        for item in ITEMS {
            if let Status::Found { size, .. } = self.status(item, context) {
                match who(item) {
                    Who::Neet | Who::Tool => neet_total += size,
                    Who::You => your_total += size,
                    Who::Mac | Who::Nobody => {}
                }
            }
        }
        (neet_total, your_total)
    }

    fn selected(&self) -> Item {
        ITEMS[self.table.selected().unwrap_or(0).min(ITEMS.len() - 1)]
    }

    /// The table, its rows `gap` blank rows apart, so a tall box is filled
    fn draw_table(&mut self, frame: &mut Frame, area: Rect, context: &Context, gap: u16) {
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
        let rows: Vec<Row> = ITEMS
            .iter()
            .zip(&statuses)
            .map(|(&item, status)| {
                let (size, bar, found) = match *status {
                    Status::Found { size, count } => {
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
                            format::size_bar(size, largest, usize::from(SHARE_BAR)),
                            found,
                        )
                    }
                    Status::Looking => (
                        Span::raw("scanning").fg(super::visual::ACCENT),
                        Span::raw(""),
                        String::new(),
                    ),
                    // An empty bar & zeros, so the row lines up with the rest
                    Status::Nothing => (
                        Span::raw("0 B"),
                        format::size_bar(0, largest, usize::from(SHARE_BAR)),
                        "0".to_string(),
                    ),
                    Status::Failed => (Span::raw("unknown").yellow(), Span::raw(""), String::new()),
                    Status::NoAccess => (
                        Span::raw("no access").yellow(),
                        Span::raw(""),
                        String::new(),
                    ),
                };
                columns
                    .row([
                        Cell::from(format::shorten_middle(name(item), columns.width(0))),
                        Cell::from(Line::from(size).right_aligned()),
                        Cell::from(bar),
                        Cell::from(Line::from(found).right_aligned()),
                        Cell::from(who_span(who(item))),
                    ])
                    .bottom_margin(gap)
            })
            .collect();
        let header = columns
            .row([
                Cell::from("Item"),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from(""),
                Cell::from(Line::from("Found").right_aligned()),
                Cell::from("How"),
            ])
            .style(super::visual::HEADING)
            .bottom_margin(u16::from(gap > 0));
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(titled("Quick Clean"))
            .highlight_symbol("▸ ")
            .row_highlight_style(super::visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// The selected item in headed sections: its numbers, what it is, what
    /// changes after, and what Enter does. Each section keeps the same
    /// number of lines on every row, so spreading them never moves them.
    fn details(&self, context: &Context, width: usize) -> Vec<Vec<Line<'static>>> {
        let item = self.selected();
        let status = self.status(item, context);
        let found: u64 = ITEMS
            .iter()
            .filter_map(|&item| match self.status(item, context) {
                Status::Found { size, .. } => Some(size),
                _ => None,
            })
            .sum();
        let size = match status {
            Status::Found { size, count } => {
                let mut value = vec![format::size_span(size, format::size(size)).bold()];
                if let Some(count) = count {
                    let noun = if count == 1 { "item" } else { "items" };
                    value.push(Span::raw(format!(
                        " · {} {noun}",
                        format::count(u64::try_from(count).unwrap_or(u64::MAX))
                    )));
                }
                value.push(Span::raw(format!(" · {}%", format::percent(size, found))));
                value
            }
            Status::Looking => vec![Span::raw("scanning").fg(super::visual::ACCENT)],
            Status::Nothing => vec![Span::raw("0 B")],
            Status::Failed => vec![Span::raw("unknown").yellow()],
            Status::NoAccess => vec![Span::raw("no access").yellow()],
        };
        let heading = |text: &str| Line::from(text.to_string()).style(super::visual::HEADING);
        let numbers = vec![
            heading("Details"),
            field(
                "Where",
                vec![
                    Span::raw(format::shorten_middle(
                        place(item),
                        width.saturating_sub(LABEL),
                    ))
                    .fg(super::visual::ACCENT),
                ],
            ),
            field("Size", size),
            field("Undo", vec![undo(item)]),
        ];
        let wrap = |text: &str, color: Option<Color>| {
            let style = color.map_or_else(ratatui::style::Style::default, |color| {
                ratatui::style::Style::default().fg(color)
            });
            super::visual::hanging(Vec::new(), 0, text, style, width)
        };
        // Missing sizes say why in place of what the item is
        let what = match status {
            Status::Failed => {
                let mut lines = wrap("Could not read the size.", Some(Color::Yellow));
                lines.extend(wrap(
                    if item == Item::Rules {
                        "Deep Clean shows why. r there tries again."
                    } else {
                        "Open Quick Clean again to retry."
                    },
                    None,
                ));
                lines
            }
            Status::NoAccess => wrap(NO_ACCESS, Some(Color::Yellow)),
            _ => wrap(about(item), None),
        };
        let after = wrap(afterwards(item), None);
        // Room for the longest text of any row, so the sections never move
        let room = |texts: &mut dyn Iterator<Item = &'static str>| {
            texts.map(|text| wrap(text, None).len()).max().unwrap_or(1)
        };
        let pad = |mut lines: Vec<Line<'static>>, room: usize| {
            while lines.len() < room {
                lines.push(Line::default());
            }
            lines
        };
        let what_room = room(&mut ITEMS.iter().map(|&item| about(item)));
        let after_room = room(&mut ITEMS.iter().map(|&item| afterwards(item)));
        let mut about_section = vec![heading("About")];
        about_section.extend(pad(what, what_room));
        let mut after_section = vec![heading("After")];
        after_section.extend(pad(after, after_room));
        vec![
            numbers,
            about_section,
            after_section,
            vec![heading("Next"), Line::from(action(item))],
        ]
    }

    /// What was found, by how it is cleared, and in all
    fn totals_lines(&self, context: &Context) -> Vec<Line<'static>> {
        let mut trash = 0;
        let mut deleted = 0;
        let mut yours = 0;
        for item in ITEMS {
            if let Status::Found { size, .. } = self.status(item, context) {
                match who(item) {
                    Who::Neet => trash += size,
                    Who::Tool => deleted += size,
                    Who::You => yours += size,
                    Who::Mac | Who::Nobody => {}
                }
            }
        }
        vec![
            field(
                "To Trash",
                vec![Span::raw(format!("~{}", format::size(trash))).green()],
            ),
            field("Deleted", vec![Span::raw(format::size(deleted)).red()]),
            field("By you", vec![Span::raw(format::size(yours)).yellow()]),
            field(
                "Total",
                vec![Span::raw(format!("~{}", format::size(trash + deleted + yours))).bold()],
            ),
        ]
    }

    /// Free space now and after the cleanup, with a gauge that shows the
    /// part the cleanup frees in green
    fn after_lines(&self, context: &Context, width: u16) -> Vec<Line<'static>> {
        let Some(disk) = context.disk else {
            return vec![Line::from("Disk space unavailable.").yellow()];
        };
        let (cleanup, _) = self.totals(context);
        let freed = cleanup.min(disk.used());
        let after = disk.available.saturating_add(freed);
        // The bar: what stays used, then what the cleanup frees in green,
        // then what is free already.
        let bar = usize::from(width).saturating_sub(18).clamp(10, 40);
        let cells = |part: u64| {
            usize::try_from(u128::from(part) * bar as u128 / u128::from(disk.total.max(1)))
                .unwrap_or(bar)
        };
        let stays = cells(disk.used() - freed);
        let green = cells(disk.used())
            .saturating_sub(stays)
            .max(usize::from(freed > 0));
        let rest = bar.saturating_sub(stays + green);
        let now = format::percent(disk.used(), disk.total);
        let then = format::percent(disk.used() - freed, disk.total);
        vec![
            Line::from(vec![
                Span::raw("█".repeat(stays)).fg(super::visual::ACCENT),
                Span::raw("█".repeat(green)).green(),
                Span::raw("░".repeat(rest)).fg(super::visual::ACCENT),
                Span::raw(format!(" {now}% → {then}% used")).bold(),
            ]),
            field(
                "Free now",
                vec![Span::raw(format!(
                    "{} of {}",
                    format::size(disk.available),
                    format::size(disk.total)
                ))],
            ),
            field(
                "Free after",
                vec![
                    Span::raw(format!("~{}", format::size(after)))
                        .green()
                        .bold(),
                ],
            ),
        ]
    }

    /// The largest items of the selected row, at most `rows` lines, or why
    /// there is no list
    fn largest_lines(
        &self,
        context: &Context,
        width: u16,
        rows: usize,
    ) -> Result<Vec<Line<'static>>, Line<'static>> {
        let item = self.selected();
        let width = usize::from(width.saturating_sub(4));
        let mut entries: Vec<(u64, String)> = match item {
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
                    } else if kind == Kind::Trash {
                        self.finding(kind)
                            .and_then(|found| found.node)
                            .map(|trash| {
                                scan.tree
                                    .get(trash)
                                    .children
                                    .iter()
                                    .copied()
                                    .filter(|&id| scan.tree.get(id).name != ".DS_Store")
                                    .collect()
                            })
                            .unwrap_or_default()
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
        if entries.is_empty() {
            return Err(Line::from(match item {
                Item::Clutter(
                    Kind::SimulatorRuntimes | Kind::TempFiles | Kind::SimulatorDownloads,
                ) => "Outside home, not scanned.",
                _ => "Nothing to list.",
            }));
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut lines: Vec<Line> = entries
            .iter()
            .take(rows.max(1))
            .map(|(size, path)| {
                Line::from(vec![
                    format::size_span(*size, format!("{:>9}  ", format::size(*size))),
                    Span::raw(format::shorten_path(path, width.saturating_sub(11))),
                ])
            })
            .collect();
        if entries.len() > lines.len() {
            lines.pop();
            lines.push(Line::from(format!(
                "{:>9}  & {} more",
                "",
                format::count(u64::try_from(entries.len() - lines.len()).unwrap_or(u64::MAX))
            )));
        }
        Ok(lines)
    }
}

/// How to let neet read the Trash
const NO_ACCESS: &str = "Turn on Full Disk Access for your terminal in System Settings, Privacy & Security, then restart it.";

/// Whether macOS refused to let the scan read `~/.Trash`
fn trash_blocked(context: &Context) -> bool {
    let ScanStatus::Done { scan, .. } = context.scan else {
        return false;
    };
    let trash = scan.tree.path(scan.tree.root()).join(".Trash");
    scan.errors
        .iter()
        .any(|error| error.permission_denied && error.path.as_deref() == Some(trash.as_path()))
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
    .max(9);
    let counts = format::column_width(
        "Found",
        statuses.iter().filter_map(|status| match status {
            Status::Found {
                count: Some(count), ..
            } => Some(format::count(u64::try_from(*count).unwrap_or(u64::MAX))),
            _ => None,
        }),
    );
    let names = format::column_width("Item", ITEMS.iter().map(|&item| name(item).to_string()));
    super::visual::Columns::new(
        width,
        [(names, 0), (sizes, 0), (SHARE_BAR, 0), (counts, 0), (8, 1)],
        &[2, 3],
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
        let items = u16::try_from(ITEMS.len()).unwrap_or(u16::MAX);
        if area.width < super::visual::SIDE_BY_SIDE {
            let [table, below] =
                Layout::vertical([Constraint::Length(items + 3), Constraint::Fill(1)]).areas(area);
            self.draw_table(frame, table, context, 0);
            if below.height > 2 {
                let width = usize::from(below.width.saturating_sub(4));
                frame.render_widget(
                    Paragraph::new(joined(self.details(context, width)))
                        .wrap(Wrap { trim: false })
                        .block(titled(name(self.selected()))),
                    below,
                );
            }
            return;
        }
        // Left: the table, the selected item under it, and the totals.
        // Right: the largest items and the disk after cleanup. The item's
        // box fits its sections, a blank row apart, and the table takes the
        // rest, spacing its rows to fill it. Every box keeps its size as the
        // selection moves, and the bottom boxes line up.
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)]).areas(area);
        let inner = right.width.saturating_sub(4);
        let totals = self.totals_lines(context);
        let after = self.after_lines(context, inner);
        let bottom = height_of(&totals, left.width).max(height_of(&after, right.width));
        let details = joined(self.details(context, usize::from(left.width.saturating_sub(4))));
        let details_height = height_of(&details, left.width);
        let with_bottom = left.height >= items + 3 + details_height + bottom;
        let with_details = left.height >= items + 3 + details_height;
        let [table, middle, under] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(if with_details { details_height } else { 0 }),
            Constraint::Length(if with_bottom { bottom } else { 0 }),
        ])
        .areas(left);
        // A blank row under each row when the box holds them
        let gap = u16::from(table.height >= items * 2 + 4);
        self.draw_table(frame, table, context, gap);
        if with_details {
            frame.render_widget(
                Paragraph::new(details)
                    .wrap(Wrap { trim: false })
                    .block(titled(name(self.selected()))),
                middle,
            );
        }
        if with_bottom {
            frame.render_widget(Paragraph::new(totals).block(titled("Found")), under);
        }
        let [list, under] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(if with_bottom { bottom } else { 0 }),
        ])
        .areas(right);
        // A header & rows spaced as the table's, so the two line up
        let largest_block = titled("Largest");
        let step = usize::from(gap) + 1;
        let room = usize::from(list.height.saturating_sub(2)).saturating_sub(step) / step;
        let mut lines =
            vec![Line::from(format!("{:>9}  {}", "Size", "Name")).style(super::visual::HEADING)];
        match self.largest_lines(context, list.width, room) {
            Ok(rows) => {
                for row in rows {
                    lines.extend(std::iter::repeat_n(Line::default(), usize::from(gap)));
                    lines.push(row);
                }
                frame.render_widget(Paragraph::new(lines).block(largest_block), list);
            }
            Err(note) => {
                let inner = largest_block.inner(list);
                frame.render_widget(largest_block, list);
                let [line] = Layout::vertical([Constraint::Length(1)])
                    .flex(ratatui::layout::Flex::Center)
                    .areas(inner);
                frame.render_widget(Paragraph::new(note).centered(), line);
            }
        }
        if with_bottom {
            frame.render_widget(Paragraph::new(after).block(titled("After cleanup")), under);
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
                    if matches!(kind, Kind::SimulatorRuntimes | Kind::SimulatorDownloads) {
                        return Action::Open(Box::new(Simulators::new()));
                    }
                    if kind == Kind::DockerImage {
                        return Action::Open(Box::new(DockerSpace::new()));
                    }
                    if kind == Kind::Trash {
                        let tree = &scan.tree;
                        let root = tree.root();
                        let trash = tree
                            .get(root)
                            .children
                            .iter()
                            .copied()
                            .find(|&child| tree.get(child).name == ".Trash");
                        let path = tree.path(root).join(".Trash");
                        let blocked = trash_blocked(context);
                        return Action::Open(Box::new(Trash::new(tree, trash, path, blocked)));
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
        "↑↓ move · enter open · d show in Disk · esc home · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("g  G", "Jump to the first or last row"),
            ("Enter  →  l", "Open the selected row"),
            ("d", "Show it in Disk"),
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

    #[test]
    fn a_trash_macos_will_not_let_neet_read_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        let home = fs::canonicalize(dir.path()).expect("home should resolve");
        let trash = home.join(".Trash");
        fs::create_dir_all(&trash).expect("folder should be made");
        fs::write(trash.join("old.zip"), vec![7u8; 2_000_000]).expect("file should be written");
        fs::set_permissions(&trash, fs::Permissions::from_mode(0o000)).expect("locked");
        let scanned = scan::scan(&home, |_| {});
        fs::set_permissions(&trash, fs::Permissions::from_mode(0o755)).expect("unlocked");
        let scan = ScanStatus::Done {
            scan: scanned.expect("scan should finish"),
            elapsed: std::time::Duration::from_secs(1),
        };

        // Unknown, rather than none
        let screen = render(&mut quick(), &scan);
        assert!(screen.contains("no access"), "{screen}");
        let (_dir, readable) = done();
        assert!(!render(&mut quick(), &readable).contains("no access"));
    }

    fn quick() -> QuickClean {
        let simulator = Finding {
            kind: Kind::SimulatorRuntimes,
            size: 17_000_000_000,
            count: 2,
            node: None,
        };
        QuickClean::with(Asked::Done(Some(simulator)), Asked::Done(None))
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
    fn missing_quick_clean_results_do_not_keep_scanning() {
        let (_dir, scan) = done();
        for kind in [Kind::SimulatorRuntimes, Kind::TempFiles] {
            let (sender, answer) = mpsc::channel();
            let mut screen = match kind {
                Kind::SimulatorRuntimes => {
                    QuickClean::with(Asked::Waiting(answer), Asked::Done(None))
                }
                _ => QuickClean::with(Asked::Done(None), Asked::Waiting(answer)),
            };
            screen
                .table
                .select(ITEMS.iter().position(|item| *item == Item::Clutter(kind)));
            let _ = render(&mut screen, &scan);
            assert!(matches!(
                screen.status(Item::Clutter(kind), &context(&scan)),
                Status::Looking
            ));
            drop(sender);
            let text = render(&mut screen, &scan);
            assert!(!text.contains("scanning"));
            assert!(text.contains("unknown"));
            assert!(text.contains("Quick Clean again"));
            assert!(matches!(screen.back(), Action::Back));
        }
    }

    #[test]
    fn queued_quick_clean_results_keep_sizes_and_errors() {
        let (_dir, scan) = done();
        let finding = Finding {
            kind: Kind::TempFiles,
            size: 2048,
            count: 1,
            node: None,
        };
        for result in [
            Ok(Some(finding)),
            Ok(None),
            Err(io::Error::other("read failed")),
        ] {
            let failed = result.is_err();
            let size = result
                .as_ref()
                .ok()
                .and_then(|found| found.as_ref())
                .map(|found| found.size);
            let (sender, answer) = mpsc::channel();
            sender.send(result).unwrap();
            drop(sender);
            let mut screen = QuickClean::with(Asked::Done(None), Asked::Waiting(answer));
            screen.temp.poll();
            screen.temp.poll();
            let status = screen.status(Item::Clutter(Kind::TempFiles), &context(&scan));
            if failed {
                assert!(matches!(status, Status::Failed));
            } else if let Some(expected) = size {
                assert!(matches!(status, Status::Found { size, .. } if size == expected));
            } else {
                assert!(matches!(status, Status::Nothing));
            }
        }
    }

    #[test]
    fn a_failed_cache_plan_shows_unknown_instead_of_scanning() {
        let (_dir, scan) = done();
        let estimate = super::super::clean::Estimate::start(None);
        let context = Context {
            cleanable: None,
            plan: Some(&estimate),
            ..context(&scan)
        };
        let mut screen = quick();
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        terminal
            .draw(|frame| screen.draw(frame, frame.area(), &context))
            .unwrap();
        let text = crate::ui::visual::tests::text(terminal.backend().buffer());
        assert!(text.contains("unknown"));
        assert!(!text.contains("scanning"));
        assert!(text.contains("Deep Clean shows why"));
        assert!(matches!(
            press(&mut screen, &scan, KeyCode::Enter),
            Action::Open(_)
        ));
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
        assert!(screen.contains("0 B"));
        assert!(screen.contains("by you"));
        assert!(screen.contains("deleted"));
        assert!(screen.contains("macOS"));
        assert!(screen.contains("Enter opens Deep Clean"));
    }

    #[test]
    fn build_folders_show_their_paths_and_open_the_picker() {
        let (_dir, scan) = done();
        let mut screen = quick();
        let _ = render(&mut screen, &scan);

        press(&mut screen, &scan, KeyCode::Down);
        let text = render(&mut screen, &scan);
        assert!(text.contains("~/code/web/node_modules"));
        assert!(text.contains("Enter lists them"));
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
    fn boxes_line_up_and_keep_their_size_as_the_selection_moves() {
        use crate::ui::visual::tests as view;
        let (_dir, scan) = done();
        let mut screen = quick();
        let mut context = context(&scan);
        context.disk = Some(neet_core::disk::DiskSpace {
            total: 500_000_000_000,
            available: 100_000_000_000,
            purgeable: None,
        });
        let mut layouts = Vec::new();
        for _ in 0..ITEMS.len() {
            let text = view::text(&view::render("quick", &mut screen, &context, 160, 50));
            let rows: Vec<String> = text.lines().map(str::to_string).collect();
            let row_of = |needle: &str| {
                rows.iter()
                    .position(|row| row.contains(needle))
                    .unwrap_or_else(|| panic!("{needle:?} missing:\n{text}"))
            };
            // The bottom boxes share a row, so their borders line up.
            assert_eq!(row_of("┌ Found"), row_of("┌ After cleanup"), "{text}");
            assert!(rows[0].contains("┌ Quick Clean"));
            assert!(rows[0].contains("┌ Largest"));
            layouts.push((row_of("┌ Largest"), row_of("┌ Found")));
            screen.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &context);
        }
        // Every row draws the boxes in the same places.
        assert!(
            layouts.windows(2).all(|pair| pair[0] == pair[1]),
            "{layouts:?}"
        );
    }

    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let (_dir, scan) = done();
        let mut screen = quick();
        for (width, height) in view::SIZES {
            let buffer = view::render("quick", &mut screen, &context(&scan), width, height);
            view::aligned(&buffer, "Size", "~8.6 GB");
            assert!(view::text(&buffer).contains("deleted"));
            assert!(view::text(&buffer).contains("macOS"));
        }
    }
}
