use std::cmp::Reverse;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
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
use super::clean::{Clean, display_path};
use super::disk::Disk;
use super::format;
use super::loading::scanning;
use super::pick::Pick;
use super::scan::ScanStatus;
use super::tools::{DockerSpace, Simulators, Trash};

/// Below this width the largest items are left out.
const MIN_SIDE_WIDTH: u16 = 130;

/// Width of the table and the box under it, when the largest items are beside them
const TABLE_WIDTH: u16 = 86;

/// From this height the table's rows stand a row apart.
const TALL: u16 = 45;

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
    Found {
        size: u64,
        count: Option<usize>,
    },
    Looking,
    Nothing,
    /// macOS would not let the scan read it, so its size is unknown
    NoAccess,
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
        Who::Mac => Span::raw("macOS").fg(super::visual::ACCENT),
    }
}

/// What the item is
fn about(item: Item) -> &'static str {
    match item {
        Item::Rules => "Saved files & logs from apps & tools.",
        Item::Clutter(Kind::Trash) => "Files in the Trash still use space until you empty it.",
        Item::Clutter(Kind::Installers) => {
            "App installers in Downloads. You rarely need them after installing."
        }
        Item::Clutter(Kind::BuildFolders) => {
            "Build files in your code projects. They come back when you build again."
        }
        Item::Clutter(Kind::DockerImage) => {
            "The file Docker keeps all its data in. It does not shrink by itself."
        }
        Item::Clutter(Kind::SimulatorRuntimes) => "iPhone & iPad simulators that Xcode downloaded.",
        Item::Clutter(Kind::TempFiles) => "Temporary files apps left behind.",
    }
}

/// Where the item's files are
fn place(item: Item) -> &'static str {
    match item {
        Item::Rules => "~/Library/Caches, ~/Library/Logs & tool caches like npm & pip.",
        Item::Clutter(Kind::Trash) => "~/.Trash",
        Item::Clutter(Kind::Installers) => "~/Downloads",
        Item::Clutter(Kind::BuildFolders) => "node_modules & target folders in your projects.",
        Item::Clutter(Kind::DockerImage) => "~/Library/Containers/com.docker.docker",
        Item::Clutter(Kind::SimulatorRuntimes) => "/Library/Developer/CoreSimulator",
        Item::Clutter(Kind::TempFiles) => "/private/var/folders",
    }
}

/// What changes once the item is cleared
fn afterwards(item: Item) -> &'static str {
    match item {
        Item::Rules => "Apps make them again. Some may open a bit slower at first.",
        Item::Clutter(Kind::Trash) => "The files are gone for good.",
        Item::Clutter(Kind::Installers) => "Download it again if you ever need it.",
        Item::Clutter(Kind::BuildFolders) => "Your next build takes a bit longer.",
        Item::Clutter(Kind::DockerImage) => {
            "Unused Docker data is deleted. A reset deletes all of it."
        }
        Item::Clutter(Kind::SimulatorRuntimes) => "Xcode downloads it again if you need it.",
        Item::Clutter(Kind::TempFiles) => "Nothing to do. The space comes back by itself.",
    }
}

/// Whether clearing the item can be undone, in the color that says so
fn undo(item: Item) -> Span<'static> {
    match who(item) {
        Who::Neet => Span::raw("Yes, from the Trash").green(),
        Who::Tool => Span::raw("No, deleted for good").red(),
        // Only the Trash is left to you, and emptying it cannot be undone.
        Who::You => Span::raw("No, once emptied").red(),
        Who::Mac => Span::raw("Not needed").fg(super::visual::ACCENT),
    }
}

