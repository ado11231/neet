use std::cmp::Reverse;

use std::time::SystemTime;

use neet_core::safety::{CleanupRoots, SafetyError};
use neet_core::tree::{NodeId, NodeKind, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Paragraph, Row, Table, TableState};

use super::app::{Action, Context, Screen};
use super::format;
use super::loading::scanning;
use super::scan::ScanStatus;

/// How many characters wide each size bar is, as on the other screens
const BAR_WIDTH: usize = 10;

/// Width of the labels on the right, as on the other screens
const LABEL: usize = 10;

/// Below this width the preview column is hidden.
const MIN_PREVIEW_WIDTH: u16 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sort {
    Size,
    Name,
    Items,
}

impl Sort {
    fn next(self) -> Self {
        match self {
            Self::Size => Self::Name,
            Self::Name => Self::Items,
            Self::Items => Self::Size,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Name => "name",
            Self::Items => "items",
        }
    }

    /// The sort a preference names, or by size
    fn from_label(label: &str) -> Self {
        match label {
            "name" => Self::Name,
            "items" => Self::Items,
            _ => Self::Size,
        }
    }
}

/// Where the user is in the scan tree.
struct Browser {
    current: NodeId,
    rows: Vec<NodeId>,
    list: TableState,
    sort: Sort,
    /// For saying whether the selected item can be cleaned
    roots: Option<CleanupRoots>,
}

impl Browser {
    #[cfg(test)]
    fn new(tree: &Tree) -> Self {
        Self::sorted(tree, Sort::Size)
    }

    fn sorted(tree: &Tree, sort: Sort) -> Self {
        let mut browser = Self {
            current: tree.root(),
            rows: Vec::new(),
            list: TableState::default(),
            sort,
            roots: CleanupRoots::new(&tree.path(tree.root())).ok(),
        };
        browser.show(tree, tree.root(), None);
        browser
    }

    /// Lists `folder`, selecting `select` if given, or the first row.
    fn show(&mut self, tree: &Tree, folder: NodeId, select: Option<NodeId>) {
        self.current = folder;
        self.rows = sorted_children(tree, folder, self.sort);
        let index = select
            .and_then(|id| self.rows.iter().position(|&row| row == id))
            .or(if self.rows.is_empty() { None } else { Some(0) });
        self.list.select(index);
    }

    fn selected(&self) -> Option<NodeId> {
        self.list
            .selected()
            .and_then(|index| self.rows.get(index).copied())
    }

    fn move_to(&mut self, index: usize) {
        if !self.rows.is_empty() {
            self.list.select(Some(index.min(self.rows.len() - 1)));
        }
    }

    fn up(&mut self) {
        let index = self.list.selected().unwrap_or(0);
        self.move_to(index.saturating_sub(1));
    }

    fn down(&mut self) {
        let index = self.list.selected().unwrap_or(0);
        self.move_to(index + 1);
    }

    /// Opens the selected folder, if it is one.
    fn enter(&mut self, tree: &Tree) {
        if let Some(id) = self.selected()
            && tree.get(id).kind == NodeKind::Directory
        {
            self.show(tree, id, None);
        }
    }

    /// Goes up to the parent folder, keeping the folder just left selected.
    fn leave(&mut self, tree: &Tree) {
        if let Some(parent) = tree.get(self.current).parent {
            self.show(tree, parent, Some(self.current));
        }
    }

    fn cycle_sort(&mut self, tree: &Tree) {
        self.sort = self.sort.next();
        let selected = self.selected();
        self.show(tree, self.current, selected);
    }
}

fn sorted_children(tree: &Tree, folder: NodeId, sort: Sort) -> Vec<NodeId> {
    let mut rows = tree.get(folder).children.clone();
    let name = |id: &NodeId| tree.get(*id).name.to_string_lossy().to_lowercase();
    match sort {
        Sort::Size => rows.sort_by_cached_key(|id| (Reverse(tree.get(*id).total_size), name(id))),
        Sort::Name => rows.sort_by_cached_key(name),
        Sort::Items => {
            rows.sort_by_cached_key(|id| (Reverse(tree.get(*id).total_items), name(id)));
        }
    }
    rows
}

