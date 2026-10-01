use std::cmp::Reverse;

use std::sync::Arc;
use std::time::SystemTime;

use neet_core::clean::{self, SkipReason};
use neet_core::safety::{CleanupRoots, SafetyError};
use neet_core::tree::{NodeId, NodeKind, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Planned, skip_reason};
use super::format;
use super::loading::scanning;
use super::review::{Notice, Review};
use super::scan::ScanStatus;

/// How many characters wide each size bar is.
const BAR_WIDTH: usize = 12;

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
}

/// Where the user is in the scan tree.
struct Browser {
    current: NodeId,
    rows: Vec<NodeId>,
    list: ListState,
    sort: Sort,
    /// For saying whether the selected item can be cleaned
    roots: Option<CleanupRoots>,
}

impl Browser {
    fn new(tree: &Tree) -> Self {
        let mut browser = Self {
            current: tree.root(),
            rows: Vec::new(),
            list: ListState::default(),
            sort: Sort::Size,
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

fn row(tree: &Tree, id: NodeId, parent_total: u64) -> ListItem<'static> {
    let node = tree.get(id);
    let name = Span::raw(display_name(tree, id));
    let name = if node.kind == NodeKind::Directory {
        name.blue().bold()
    } else {
        name
    };
    ListItem::new(Line::from(vec![
        Span::raw(format::bar(node.total_size, parent_total, BAR_WIDTH))
            .fg(bar_color(node.total_size)),
        Span::raw(" "),
        format::size_span(
            node.total_size,
            format!("{:>9}", format::size(node.total_size)),
        ),
        Span::raw(" "),
        Span::raw(format!(
            "{:>3}%  ",
            format::percent(node.total_size, parent_total)
        ))
        .dark_gray(),
        name,
    ]))
}

fn draw_folder(frame: &mut Frame, area: Rect, tree: &Tree, browser: &mut Browser) {
    let folder = tree.get(browser.current);
    let title = format!(" {} ", display_path(tree, browser.current));
    let summary = format!(
        " {} · {} items · sort: {} ",
        format::size(folder.total_size),
        format::count(folder.total_items),
        browser.sort.label()
    );
    let block = Block::bordered()
        .title(title)
        .title(Line::from(summary).right_aligned())
        .padding(Padding::horizontal(1));

    if browser.rows.is_empty() {
        frame.render_widget(
            Paragraph::new("Empty folder.").dark_gray().block(block),
            area,
        );
        return;
    }
    let items = browser
        .rows
        .iter()
        .map(|&id| row(tree, id, folder.total_size));
    let list = List::new(items)
        .block(block)
        .highlight_symbol("▸ ")
        .highlight_style(Style::new().bold().cyan());
    frame.render_stateful_widget(list, area, &mut browser.list);
}

fn draw_preview(frame: &mut Frame, area: Rect, tree: &Tree, browser: &Browser) {
    let block = Block::bordered().padding(Padding::horizontal(1));
    let Some(id) = browser.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let node = tree.get(id);
    let block = block.title(
        Line::from(format!(" {} ", display_name(tree, id)))
            .bold()
            .cyan(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let header = about(tree, id, browser);
    // Each header line wraps, so count how many rows it takes.
    let width = usize::from(inner.width.max(1));
    let height: usize = header
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum();
    let [top, rest] = Layout::vertical([
        Constraint::Length(u16::try_from(height).unwrap_or(u16::MAX)),
        Constraint::Fill(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), top);

    if node.kind != NodeKind::Directory || rest.height < 3 {
        return;
    }
    let rows = sorted_children(tree, id, browser.sort);
    let [heading, list] =
        Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(rest);
    let title = if rows.is_empty() {
        "Empty folder.".to_string()
    } else {
        format!("Inside, by {}", browser.sort.label())
    };
    frame.render_widget(
        Paragraph::new(vec![Line::default(), Line::from(title).dark_gray()]),
        heading,
    );
    let items = rows.iter().map(|&child| row(tree, child, node.total_size));
    frame.render_widget(List::new(items), list);
}

/// The bar color for an item of `bytes`, matching the size colors.
fn bar_color(bytes: u64) -> Color {
    if bytes >= format::HUGE {
        Color::Red
    } else if bytes >= format::HUGE / 5 {
        Color::Yellow
    } else {
        Color::Cyan
    }
}

/// What the selected item is, how big, and whether neet can clean it.
fn about(tree: &Tree, id: NodeId, browser: &Browser) -> Vec<Line<'static>> {
    let node = tree.get(id);
    let parent_total = tree.get(browser.current).total_size;
    let mut lines = vec![Line::from(display_path(tree, id)).dark_gray()];
    if let Some(meaning) = meaning(tree, id) {
        lines.push(Line::default());
        lines.push(Line::from(meaning));
    }
    lines.push(Line::default());

    let label = |text: &str| Span::raw(format!("{text:<9}")).dark_gray();
    lines.push(Line::from(vec![
        label("Size"),
        format::size_span(node.total_size, format::size(node.total_size)),
        Span::raw(format!(
            "  {}% of {}",
            format::percent(node.total_size, parent_total),
            display_path(tree, browser.current)
        ))
        .dark_gray(),
    ]));
    match node.kind {
        NodeKind::Directory => lines.push(Line::from(vec![
            label("Items"),
            Span::raw(format::count(node.total_items)),
        ])),
        NodeKind::Symlink => lines.push(Line::from(vec![
            label("Kind"),
            Span::raw("Link, not followed"),
        ])),
        NodeKind::File | NodeKind::Other => {}
    }
    if let Some(changed) = node
        .modified
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
    {
        lines.push(Line::from(vec![
            label("Changed"),
            Span::raw(format::age(changed)),
        ]));
    }
    lines.push(Line::default());
    lines.push(cleanable(&tree.path(id), browser.roots.as_ref()));
    lines
}

/// Whether `d` can clean an item, in one colored line.
fn cleanable(path: &std::path::Path, roots: Option<&CleanupRoots>) -> Line<'static> {
    let Some(roots) = roots else {
        return Line::from("neet could not read the home folder, so nothing can be cleaned.")
            .dark_gray();
    };
    match roots.validate_deletable(path) {
        Ok(_) => Line::from(vec![
            Span::raw("✓ ").green().bold(),
            Span::raw("Can be cleaned. Press d to review it.").green(),
        ]),
        Err(SafetyError::IsRoot) => Line::from(vec![
            Span::raw("◆ ").yellow(),
            Span::raw("A cleanup folder. Open it and clean the items inside.").yellow(),
        ]),
        Err(SafetyError::Protected) => Line::from(vec![
            Span::raw("✗ ").red().bold(),
            Span::raw("Protected. neet never moves anything in it.").red(),
        ]),
        Err(_) => Line::from(vec![
            Span::raw("· ").dark_gray(),
            Span::raw("Outside the folders neet cleans.").dark_gray(),
        ]),
    }
}

/// What well known folders in the home folder hold, in plain words.
const MEANINGS: &[(&str, &str)] = &[
    (
        "Library",
        "Settings, caches, and data that apps keep for you.",
    ),
    (
        "Library/Caches",
        "Files apps can make again. Clean empties these.",
    ),
    (
        "Library/Application Support",
        "Data apps keep, such as saved work, downloads, and settings.",
    ),
    (
        "Library/Containers",
        "Data of apps that run in a sandbox, such as App Store apps and Docker.",
    ),
    (
        "Library/Group Containers",
        "Data shared between apps from the same maker.",
    ),
    (
        "Library/Developer",
        "Xcode builds, simulators, and device support files.",
    ),
    ("Library/Logs", "Logs apps write. Clean can empty old ones."),
    ("Library/Messages", "Your Messages history and attachments."),
    ("Library/Mail", "Your Mail messages and attachments."),
    (
        "Library/Mobile Documents",
        "iCloud Drive files kept on this Mac.",
    ),
    (
        "Library/CloudStorage",
        "Files from cloud services, such as Dropbox or Google Drive.",
    ),
    (
        "Library/Saved Application State",
        "Windows apps reopen where you left off.",
    ),
    ("Documents", "Your own files. neet never touches them."),
    ("Desktop", "Your own files. neet never touches them."),
    (
        "Downloads",
        "Files you downloaded. neet never touches them.",
    ),
    ("Pictures", "Your photos, including the Photos library."),
    ("Music", "Your music and the Music library."),
    ("Movies", "Your videos."),
    (".Trash", "Your Trash. Emptying it frees this space."),
    (
        ".npm",
        "npm, the Node.js package manager. Mostly its download cache.",
    ),
    (
        ".cargo",
        "Rust's package manager: downloaded packages and installed programs.",
    ),
    (".rustup", "Rust versions installed by rustup."),
    (".pyenv", "Python versions installed by pyenv."),
    (".cache", "Caches some command line tools keep."),
    (".config", "Settings for command line tools."),
    (
        ".local",
        "Programs and data some command line tools install.",
    ),
    (
        ".docker",
        "Docker settings. Its disk image is in Library/Containers.",
    ),
    (".vscode", "Visual Studio Code extensions."),
    (
        ".gradle",
        "Gradle, a Java build tool: downloaded packages and caches.",
    ),
    (".m2", "Maven, a Java build tool: downloaded packages."),
    (".ssh", "Your SSH keys. neet never touches them."),
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
}

impl Disk {
    pub fn new() -> Self {
        Self { browser: None }
    }

    /// Opens on the folder holding `id`, with `id` selected.
    pub fn showing(tree: &Tree, id: NodeId) -> Self {
        let mut browser = Browser::new(tree);
        if let Some(parent) = tree.get(id).parent {
            browser.show(tree, parent, Some(id));
        }
        Self {
            browser: Some(browser),
        }
    }
}

impl Screen for Disk {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let progress = match context.scan {
            ScanStatus::Done { scan, .. } => {
                let tree = &scan.tree;
                let browser = self.browser.get_or_insert_with(|| Browser::new(tree));
                if area.width >= MIN_PREVIEW_WIDTH {
                    let [folder, preview] =
                        Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)])
                            .areas(area);
                    draw_folder(frame, folder, tree, browser);
                    draw_preview(frame, preview, tree, browser);
                } else {
                    draw_folder(frame, area, tree, browser);
                }
                return;
            }
            ScanStatus::Running(progress) => progress,
            ScanStatus::Failed(reason) => {
                let block = Block::bordered()
                    .title(" Disk ")
                    .padding(Padding::horizontal(1));
                frame.render_widget(
                    Paragraph::new(format!(
                        "The scan failed, so there is nothing to show. {reason}"
                    ))
                    .red()
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .block(block),
                    area,
                );
                return;
            }
        };
        scanning(
            "Disk",
            "Your folders show here when the scan finishes.",
            *progress,
        )
        .draw(frame, area);
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
            KeyCode::Char('d') => {
                if let Some(id) = browser.selected() {
                    return plan_cleanup(tree, id);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · → open · ← up · s sort · d clean · esc home · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move the selection"),
            ("→  Enter  l", "Open the selected folder"),
            ("←  h", "Go up to the parent folder"),
            ("s", "Sort by size, name, or items"),
            ("g  G", "Jump to the first or last row"),
            ("d", "Review moving the selected item to the Trash"),
            ("Esc", "Go back to Home"),
            ("q", "Quit"),
        ]
    }
}