/// The steps that free an item
fn steps_of(item: Item) -> &'static [&'static str] {
    match item {
        Item::Rules => &[
            "Press Enter to open Deep Clean.",
            "Press Space to pick what to clean.",
            "Check the list, then confirm.",
        ],
        Item::Clutter(Kind::BuildFolders) => &[
            "Press Enter to see the folders.",
            "All are picked. Unpick projects you use.",
            "Check the list, then confirm.",
        ],
        Item::Clutter(Kind::Installers) => &[
            "Press Enter to see the installers.",
            "All are picked. Unpick ones to keep.",
            "Check the list, then confirm.",
        ],
        Item::Clutter(Kind::Trash) => &[
            "Press Enter to see what is in the Trash.",
            "Press e to empty it, then y to confirm.",
            "Emptied files are gone for good.",
        ],
        Item::Clutter(Kind::DockerImage) => &[
            "Press Enter to see what Docker uses.",
            "Confirm to delete unused data.",
            "Press x to reset Docker fully.",
        ],
        Item::Clutter(Kind::SimulatorRuntimes) => &[
            "Press Enter to see the simulators.",
            "Press Space to pick ones you don't need.",
            "Confirm. They are deleted for good.",
        ],
        Item::Clutter(Kind::TempFiles) => &[
            "macOS removes old ones by itself.",
            "Restarting your Mac clears more.",
            "Don't delete them yourself.",
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
                Some(size) => Status::Found {
                    size,
                    count: context
                        .plan
                        .and_then(|estimate| estimate.planned.as_ref())
                        .map(|planned| {
                            planned.plan.rules.iter().map(|rule| rule.items.len()).sum()
                        }),
                },
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
                    Who::Mac => {}
                }
            }
        }
        (neet_total, your_total)
    }

    fn selected(&self) -> Item {
        ITEMS[self.table.selected().unwrap_or(0).min(ITEMS.len() - 1)]
    }

    /// The table. With `spaced`, a blank row sits under each row, for a tall
    /// screen.
    fn draw_table(&mut self, frame: &mut Frame, area: Rect, context: &Context, spaced: bool) {
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
        let (neet_total, your_total) = self.totals(context);
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
                            format::size_bar(size, largest, SHARE_BAR),
                            found,
                        )
                    }
                    Status::Looking => (
                        Span::raw("scanning").fg(super::visual::ACCENT),
                        Span::raw(""),
                        String::new(),
                    ),
                    Status::Nothing => (Span::raw("none"), Span::raw(""), String::new()),
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
                    .bottom_margin(u16::from(spaced))
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

    /// The selected row's name, then its sections: the numbered steps that
    /// clear it, what it is, where it is, and what happens after
    fn about_lines(&self) -> (String, Vec<Vec<Line<'static>>>) {
        let item = self.selected();
        let color = match who(item) {
            Who::Neet => Color::Green,
            Who::Tool => Color::Red,
            Who::You => Color::Yellow,
            Who::Mac => super::visual::ACCENT,
        };
        let mut steps = Vec::new();
        steps.extend(steps_of(item).iter().enumerate().map(|(number, step)| {
            Line::from(vec![
                Span::raw(format!("{}. ", number + 1)).fg(color),
                Span::raw(*step),
            ])
        }));
        let about = vec![
            Line::from("About").style(super::visual::HEADING),
            Line::from(about(item)),
        ];
        let place = vec![
            Line::from("Location").style(super::visual::HEADING),
            Line::from(place(item)).fg(super::visual::ACCENT),
        ];
        let after = vec![
            Line::from("Impact").style(super::visual::HEADING),
            Line::from(afterwards(item)),
            Line::from(vec![
                Span::raw(format!("{:<11}", "Undo")).bold(),
                undo(item),
            ]),
        ];
        (
            format!(" {} ", name(item)),
            vec![steps, about, place, after],
        )
    }

    /// Every row that found something, largest first, with a bar in the
    /// color of who clears it
    fn found_lines(&self, context: &Context, width: u16) -> Vec<Line<'static>> {
        let mut found: Vec<(Item, u64)> = ITEMS
            .iter()
            .filter_map(|&item| match self.status(item, context) {
                Status::Found { size, .. } => Some((item, size)),
                _ => None,
            })
            .collect();
        found.sort_by_key(|&(_, size)| Reverse(size));
        let largest = found.first().map_or(0, |&(_, size)| size);
        let bar = usize::from(width).saturating_sub(36).clamp(8, 40);
        let mut lines = vec![Line::from("Overview").style(super::visual::HEADING)];
        if found.is_empty() {
            lines.push(Line::from("Nothing yet."));
        }
        for (item, size) in found {
            let style = who_span(who(item)).style;
            lines.push(Line::from(vec![
                Span::raw(format!("{:<24}", name(item))),
                format::size_span(size, format!("{:>9}  ", format::size(size))),
                Span::styled(format::bar(size, largest, bar), style),
            ]));
        }
        lines
    }

    /// The selected row in numbers: its size, its share of everything found,
    /// and who clears it
    fn row_lines(&self, context: &Context) -> Vec<Line<'static>> {
        let item = self.selected();
        let found: u64 = ITEMS
            .iter()
            .filter_map(|&item| match self.status(item, context) {
                Status::Found { size, .. } => Some(size),
                _ => None,
            })
            .sum();
        let field = |label: &str, value: Vec<Span<'static>>| {
            let mut spans = vec![Span::raw(format!("{label:<11}")).bold()];
            spans.extend(value);
            Line::from(spans)
        };
        let mut lines = vec![Line::from("Summary").style(super::visual::HEADING)];
        match self.status(item, context) {
            Status::Found { size, count } => {
                let mut value = vec![format::size_span(size, format::size(size)).bold()];
                if let Some(count) = count {
                    let noun = if count == 1 { "item" } else { "items" };
                    value.push(Span::raw(format!(
                        " · {} {noun}",
                        format::count(u64::try_from(count).unwrap_or(u64::MAX))
                    )));
                }
                lines.push(field("Size", value));
                lines.push(field(
                    "Share",
                    vec![
                        format::size_bar(size, found, SHARE_BAR),
                        Span::raw(format!(" {}% of all found", format::percent(size, found))),
                    ],
                ));
            }
            Status::Looking => lines.push(field(
                "Size",
                vec![Span::raw("scanning").fg(super::visual::ACCENT)],
            )),
            Status::Nothing => lines.push(field("Size", vec![Span::raw("none found")])),
            Status::NoAccess => {
                lines.push(field("Size", vec![Span::raw("no access").yellow()]));
                lines.push(Line::from(NO_ACCESS).yellow());
            }
        }
        let how = match who(item) {
            Who::Neet => "neet, to the Trash",
            Who::Tool => "neet, deleted for good",
            Who::You => "you",
            Who::Mac => "macOS",
        };
        // In the color the table uses for who clears it
        let style = who_span(who(item)).style;
        lines.push(field("Cleared by", vec![Span::styled(how, style)]));
        lines
    }

    /// Free space now, what the cleanup frees, and free space after it
    fn after_lines(&self, context: &Context, width: u16) -> Vec<Line<'static>> {
        let Some(disk) = context.disk else {
            return vec![Line::from("Disk space unavailable.").yellow()];
        };
        let (cleanup, manual) = self.totals(context);
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
        let field = |label: &str, value: Vec<Span<'static>>| {
            let mut spans = vec![Span::raw(format!("{label:<11}")).bold()];
            spans.extend(value);
            Line::from(spans)
        };
        let mut lines = vec![
            Line::from("After cleanup").style(super::visual::HEADING),
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
                "Cleanup",
                vec![
                    Span::raw(format!("~{}", format::size(cleanup)))
                        .green()
                        .bold(),
                    Span::raw(" by neet"),
                ],
            ),
        ];
        if manual > 0 {
            lines.push(field(
                "By hand",
                vec![
                    Span::raw(format::size(manual)).yellow().bold(),
                    Span::raw(" by you"),
                ],
            ));
        }
        lines.push(field(
            "Free after",
            vec![
                Span::raw(format!("~{}", format::size(after)))
                    .green()
                    .bold(),
            ],
        ));
        lines
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
                Item::Clutter(Kind::SimulatorRuntimes | Kind::TempFiles) => {
                    "Outside home. No scan details."
                }
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
const NO_ACCESS: &str = "macOS blocks your terminal from reading the Trash. Turn on Full Disk Access for your terminal app in System Settings, Privacy & Security, then restart it.";

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
        let items = u16::try_from(ITEMS.len()).unwrap_or(u16::MAX);
        // On a tall screen the table's rows stand a row apart.
        let spaced = area.height >= TALL;
        let table_height = if spaced { items * 2 + 3 } else { items + 4 };
        // The table, with how to clear the selected row under it, and the
        // row's largest items down the right. Each box spreads its sections
        // from top to bottom, so a tall screen has no empty box.
        let (title, mut sections) = self.about_lines();
        sections.push(self.row_lines(context));
        if area.width >= MIN_SIDE_WIDTH {
            let [left, side] =
                Layout::horizontal([Constraint::Length(TABLE_WIDTH), Constraint::Fill(1)])
                    .areas(area);
            let [table, below] =
                Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)])
                    .areas(left);
            self.draw_table(frame, table, context, spaced);
            super::visual::sections(frame, below, &title, sections);
            let inner_width = side.width.saturating_sub(4);
            let after = self.after_lines(context, inner_width);
            let found = self.found_lines(context, inner_width);
            let used = after.len() + found.len() + 2;
            // As many as fit with the other two boxes below, each with its
            // border, and their headings as titles
            let rows = usize::from(side.height)
                .saturating_sub(used + 2)
                .clamp(1, 12);
            let mut list = vec![Line::from("Largest items").style(super::visual::HEADING)];
            list.extend(
                self.largest_lines(context, side.width, rows)
                    .unwrap_or_else(|note| vec![note]),
            );
            super::visual::sections(frame, side, "", vec![list, found, after]);
        } else {
            let [table, below] =
                Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)])
                    .areas(area);
            self.draw_table(frame, table, context, spaced);
            // A short screen keeps what matters most: the steps, what it
            // is, its numbers, and the disk after the cleanup.
            let numbers = sections.pop().unwrap_or_default();
            let essentials = sections.drain(..2).chain([numbers]);
            let mut shown: Vec<Vec<Line<'static>>> = essentials.collect();
            shown.push(self.after_lines(context, below.width.saturating_sub(4)));
            super::visual::sections(frame, below, &title, shown);
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
        assert!(screen.contains("Press Enter to open Deep Clean"));
    }

    #[test]
    fn build_folders_show_their_paths_and_open_the_picker() {
        let (_dir, scan) = done();
        let mut screen = quick();
        let _ = render(&mut screen, &scan);

        press(&mut screen, &scan, KeyCode::Down);
        let text = render(&mut screen, &scan);
        assert!(text.contains("~/code/web/node_modules"));
        assert!(text.contains("Press Enter to see the folders"));
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
    fn tall_boxes_spread_their_sections_from_top_to_bottom() {
        use crate::ui::visual::tests as view;
        let (_dir, scan) = done();
        let mut screen = quick();
        let mut context = context(&scan);
        context.disk = Some(neet_core::disk::DiskSpace {
            total: 500_000_000_000,
            available: 100_000_000_000,
            purgeable: None,
        });
        let buffer = view::render("quick", &mut screen, &context, 160, 50);
        let text = view::text(&buffer);
        let rows: Vec<&str> = text.lines().collect();
        let row_of = |needle: &str| {
            rows.iter()
                .position(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} missing:\n{text}"))
        };
        // The boxes still run the full height, as before.
        assert!(rows[0].contains("┌ Quick Clean") && rows[0].contains("┌ Largest items"));
        let bottom = rows.iter().rposition(|row| row.contains('└')).unwrap();
        assert!(bottom >= 46, "the boxes end at row {bottom}");
        // Each section is a box of its own, titled with its heading, and
        // the boxes share the height.
        assert!(row_of("┌ Largest items") < row_of("┌ Overview"));
        assert!(row_of("┌ Overview") < row_of("┌ After cleanup"));
        assert!(row_of("Free after ~125.6 GB") < bottom - 3);
        // The table's rows stand apart on a tall screen.
        assert_eq!(
            rows[row_of("Caches & logs") + 1].trim_matches(['│', ' ']),
            ""
        );
        // Under the table: the steps, what it is, where, after, and the
        // numbers, each in its own box, in order.
        let order = [
            "┌ Caches & logs",
            "┌ About",
            "┌ Location",
            "┌ Impact",
            "┌ Summary",
        ];
        for pair in order.windows(2) {
            assert!(row_of(pair[0]) < row_of(pair[1]), "{pair:?}:\n{text}");
        }
        assert!(text.contains("Undo       Yes, from the Trash"));
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