fn display_name(tree: &Tree, id: NodeId) -> String {
    let node = tree.get(id);
    let name = node.name.to_string_lossy();
    match node.kind {
        NodeKind::Directory => format!("{name}/"),
        NodeKind::Symlink => format!("{name}@"),
        NodeKind::File | NodeKind::Other => name.into_owned(),
    }
}

/// The folder's path, with the scan root shown as `~`.
pub(super) fn display_path(tree: &Tree, id: NodeId) -> String {
    let root = tree.path(tree.root());
    let path = tree.path(id);
    match path.strip_prefix(&root) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn columns(width: u16, tree: &Tree, rows: &[NodeId], selected: bool) -> super::visual::Columns<4> {
    let sizes = format::column_width(
        "Size",
        rows.iter().map(|&id| format::size(tree.get(id).total_size)),
    )
    .max(9);
    super::visual::Columns::new(
        width,
        [(sizes, 0), (10, 0), (4, 0), (16, 1)],
        &[1],
        selected,
    )
}

fn header(columns: &super::visual::Columns<4>) -> Row<'static> {
    columns
        .row([
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from(""),
            Cell::from(Line::from("%").right_aligned()),
            Cell::from("Name"),
        ])
        .style(super::visual::HEADING)
}

fn row(
    tree: &Tree,
    id: NodeId,
    parent_total: u64,
    columns: &super::visual::Columns<4>,
) -> Row<'static> {
    let node = tree.get(id);
    let name = Span::raw(format::shorten_middle(
        &display_name(tree, id),
        columns.width(3),
    ));
    let name = if node.kind == NodeKind::Directory {
        name.fg(super::visual::ACCENT).bold()
    } else {
        name
    };
    columns.row([
        Cell::from(
            Line::from(format::size_span(
                node.total_size,
                format::size(node.total_size),
            ))
            .right_aligned(),
        ),
        Cell::from(format::size_bar(node.total_size, parent_total, BAR_WIDTH)),
        Cell::from(Line::from(share(node.total_size, parent_total)).right_aligned()),
        Cell::from(name),
    ])
}

/// The folder's rows, with its size & item count on the bottom border when
/// `summary` is set, that is when no box below says them
fn draw_folder(frame: &mut Frame, area: Rect, tree: &Tree, browser: &mut Browser, summary: bool) {
    let folder = tree.get(browser.current);
    let sort = format!(" By {} ", browser.sort.label());
    let title = format!(
        " Disk · {} ",
        format::shorten_path(
            &display_path(tree, browser.current),
            usize::from(area.width)
                .saturating_sub(format::display_width(&sort) + 13)
                .max(10),
        )
    );
    let mut block = super::visual::block()
        .title(title)
        .title(Line::from(sort).right_aligned())
        .padding(Padding::horizontal(1));
    if summary {
        block = block.title_bottom(Line::from(format!(
            " {} · {} ",
            items(folder.total_items),
            format::size(folder.total_size)
        )));
    }

    if browser.rows.is_empty() {
        frame.render_widget(Paragraph::new("Empty folder.").block(block), area);
        return;
    }
    let columns = columns(area.width, tree, &browser.rows, true);
    let items = browser
        .rows
        .iter()
        .map(|&id| row(tree, id, folder.total_size, &columns));
    let table = Table::new(items, columns.widths())
        .header(header(&columns))
        .column_spacing(2)
        .block(block)
        .highlight_symbol("▸ ")
        .row_highlight_style(super::visual::SELECTED);
    frame.render_stateful_widget(table, area, &mut browser.list);
}

