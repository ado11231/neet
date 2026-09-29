use std::cmp::Reverse;

use std::sync::Arc;
use std::time::SystemTime;

use neet_core::clean;
use neet_core::safety::CleanupRoots;
use neet_core::tree::{NodeId, NodeKind, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph};

use super::app::{Action, Context, Screen};
use super::clean::{Planned, skip_reason};
use super::format;
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
}

impl Browser {
    fn new(tree: &Tree) -> Self {
        let mut browser = Self {
            current: tree.root(),
            rows: Vec::new(),
            list: ListState::default(),
            sort: Sort::Size,
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
        Span::raw(format::bar(node.total_size, parent_total, BAR_WIDTH)).cyan(),
        Span::raw(format!(" {:>9} ", format::size(node.total_size))),
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
        .highlight_style(Style::new().reversed());
    frame.render_stateful_widget(list, area, &mut browser.list);
}

fn draw_preview(frame: &mut Frame, area: Rect, tree: &Tree, browser: &Browser) {
    let block = Block::bordered().padding(Padding::horizontal(1));
    let Some(id) = browser.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let node = tree.get(id);
    if node.kind == NodeKind::Directory {
        let rows = sorted_children(tree, id, browser.sort);
        let block = block.title(format!(" {} ", display_name(tree, id)));
        if rows.is_empty() {
            frame.render_widget(
                Paragraph::new("Empty folder.").dark_gray().block(block),
                area,
            );
            return;
        }
        let items = rows.iter().map(|&child| row(tree, child, node.total_size));
        frame.render_widget(List::new(items).block(block), area);
    } else {
        let kind = match node.kind {
            NodeKind::File => "File",
            NodeKind::Symlink => "Symbolic link, not followed",
            NodeKind::Directory | NodeKind::Other => "Other",
        };
        let lines = vec![
            Line::from(display_name(tree, id)).bold(),
            Line::default(),
            Line::from(format!("Size on disk: {}", format::size(node.total_size))),
            Line::from(format!("Kind: {kind}")),
            Line::from(tree.path(id).display().to_string()).dark_gray(),
        ];
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
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
        let message = match context.scan {
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
            ScanStatus::Running(progress) => format!(
                "Scanning your home folder… {} items so far. The folders show here when the scan finishes.",
                format::count(progress.entries)
            ),
            ScanStatus::Failed(reason) => {
                format!("The scan failed, so there is nothing to show. {reason}")
            }
        };
        let block = Block::bordered()
            .title(" Disk ")
            .padding(Padding::horizontal(1));
        frame.render_widget(
            Paragraph::new(message)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .block(block),
            area,
        );
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
        .map_err(|error| format!("the home folder could not be read: {error}"))
        .and_then(|roots| {
            clean::plan_path(&path, &roots, SystemTime::now())
                .map(|plan| (plan, roots.home().to_path_buf()))
                .map_err(|reason| skip_reason(&reason, None))
        });
    match result {
        Ok((plan, home)) => Action::Open(Box::new(Review::new(Arc::new(Planned {
            plan,
            errors: Vec::new(),
            home,
        })))),
        Err(reason) => Action::Open(Box::new(Notice::new(
            "Cannot clean this",
            vec![
                Line::from(display_path(tree, id)).bold(),
                Line::from(reason),
                Line::default(),
                Line::from(
                    "Cleanup only moves items inside the cache, log, and build folders \
                     listed in SAFETY.md.",
                )
                .dark_gray(),
            ],
        ))),
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
            bytes: 0,
        });

        let screen = render(&mut Disk::new(), &scan, 80);

        assert!(screen.contains("42 items so far"));
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
            assert!(screen.hints().contains("any key close"));
        }
        assert!(dir.path().join("Library/Caches/app/file").exists());
    }
}
