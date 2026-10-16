//! Treemap: the folder open in Disk drawn as boxes, each as big as the
//! space it takes. View only, from the home folder scan.

use std::cmp::Reverse;

use neet_core::tree::{NodeId, NodeKind, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::app::{Action, Context, Screen};
use super::format;
use super::scan::ScanStatus;
use super::visual;

/// At most this many boxes. The rest share one box.
const MAX_BOXES: usize = 24;

/// From this width the selected box's details show on the right.
const SIDE_WIDTH: u16 = 100;

/// Width of the details on the right
const SIDE: u16 = 34;

/// Terminal cells are about twice as tall as they are wide, so heights
/// count double when making boxes square.
const CELL_RATIO: f64 = 2.0;

/// One box: an item, or the smaller items together
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tile {
    Item(NodeId),
    /// How many smaller items share the last box
    Rest(usize),
}

/// Lays out `sizes`, largest first, as rectangles filling `area`, each with
/// an area in proportion to its size. Squarified, so boxes stay close to
/// square. Rectangles share edges and leave no gaps.
// Sizes only need to be close, and positions are rounded to cells and
// clamped to the area.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn squarify(sizes: &[u64], area: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().map(|&size| size as f64).sum();
    if sizes.is_empty() || total <= 0.0 || area.width == 0 || area.height == 0 {
        return vec![Rect::default(); sizes.len()];
    }
    // Work in a space where a cell's height counts double.
    let (width, height) = (f64::from(area.width), f64::from(area.height) * CELL_RATIO);
    let scale = width * height / total;
    let areas: Vec<f64> = sizes.iter().map(|&size| size as f64 * scale).collect();
    let mut placed = Vec::with_capacity(sizes.len());
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, width, height);
    let mut start = 0;
    while start < areas.len() {
        let side = w.min(h);
        // Grow the row while it makes the worst box more square.
        let mut end = start + 1;
        while end < areas.len()
            && worst(&areas[start..=end], side) <= worst(&areas[start..end], side)
        {
            end += 1;
        }
        let row: f64 = areas[start..end].iter().sum();
        if w >= h {
            // A column on the left
            let column = if h > 0.0 { row / h } else { 0.0 };
            let mut offset = y;
            for &item in &areas[start..end] {
                let part = if column > 0.0 { item / column } else { 0.0 };
                placed.push((x, offset, column, part));
                offset += part;
            }
            x += column;
            w -= column;
        } else {
            // A row along the top
            let band = if w > 0.0 { row / w } else { 0.0 };
            let mut offset = x;
            for &item in &areas[start..end] {
                let part = if band > 0.0 { item / band } else { 0.0 };
                placed.push((offset, y, part, band));
                offset += part;
            }
            y += band;
            h -= band;
        }
        start = end;
    }
    // Round each edge, so boxes that touch share the same cell edge.
    placed
        .into_iter()
        .map(|(x, y, w, h)| {
            let left = (x.round() as u16).min(area.width);
            let right = ((x + w).round() as u16).min(area.width);
            let top = ((y / CELL_RATIO).round() as u16).min(area.height);
            let bottom = (((y + h) / CELL_RATIO).round() as u16).min(area.height);
            Rect::new(
                area.x + left,
                area.y + top,
                right.saturating_sub(left),
                bottom.saturating_sub(top),
            )
        })
        .collect()
}

/// The most stretched box in a row of `areas` laid along `side`
fn worst(areas: &[f64], side: f64) -> f64 {
    let sum: f64 = areas.iter().sum();
    let largest = areas.iter().copied().fold(0.0, f64::max);
    let smallest = areas.iter().copied().fold(f64::MAX, f64::min);
    if sum <= 0.0 || smallest <= 0.0 {
        return f64::MAX;
    }
    let side2 = side * side;
    let sum2 = sum * sum;
    (side2 * largest / sum2).max(sum2 / (side2 * smallest))
}

/// The boxes for `folder`: its largest items, and the rest in one box
fn tiles(tree: &Tree, folder: NodeId) -> Vec<(Tile, u64)> {
    let mut children: Vec<NodeId> = tree
        .get(folder)
        .children
        .iter()
        .copied()
        .filter(|&id| tree.get(id).total_size > 0)
        .collect();
    children.sort_by_key(|&id| Reverse(tree.get(id).total_size));
    let mut tiles: Vec<(Tile, u64)> = children
        .iter()
        .take(MAX_BOXES)
        .map(|&id| (Tile::Item(id), tree.get(id).total_size))
        .collect();
    if children.len() > MAX_BOXES {
        let rest = &children[MAX_BOXES..];
        let size = rest.iter().map(|&id| tree.get(id).total_size).sum();
        tiles.push((Tile::Rest(rest.len()), size));
    }
    tiles
}