/// The selected item on the right: its details, what is inside it, and the
/// disk. The details & disk boxes keep one height, and Inside takes the
/// rest, so moving between rows never resizes the boxes.
fn draw_preview(
    frame: &mut Frame,
    area: Rect,
    tree: &Tree,
    browser: &Browser,
    disk: Option<&neet_core::disk::DiskSpace>,
    bottom: u16,
) {
    let Some(id) = browser.selected() else {
        frame.render_widget(super::visual::block(), area);
        return;
    };
    let width = usize::from(area.width.saturating_sub(4));
    let details = about(tree, id, browser, width);
    let mac = this_mac(disk, tree.get(tree.root()).total_size, width);
    let details_height = height_of(&details, area.width);
    let mac_height = bottom;
    let name = format::shorten_middle(&display_name(tree, id), width.saturating_sub(2));
    let details_block = super::visual::block()
        .title(format!(" {name} "))
        .padding(Padding::horizontal(1));

    // Leave out This Mac, then Inside, when the screen is too short
    let with_mac = area.height >= details_height + mac_height + 4;
    let with_inside = area.height >= details_height + 4;
    if !with_inside {
        frame.render_widget(
            Paragraph::new(details)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .block(details_block),
            area,
        );
        return;
    }
    let [top, middle, bottom] = Layout::vertical([
        Constraint::Length(details_height),
        Constraint::Fill(1),
        Constraint::Length(if with_mac { mac_height } else { 0 }),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(details)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(details_block),
        top,
    );
    let inside_block = super::visual::block()
        .title(" Inside ")
        .padding(Padding::horizontal(1));
    let rows = usize::from(middle.height.saturating_sub(2));
    let inside = if tree.get(id).kind == NodeKind::Directory {
        inside(tree, id, browser.sort, width, rows)
    } else {
        Vec::new()
    };
    if inside.is_empty() {
        let note = if tree.get(id).kind == NodeKind::Directory {
            "Empty folder."
        } else {
            "Not a folder."
        };
        let inner = inside_block.inner(middle);
        frame.render_widget(inside_block, middle);
        let [line] = Layout::vertical([Constraint::Length(1)])
            .flex(ratatui::layout::Flex::Center)
            .areas(inner);
        frame.render_widget(Paragraph::new(note).centered(), line);
    } else {
        frame.render_widget(Paragraph::new(inside).block(inside_block), middle);
    }
    if with_mac {
        frame.render_widget(
            Paragraph::new(mac).block(
                super::visual::block()
                    .title(" This Mac ")
                    .padding(Padding::horizontal(1)),
            ),
            bottom,
        );
    }
}

/// How tall a box is with `lines` in it, borders included
fn height_of(lines: &[Line<'_>], width: u16) -> u16 {
    super::visual::wrapped_rows(lines, width.saturating_sub(4)) + 2
}

/// What the selected folder holds, in the current sort, with a bar against
/// the folder, as many as fit in `room` rows
fn inside(tree: &Tree, id: NodeId, sort: Sort, width: usize, room: usize) -> Vec<Line<'static>> {
    let node = tree.get(id);
    let rows = sorted_children(tree, id, sort);
    let shown = if rows.len() > room {
        room.saturating_sub(1)
    } else {
        rows.len()
    };
    let name_room = width.saturating_sub(13 + BAR_WIDTH);
    let mut lines = Vec::new();
    for &child in rows.iter().take(shown) {
        let size = tree.get(child).total_size;
        let name = Span::raw(format::shorten_middle(
            &display_name(tree, child),
            name_room,
        ));
        lines.push(Line::from(vec![
            format::size_span(size, format!("{:>9}  ", format::size(size))),
            format::size_bar(size, node.total_size, BAR_WIDTH),
            Span::raw("  "),
            if tree.get(child).kind == NodeKind::Directory {
                name.fg(super::visual::ACCENT)
            } else {
                name
            },
        ]));
    }
    if rows.len() > shown && shown > 0 {
        lines.push(Line::from(format!(
            "{:>9}  & {} more",
            "",
            format::count(u64::try_from(rows.len() - shown).unwrap_or(u64::MAX))
        )));
    }
    lines
}

/// The folder being listed: its size, how many items it holds, and its
/// largest item
fn this_folder(tree: &Tree, browser: &Browser, width: usize) -> Vec<Line<'static>> {
    let folder = tree.get(browser.current);
    let mut lines = vec![
        field(
            "Size",
            format::size_span(folder.total_size, format::size(folder.total_size)),
        ),
        field("Items", Span::raw(format::count(folder.total_items))),
    ];
    let largest = folder
        .children
        .iter()
        .copied()
        .max_by_key(|&child| tree.get(child).total_size);
    if let Some(largest) = largest {
        let share = format!(
            " · {}",
            share(tree.get(largest).total_size, folder.total_size)
        );
        let name = format::shorten_middle(
            &display_name(tree, largest),
            width.saturating_sub(LABEL + share.len()),
        );
        lines.push(field("Largest", Span::raw(format!("{name}{share}"))));
    }
    lines.extend(super::visual::hanging(
        Vec::new(),
        0,
        "View only. Clean in Deep Clean, Remove App, or Large Files.",
        ratatui::style::Style::default(),
        width,
    ));
    lines
}