/// Plans a cleanup of one item, with the same checks as any rule, and opens
/// the review. If the item cannot be cleaned, says why.
pub(super) fn plan_cleanup(tree: &Tree, id: NodeId) -> Action {
    let home = tree.path(tree.root());
    let path = tree.path(id);
    let result = CleanupRoots::new(&home)
        .map_err(|error| vec![format!("The home folder could not be read: {error}.")])
        .and_then(|roots| {
            clean::plan_path(&path, &roots, SystemTime::now())
                .map(|plan| (plan, roots.home().to_path_buf()))
                .map_err(|reason| refusal(&reason))
        });
    match result {
        Ok((plan, home)) => Action::Open(Box::new(Review::new(Arc::new(Planned {
            plan,
            errors: Vec::new(),
            home,
        })))),
        Err(why) => {
            let mut lines = vec![Line::from(display_path(tree, id)).bold(), Line::default()];
            lines.extend(why.into_iter().map(Line::from));
            Action::Open(Box::new(Notice::new("neet will not move this", lines)))
        }
    }
}

/// Why an item picked in Disk cannot be cleaned, in plain sentences.
fn refusal(reason: &SkipReason) -> Vec<String> {
    match reason {
        SkipReason::Refused(SafetyError::OutsideRoots) => vec![
            "It is outside the folders neet cleans.".to_string(),
            "neet only moves items inside cache, log, and build folders, such as \
             ~/Library/Caches, ~/Library/Logs, and ~/.npm/_cacache."
                .to_string(),
        ],
        SkipReason::Refused(SafetyError::IsRoot) => vec![
            "It is a whole cleanup folder.".to_string(),
            "Open it with → and pick an item inside it instead.".to_string(),
        ],
        SkipReason::Refused(SafetyError::Protected) => {
            vec!["It holds your own files or keys, so neet never moves anything in it.".to_string()]
        }
        SkipReason::Refused(error) => vec![sentence(&error.to_string())],
        other => vec![sentence(&skip_reason(other, None))],
    }
}