/// A box's color: red from 5 GB and yellow from 1 GB, as elsewhere, then
/// blue for folders and green for files
fn color(tree: &Tree, tile: Tile, size: u64) -> Color {
    if size >= format::HUGE {
        Color::Red
    } else if size >= format::HUGE / 5 {
        Color::Yellow
    } else {
        match tile {
            Tile::Item(id) if tree.get(id).kind == NodeKind::Directory => visual::ACCENT,
            Tile::Item(_) => Color::Green,
            Tile::Rest(_) => Color::Magenta,
        }
    }
}

fn tile_name(tree: &Tree, tile: Tile) -> String {
    match tile {
        Tile::Item(id) => {
            let node = tree.get(id);
            let name = node.name.to_string_lossy();
            if node.kind == NodeKind::Directory {
                format!("{name}/")
            } else {
                name.into_owned()
            }
        }
        Tile::Rest(count) => format!("{count} smaller"),
    }
}

/// The folder open in Disk, as boxes
pub struct Treemap {
    folder: NodeId,
    selected: usize,
    /// Folders opened from here, with the box selected in each, for going
    /// back up
    trail: Vec<(NodeId, usize)>,
}

impl Treemap {
    pub fn new(folder: NodeId) -> Self {
        Self {
            folder,
            selected: 0,
            trail: Vec::new(),
        }
    }

    fn draw_map(&self, frame: &mut Frame, area: Rect, tree: &Tree, tiles: &[(Tile, u64)]) {
        let path = super::disk::display_path(tree, self.folder);
        let total = tree.get(self.folder).total_size;
        let block = visual::block()
            .title(format!(
                " {} ",
                format::shorten_path(&path, usize::from(area.width / 2))
            ))
            .title(Line::from(format!(" {} ", format::size(total))).right_aligned());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if tiles.is_empty() {
            frame.render_widget(
                Paragraph::new("Nothing here takes space.").centered(),
                Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1),
            );
            return;
        }
        let sizes: Vec<u64> = tiles.iter().map(|&(_, size)| size).collect();
        for (index, (rect, &(tile, size))) in squarify(&sizes, inner).iter().zip(tiles).enumerate()
        {
            if rect.width == 0 || rect.height == 0 {
                continue;
            }
            let selected = index == self.selected;
            let color = color(tree, tile, size);
            let style = if selected {
                Style::new().fg(color).bold()
            } else {
                Style::new().fg(color)
            };
            if rect.width < 3 || rect.height < 2 {
                // Too small for a border: a block of shade instead
                let fill = if selected { "█" } else { "░" };
                let row = fill.repeat(usize::from(rect.width));
                let lines: Vec<Line> = (0..rect.height)
                    .map(|_| Line::from(Span::styled(row.clone(), style)))
                    .collect();
                frame.render_widget(Paragraph::new(lines), *rect);
                continue;
            }
            let block = Block::new()
                .borders(Borders::ALL)
                .border_set(if selected {
                    ratatui::symbols::border::THICK
                } else {
                    ratatui::symbols::border::PLAIN
                })
                .border_style(style);
            let room = usize::from(rect.width.saturating_sub(2));
            let mut lines = vec![Line::from(Span::styled(
                format::shorten_middle(&tile_name(tree, tile), room),
                if selected {
                    visual::SELECTED
                } else {
                    Style::new()
                },
            ))];
            if rect.height >= 4 {
                lines.push(Line::from(format::size_span(
                    size,
                    format::shorten_middle(&format::size(size), room),
                )));
            }
            frame.render_widget(Paragraph::new(lines).block(block), *rect);
        }
    }

    fn details(&self, tree: &Tree, tiles: &[(Tile, u64)]) -> Vec<Vec<Line<'static>>> {
        let total = tree.get(self.folder).total_size;
        let field = |label: &str, value: Span<'static>| {
            Line::from(vec![Span::raw(format!("{label:<8}")).bold(), value])
        };
        let mut selected = vec![Line::from("Selected").style(visual::HEADING)];
        if let Some(&(tile, size)) = tiles.get(self.selected) {
            selected.push(Line::from(tile_name(tree, tile)).bold());
            selected.push(field("Size", format::size_span(size, format::size(size))));
            selected.push(field(
                "Share",
                Span::raw(format!("{}% of this folder", format::percent(size, total))),
            ));
            if let Tile::Item(id) = tile {
                let node = tree.get(id);
                if node.kind == NodeKind::Directory {
                    selected.push(field("Items", Span::raw(format::count(node.total_items))));
                    selected.push(Line::default());
                    selected.push(Line::from("Enter opens it."));
                }
            } else {
                selected.push(Line::default());
                selected.push(Line::from("Too small to draw one by one. Disk lists them."));
            }
        }
        let legend = vec![
            Line::from("Colors").style(visual::HEADING),
            Line::from(vec![Span::raw("■ ").red(), Span::raw("5 GB and up")]),
            Line::from(vec![Span::raw("■ ").yellow(), Span::raw("1 GB and up")]),
            Line::from(vec![
                Span::raw("■ ").fg(visual::ACCENT),
                Span::raw("folder"),
            ]),
            Line::from(vec![Span::raw("■ ").green(), Span::raw("file")]),
            Line::from(vec![
                Span::raw("■ ").magenta(),
                Span::raw("smaller items together"),
            ]),
        ];
        vec![selected, legend]
    }

    fn open(&mut self, tree: &Tree) {
        let tiles = tiles(tree, self.folder);
        if let Some(&(Tile::Item(id), _)) = tiles.get(self.selected)
            && tree.get(id).kind == NodeKind::Directory
            && !tree.get(id).children.is_empty()
        {
            self.trail.push((self.folder, self.selected));
            self.folder = id;
            self.selected = 0;
        }
    }

    /// Goes up a folder. At the folder Disk had open, goes back to Disk.
    fn up(&mut self) -> Action {
        match self.trail.pop() {
            Some((folder, selected)) => {
                self.folder = folder;
                self.selected = selected;
                Action::None
            }
            None => Action::Back,
        }
    }
}