/// `part` as a percent of `total`, with `<1%` for a part too small to round
/// up to one
fn share(part: u64, total: u64) -> String {
    match format::percent(part, total) {
        0 if part > 0 => "<1%".to_string(),
        percent => format!("{percent}%"),
    }
}

/// The whole disk: a gauge of what is used, how much is used & free, and
/// how much of it the home folder takes
fn this_mac(
    disk: Option<&neet_core::disk::DiskSpace>,
    home: u64,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let Some(disk) = disk else {
        lines.push(Line::from("Disk space unavailable.").yellow());
        return lines;
    };
    let used = format::percent(disk.used(), disk.total);
    let bar = width.saturating_sub(12).clamp(10, 40);
    let filled =
        usize::try_from(u128::from(disk.used()) * bar as u128 / u128::from(disk.total.max(1)))
            .unwrap_or(bar)
            .min(bar);
    lines.extend([
        Line::from(vec![
            Span::raw("█".repeat(filled)).fg(super::visual::ACCENT),
            Span::raw("░".repeat(bar - filled)).fg(super::visual::ACCENT),
            Span::raw(format!(" {used}% used")).bold(),
        ]),
        field(
            "Used",
            Span::raw(format!(
                "{} of {}",
                format::size(disk.used()),
                format::size(disk.total)
            )),
        ),
        field("Free", Span::raw(format::size(disk.available)).green()),
        field(
            "Home",
            Span::raw(format!(
                "{} · {} of the disk",
                format::size(home),
                share(home, disk.total)
            )),
        ),
    ]);
    lines
}

/// `1 item` or `27 items`
fn items(value: u64) -> String {
    format!(
        "{} {}",
        format::count(value),
        if value == 1 { "item" } else { "items" }
    )
}

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<LABEL$}")).bold(), value])
}