/// `text` with a capital first letter and a full stop.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), chars.as_str()),
        None => String::new(),
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
        let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
        let context = Context {
            scan,
            disk: None,
            cleanable: None,
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
        assert!(screen.contains("sort: size"));

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        disk.handle_key(
            key,
            &Context {
                scan: &scan,
                disk: None,
                cleanable: None,
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

        assert!(screen.contains("Scanning your home folder"));
        assert!(screen.contains("42 items · 2.0 MB so far"));
    }

    #[test]
    fn the_selected_row_is_cyan_without_a_filled_background() {
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
        };
        terminal
            .draw(|frame| Disk::new().draw(frame, frame.area(), &context))
            .unwrap();
        let buffer = terminal.backend().buffer();

        // The first row sits below the top border, after the padding.
        let arrow = &buffer[(2, 1)];
        assert_eq!(arrow.symbol(), "▸");
        assert_eq!(arrow.fg, ratatui::style::Color::Cyan);
        assert!(!arrow.modifier.contains(ratatui::style::Modifier::REVERSED));
        assert_eq!(arrow.bg, ratatui::style::Color::Reset);
    }

    #[test]
    fn d_reviews_an_item_in_a_cleanup_folder_and_explains_any_other() {
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        for file in ["Library/Caches/app/file", "Documents/keep.txt"] {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().expect("path should have a parent"))
                .expect("folder should be created");
            std::fs::write(path, "x").expect("file should be written");
        }
        let mut tree = Tree::new(dir.path());
        let root = tree.root();
        let library = tree.add(root, "Library", NodeKind::Directory, 0);
        let caches = tree.add(library, "Caches", NodeKind::Directory, 0);
        let app = tree.add(caches, "app", NodeKind::Directory, 0);
        let documents = tree.add(root, "Documents", NodeKind::Directory, 0);

        let Action::Open(screen) = plan_cleanup(&tree, app) else {
            panic!("a cache folder should open the review");
        };
        assert!(screen.hints().contains("enter continue"));

        for refused in [documents, caches, library] {
            let Action::Open(screen) = plan_cleanup(&tree, refused) else {
                panic!("a refused item should open a notice");
            };
            assert!(screen.hints().contains("any key to close"));
        }
        assert!(dir.path().join("Library/Caches/app/file").exists());
    }

    #[test]
    fn the_preview_says_whether_an_item_can_be_cleaned() {
        let dir = tempfile::tempdir().expect("temporary directory should be created");
        for folder in ["Library/Caches/app", "Documents", "notes"] {
            std::fs::create_dir_all(dir.path().join(folder)).expect("folder should be created");
        }
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        let status = |path: &str| cleanable(&dir.path().join(path), Some(&roots)).to_string();

        assert!(status("Library/Caches/app").contains("Can be cleaned"));
        assert!(status("Library/Caches").contains("A cleanup folder"));
        assert!(status("Documents").contains("Protected"));
        assert!(status("notes").contains("Outside the folders neet cleans"));
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
    fn a_refusal_says_why_in_plain_sentences() {
        let outside = refusal(&SkipReason::Refused(SafetyError::OutsideRoots));
        assert_eq!(outside[0], "It is outside the folders neet cleans.");
        assert!(outside[1].contains("~/Library/Caches"));
        let root = refusal(&SkipReason::Refused(SafetyError::IsRoot));
        assert!(root[1].contains("pick an item inside it"));
        assert_eq!(
            refusal(&SkipReason::Refused(SafetyError::Link)),
            ["The path is a link."]
        );
        assert_eq!(refusal(&SkipReason::Link), ["A link, left in place."]);
    }
}