impl Screen for Treemap {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let ScanStatus::Done { scan, .. } = context.scan else {
            return;
        };
        let tree = &scan.tree;
        let tiles = tiles(tree, self.folder);
        self.selected = self.selected.min(tiles.len().saturating_sub(1));
        if area.width >= SIDE_WIDTH {
            let [map, side] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDE)]).areas(area);
            self.draw_map(frame, map, tree, &tiles);
            visual::sections(frame, side, "", self.details(tree, &tiles));
        } else {
            let [map, below] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(4)]).areas(area);
            self.draw_map(frame, map, tree, &tiles);
            let lines = tiles
                .get(self.selected)
                .map_or_else(Vec::new, |&(tile, size)| {
                    vec![Line::from(vec![
                        Span::raw(tile_name(tree, tile)).bold(),
                        Span::raw("  "),
                        format::size_span(size, format::size(size)),
                    ])]
                });
            frame.render_widget(
                Paragraph::new(lines).block(
                    visual::block()
                        .title(" Selected ")
                        .padding(ratatui::widgets::Padding::horizontal(1)),
                ),
                below,
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, context: &Context) -> Action {
        let ScanStatus::Done { scan, .. } = context.scan else {
            return Action::None;
        };
        let tree = &scan.tree;
        let count = tiles(tree, self.folder).len();
        match key.code {
            KeyCode::Right | KeyCode::Down | KeyCode::Char('l' | 'j') | KeyCode::Tab => {
                if count > 0 {
                    self.selected = (self.selected + 1) % count;
                }
            }
            KeyCode::Left | KeyCode::Up | KeyCode::Char('h' | 'k') | KeyCode::BackTab => {
                if count > 0 {
                    self.selected = (self.selected + count - 1) % count;
                }
            }
            KeyCode::Enter => self.open(tree),
            KeyCode::Backspace => return self.up(),
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        self.up()
    }

    fn hints(&self) -> &'static str {
        "←→ move · enter open · backspace or esc up · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("← → ↑ ↓", "Move between boxes, largest first"),
            ("Enter", "Open the selected folder"),
            ("Backspace  Esc", "Go up a folder, then back to Disk"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neet_core::scan::Scan;
    use std::time::Duration;

    #[test]
    fn boxes_fill_the_area_without_gaps_or_overlap() {
        let area = Rect::new(2, 3, 80, 24);
        let sizes = [500, 300, 120, 50, 20, 7, 3];

        let rects = squarify(&sizes, area);

        let mut covered = vec![0u8; 80 * 24];
        for rect in &rects {
            assert!(rect.x >= area.x && rect.right() <= area.right(), "{rect:?}");
            assert!(
                rect.y >= area.y && rect.bottom() <= area.bottom(),
                "{rect:?}"
            );
            for y in rect.top()..rect.bottom() {
                for x in rect.left()..rect.right() {
                    covered[usize::from(y - area.y) * 80 + usize::from(x - area.x)] += 1;
                }
            }
        }
        assert!(covered.iter().all(|&count| count == 1), "every cell once");
        // The largest box takes about its share.
        let largest = u32::from(rects[0].width) * u32::from(rects[0].height);
        let share = f64::from(largest) / (80.0 * 24.0);
        assert!((share - 0.5).abs() < 0.08, "{share}");
    }

    #[test]
    fn empty_and_zero_inputs_draw_nothing() {
        assert!(squarify(&[], Rect::new(0, 0, 10, 10)).is_empty());
        assert_eq!(
            squarify(&[0, 0], Rect::new(0, 0, 10, 10)),
            vec![Rect::default(); 2]
        );
        assert_eq!(
            squarify(&[5], Rect::new(0, 0, 0, 10)),
            vec![Rect::default()]
        );
        assert_eq!(
            squarify(&[5], Rect::new(0, 0, 10, 4)),
            vec![Rect::new(0, 0, 10, 4)]
        );
    }

    /// ~/big/ (900, with inner/ 600), ~/notes.txt (100), 30 tiny files
    fn sample() -> (Tree, NodeId) {
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let big = tree.add(root, "big", NodeKind::Directory, 0);
        let inner = tree.add(big, "inner", NodeKind::Directory, 0);
        let _ = tree.add(inner, "a.bin", NodeKind::File, 600);
        let _ = tree.add(big, "b.bin", NodeKind::File, 300);
        let _ = tree.add(root, "notes.txt", NodeKind::File, 100);
        for index in 0..30 {
            let _ = tree.add(root, format!("tiny{index}"), NodeKind::File, 1);
        }
        let _ = tree.add(root, "empty", NodeKind::File, 0);
        (tree, big)
    }

    fn done(tree: Tree) -> ScanStatus {
        ScanStatus::Done {
            scan: Scan {
                tree,
                errors: Vec::new(),
                other_disks: Vec::new(),
            },
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn many_small_items_share_one_box_and_empty_ones_are_left_out() {
        let (tree, _) = sample();

        let tiles = tiles(&tree, tree.root());

        assert_eq!(tiles.len(), MAX_BOXES + 1);
        assert_eq!(tiles[0].1, 900);
        assert_eq!(tiles.last().unwrap().0, Tile::Rest(32 - MAX_BOXES));
    }

    fn render(screen: &mut Treemap, scan: &ScanStatus, width: u16, height: u16) -> String {
        let context = Context {
            scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let buffer = visual::tests::render("treemap", screen, &context, width, height);
        visual::tests::text(&buffer)
    }

    fn press(screen: &mut Treemap, scan: &ScanStatus, code: KeyCode) -> Action {
        let context = Context {
            scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        screen.handle_key(
            KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE),
            &context,
        )
    }

    #[test]
    fn opens_folders_and_goes_back_up_to_disk() {
        let (tree, big) = sample();
        let root = tree.root();
        let scan = done(tree);
        let mut screen = Treemap::new(root);

        let text = render(&mut screen, &scan, 120, 30);
        assert!(text.contains("big/"), "{text}");
        assert!(
            text.contains("notes.txt") || text.contains("note"),
            "{text}"
        );
        assert!(text.contains("Enter opens it"), "{text}");

        press(&mut screen, &scan, KeyCode::Enter);
        assert_eq!(screen.folder, big);
        let text = render(&mut screen, &scan, 120, 30);
        assert!(text.contains("inner/"), "{text}");

        // A file does not open.
        press(&mut screen, &scan, KeyCode::Right);
        press(&mut screen, &scan, KeyCode::Enter);
        assert_eq!(screen.folder, big);

        assert!(matches!(screen.back(), Action::None));
        assert_eq!(screen.folder, root);
        assert_eq!(screen.selected, 0, "the box opened stays selected");
        assert!(matches!(screen.back(), Action::Back));
    }

    #[test]
    fn fits_every_screen_size_and_an_empty_folder() {
        for (width, height) in visual::tests::SIZES {
            let (tree, _) = sample();
            let root = tree.root();
            let scan = done(tree);
            let text = render(&mut Treemap::new(root), &scan, width, height);
            assert!(text.contains("big/"), "{width}x{height}\n{text}");
        }
        let mut tree = Tree::new("/Users/test");
        let empty = tree.add(tree.root(), "empty", NodeKind::Directory, 0);
        let scan = done(tree);
        assert!(render(&mut Treemap::new(empty), &scan, 80, 24).contains("Nothing here"));
    }
}