/// What the selected item is, how big, and whether it can be cleaned, with
/// values fitted to `width`. Always the same rows, and room for a two line
/// note, so the box keeps its height from row to row.
fn about(tree: &Tree, id: NodeId, browser: &Browser, width: usize) -> Vec<Line<'static>> {
    let node = tree.get(id);
    let parent_total = tree.get(browser.current).total_size;
    let mut lines = vec![
        field(
            "Path",
            Span::raw(format::shorten_path(
                &display_path(tree, id),
                width.saturating_sub(LABEL),
            ))
            .fg(super::visual::ACCENT),
        ),
        Line::from(vec![
            Span::raw(format!("{:<LABEL$}", "Size")).bold(),
            format::size_span(node.total_size, format::size(node.total_size)),
            Span::raw(format!(" · {}", share(node.total_size, parent_total))),
        ]),
    ];
    let mut hint = meaning(tree, id);
    match node.kind {
        NodeKind::Directory => {
            lines.push(field("Items", Span::raw(format::count(node.total_items))));
        }
        NodeKind::Symlink => lines.push(field("Type", Span::raw("link, not followed"))),
        NodeKind::File | NodeKind::Other => {
            let name = node.name.to_string_lossy();
            let (kind, about) = super::large::kind_of(&name);
            // Quick Clean can prune & reset Docker, so say where to go
            let about = if name == "Docker.raw" {
                Some("Docker's disk. Clear it in Quick Clean.")
            } else {
                about
            };
            lines.push(field("Type", Span::raw(kind)));
            hint = hint.or(about);
        }
    }
    let changed = node
        .modified
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map_or_else(|| "unknown".to_string(), format::age);
    lines.push(field("Changed", Span::raw(changed)));
    let mut clean = field("Clean", Span::raw(""));
    clean
        .spans
        .extend(cleanable(&tree.path(id), browser.roots.as_ref()).spans);
    lines.push(clean);
    lines.push(Line::default());
    let mut hint_lines = hint
        .map(|hint| {
            super::visual::hanging(Vec::new(), 0, hint, ratatui::style::Style::default(), width)
        })
        .unwrap_or_default();
    while hint_lines.len() < 2 {
        hint_lines.push(Line::default());
    }
    lines.extend(hint_lines);
    lines
}

/// Whether an item can be cleaned, in one short colored word or two.
fn cleanable(path: &std::path::Path, roots: Option<&CleanupRoots>) -> Line<'static> {
    let Some(roots) = roots else {
        return Line::from("unknown");
    };
    match roots.validate_deletable(path) {
        Ok(_) => Line::from(vec![
            Span::raw("✓ ").green().bold(),
            Span::raw("yes").green(),
        ]),
        Err(SafetyError::IsRoot) => Line::from(vec![
            Span::raw("◆ ").yellow(),
            Span::raw("what's inside").yellow(),
        ]),
        Err(SafetyError::Protected) => Line::from(vec![
            Span::raw("✗ ").red().bold(),
            Span::raw("protected").red(),
        ]),
        Err(_) => Line::from("no"),
    }
}

/// What well known folders in the home folder hold, in a few words.
const MEANINGS: &[(&str, &str)] = &[
    ("Library", "App settings, caches, and data."),
    (
        "Library/Caches",
        "Files apps can make again. Deep Clean empties it.",
    ),
    (
        "Library/Application Support",
        "App data: saved work, downloads, settings.",
    ),
    (
        "Library/Containers",
        "Data of sandboxed apps, like App Store apps and Docker.",
    ),
    (
        "Library/Group Containers",
        "Data shared by apps from one maker.",
    ),
    (
        "Library/Developer",
        "Xcode builds, simulators, and device support.",
    ),
    ("Library/Logs", "App logs. Deep Clean empties old ones."),
    ("Library/Messages", "Messages history and attachments."),
    ("Library/Mail", "Mail messages and attachments."),
    (
        "Library/Mobile Documents",
        "iCloud Drive files on this Mac.",
    ),
    (
        "Library/CloudStorage",
        "Dropbox, Google Drive, and other cloud files.",
    ),
    ("Library/Saved Application State", "Windows apps reopen."),
    ("Documents", "Your files. Never cleaned."),
    ("Desktop", "Your files. Never cleaned."),
    ("Downloads", "Your downloads. Never cleaned."),
    ("Pictures", "Photos and the Photos library."),
    ("Music", "Music and the Music library."),
    ("Movies", "Your videos."),
    (".Trash", "The Trash. Empty it to free this space."),
    (".npm", "npm's download cache."),
    (".cargo", "Rust packages and installed tools."),
    (".rustup", "Rust versions."),
    (".pyenv", "Python versions."),
    (".cache", "Command line tool caches."),
    (".config", "Command line tool settings."),
    (".local", "Command line tool programs and data."),
    (
        ".docker",
        "Docker settings. Its disk is in Library/Containers.",
    ),
    (".vscode", "VS Code extensions."),
    (".gradle", "Gradle packages and caches."),
    (".m2", "Maven packages."),
    (".ssh", "SSH keys. Never cleaned."),
];

/// What a well known folder holds, if `id` is one.
fn meaning(tree: &Tree, id: NodeId) -> Option<&'static str> {
    let path = tree.path(id);
    let rest = path.strip_prefix(tree.path(tree.root())).ok()?;
    let rest = rest.to_str()?;
    MEANINGS
        .iter()
        .find(|(name, _)| *name == rest)
        .map(|(_, meaning)| *meaning)
}

/// Browses the home folder scan by size.
pub struct Disk {
    browser: Option<Browser>,
    /// The sort to open with, from Settings, Preferences
    sort: Sort,
}

impl Disk {
    pub fn new() -> Self {
        Self {
            browser: None,
            sort: Sort::from_label(&super::settings::saved_prefs().disk_sort),
        }
    }

    /// Opens on the folder holding `id`, with `id` selected.
    pub fn showing(tree: &Tree, id: NodeId) -> Self {
        let sort = Sort::from_label(&super::settings::saved_prefs().disk_sort);
        let mut browser = Browser::sorted(tree, sort);
        if let Some(parent) = tree.get(id).parent {
            browser.show(tree, parent, Some(id));
        }
        Self {
            browser: Some(browser),
            sort,
        }
    }
}

impl Screen for Disk {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let progress = match context.scan {
            ScanStatus::Done { scan, .. } => {
                let tree = &scan.tree;
                let sort = self.sort;
                let browser = self
                    .browser
                    .get_or_insert_with(|| Browser::sorted(tree, sort));
                if area.width >= MIN_PREVIEW_WIDTH {
                    let [folder, preview] =
                        Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)])
                            .areas(area);
                    // This folder & This Mac share one height, so the bottom
                    // boxes line up, and the table fills the rest
                    let left = usize::from(folder.width.saturating_sub(4));
                    let right = usize::from(preview.width.saturating_sub(4));
                    let this = this_folder(tree, browser, left);
                    let mac = this_mac(
                        context.disk.as_ref(),
                        tree.get(tree.root()).total_size,
                        right,
                    );
                    let bottom = height_of(&this, folder.width).max(height_of(&mac, preview.width));
                    if folder.height >= bottom + 6 {
                        let [table, rest] =
                            Layout::vertical([Constraint::Fill(1), Constraint::Length(bottom)])
                                .areas(folder);
                        draw_folder(frame, table, tree, browser, false);
                        frame.render_widget(
                            Paragraph::new(this)
                                .wrap(ratatui::widgets::Wrap { trim: false })
                                .block(
                                    super::visual::block()
                                        .title(" This folder ")
                                        .padding(Padding::horizontal(1)),
                                ),
                            rest,
                        );
                    } else {
                        draw_folder(frame, folder, tree, browser, true);
                    }
                    draw_preview(frame, preview, tree, browser, context.disk.as_ref(), bottom);
                } else {
                    draw_folder(frame, area, tree, browser, true);
                }
                return;
            }
            ScanStatus::Running(progress) => progress,
            ScanStatus::Failed(reason) => {
                let block = super::visual::block()
                    .title(" Disk ")
                    .padding(Padding::horizontal(1));
                frame.render_widget(
                    Paragraph::new(format!("The scan failed: {reason}"))
                        .red()
                        .wrap(ratatui::widgets::Wrap { trim: true })
                        .block(block),
                    area,
                );
                return;
            }
        };
        scanning("Disk", "Loading folders by size.", *progress).draw(frame, area);
    }

    fn handle_key(&mut self, key: KeyEvent, context: &Context) -> Action {
        let (ScanStatus::Done { scan, .. }, Some(browser)) = (context.scan, self.browser.as_mut())
        else {
            return Action::None;
        };
        let tree = &scan.tree;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => browser.up(),
            KeyCode::Down | KeyCode::Char('j') => browser.down(),
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => browser.enter(tree),
            KeyCode::Left | KeyCode::Char('h') => browser.leave(tree),
            KeyCode::Char('s') => browser.cycle_sort(tree),
            KeyCode::Char('g') | KeyCode::Home => browser.move_to(0),
            KeyCode::Char('G') | KeyCode::End => browser.move_to(usize::MAX),
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · → open · ← up · s sort · esc home · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between items"),
            ("→  Enter  l", "Open the selected folder"),
            ("←  h", "Go up to the parent folder"),
            ("s", "Sort by size, name, or items"),
            ("g  G", "Jump to the first or last row"),
            ("Esc", "Back to Home"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neet_core::scan::{Progress, Scan};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;

    /// ~/big/ (100), ~/big/inner.bin (100), ~/apple.txt (5), ~/Zed/ (1, 2 items)
    fn sample() -> Tree {
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let big = tree.add(root, "big", NodeKind::Directory, 0);
        let _ = tree.add(big, "inner.bin", NodeKind::File, 100);
        let _ = tree.add(root, "apple.txt", NodeKind::File, 5);
        let zed = tree.add(root, "Zed", NodeKind::Directory, 0);
        let _ = tree.add(zed, "a", NodeKind::File, 1);
        let _ = tree.add(zed, "b", NodeKind::File, 0);
        tree
    }

    fn names(tree: &Tree, browser: &Browser) -> Vec<String> {
        browser
            .rows
            .iter()
            .map(|&id| display_name(tree, id))
            .collect()
    }

    fn selected_name(tree: &Tree, browser: &Browser) -> String {
        display_name(tree, browser.selected().expect("a row should be selected"))
    }

    #[test]
    fn starts_at_the_root_sorted_by_size() {
        let tree = sample();
        let browser = Browser::new(&tree);

        assert_eq!(names(&tree, &browser), ["big/", "apple.txt", "Zed/"]);
        assert_eq!(selected_name(&tree, &browser), "big/");
    }

    #[test]
    fn sort_cycles_through_name_and_items_and_keeps_the_selection() {
        let tree = sample();
        let mut browser = Browser::new(&tree);
        browser.down();

        browser.cycle_sort(&tree);
        assert_eq!(names(&tree, &browser), ["apple.txt", "big/", "Zed/"]);
        assert_eq!(selected_name(&tree, &browser), "apple.txt");

        browser.cycle_sort(&tree);
        assert_eq!(names(&tree, &browser), ["Zed/", "big/", "apple.txt"]);

        browser.cycle_sort(&tree);
        assert_eq!(browser.sort, Sort::Size);
    }

    #[test]
    fn entering_and_leaving_a_folder_keeps_your_place() {
        let tree = sample();
        let mut browser = Browser::new(&tree);
        browser.move_to(2);

        browser.enter(&tree);
        assert_eq!(display_path(&tree, browser.current), "~/Zed");
        assert_eq!(names(&tree, &browser), ["a", "b"]);

        browser.leave(&tree);
        assert_eq!(display_path(&tree, browser.current), "~");
        assert_eq!(selected_name(&tree, &browser), "Zed/");
    }

    #[test]
    fn entering_a_file_or_leaving_the_root_does_nothing() {
        let tree = sample();
        let mut browser = Browser::new(&tree);
        browser.down();

        browser.enter(&tree);
        browser.leave(&tree);

        assert_eq!(browser.current, tree.root());
        assert_eq!(selected_name(&tree, &browser), "apple.txt");
    }

    #[test]
    fn selection_stops_at_the_ends() {
        let tree = sample();
        let mut browser = Browser::new(&tree);

        browser.up();
        assert_eq!(browser.list.selected(), Some(0));
        browser.move_to(usize::MAX);
        browser.down();
        assert_eq!(browser.list.selected(), Some(2));
    }

    fn render(disk: &mut Disk, scan: &ScanStatus, width: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        let context = Context {
            scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        terminal
            .draw(|frame| disk.draw(frame, frame.area(), &context))
            .unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn shows_folders_and_a_preview_once_the_scan_is_done() {
        let scan = ScanStatus::Done {
            scan: Scan {
                tree: sample(),
                errors: Vec::new(),
                other_disks: Vec::new(),
            },
            elapsed: std::time::Duration::ZERO,
        };
        let mut disk = Disk::new();

        let screen = render(&mut disk, &scan, 120);
        assert!(screen.contains("big/"));
        assert!(screen.contains("inner.bin"));
        assert!(screen.contains("By size"));

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        disk.handle_key(
            key,
            &Context {
                scan: &scan,
                disk: None,
                cleanable: None,
                plan: None,
            },
        );
        let screen = render(&mut disk, &scan, 120);
        assert!(screen.contains("~/big"));
    }

    #[test]
    fn waits_while_the_scan_runs() {
        let scan = ScanStatus::Running(Progress {
            entries: 42,
            bytes: 2_000_000,
        });

        let screen = render(&mut Disk::new(), &scan, 80);

        assert!(screen.contains("Scanning home"));
        assert!(screen.contains("42 items · 2.0 MB so far"));
    }

    #[test]
    fn the_selected_row_is_bold_without_a_filled_background() {
        let scan = ScanStatus::Done {
            scan: Scan {
                tree: sample(),
                errors: Vec::new(),
                other_disks: Vec::new(),
            },
            elapsed: std::time::Duration::ZERO,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        terminal
            .draw(|frame| Disk::new().draw(frame, frame.area(), &context))
            .unwrap();
        let buffer = terminal.backend().buffer();

        // The first row follows the border and the header.
        let arrow = &buffer[(2, 2)];
        assert_eq!(arrow.symbol(), "▸");
        assert!(arrow.modifier.contains(ratatui::style::Modifier::BOLD));
        assert!(!arrow.modifier.contains(ratatui::style::Modifier::REVERSED));
        assert_eq!(arrow.bg, ratatui::style::Color::Reset);
    }

    #[test]
    fn the_preview_says_whether_an_item_can_be_cleaned() {
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        for folder in ["Library/Caches/app", "Documents", "notes"] {
            std::fs::create_dir_all(dir.path().join(folder)).expect("folder should be created");
        }
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        let status = |path: &str| cleanable(&dir.path().join(path), Some(&roots)).to_string();

        assert!(status("Library/Caches/app").contains("yes"));
        assert!(status("Library/Caches").contains("what's inside"));
        assert!(status("Documents").contains("protected"));
        assert_eq!(status("notes"), "no");
    }

    #[test]
    fn well_known_folders_are_explained() {
        let mut tree = Tree::new("/Users/someone");
        let root = tree.root();
        let library = tree.add(root, "Library", NodeKind::Directory, 0);
        let caches = tree.add(library, "Caches", NodeKind::Directory, 0);
        let other = tree.add(root, "project", NodeKind::Directory, 0);

        assert!(meaning(&tree, caches).is_some_and(|text| text.contains("make again")));
        assert!(meaning(&tree, library).is_some());
        assert_eq!(meaning(&tree, other), None);
    }
    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let scan = ScanStatus::Done {
            scan: Scan {
                tree: sample(),
                errors: Vec::new(),
                other_disks: Vec::new(),
            },
            elapsed: std::time::Duration::ZERO,
        };
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let mut screen = Disk::new();
        for (width, height) in view::SIZES {
            let buffer = view::render("disk", &mut screen, &context, width, height);
            view::aligned(&buffer, "Size", "100 B");
            view::aligned(&buffer, "%", "94%");
            assert!(view::text(&buffer).contains("apple.txt"));
        }
    }
}
