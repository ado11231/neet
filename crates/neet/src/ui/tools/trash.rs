//! The Trash: what is in it, largest first, and emptying it through Finder
//! after a red question. Emptying is the one thing neet deletes for good
//! itself. See Emptying the Trash in `docs/SAFETY.md`.

use std::path::PathBuf;
use std::time::Instant;

use neet_core::tree::{NodeId, NodeKind, Tree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Row, Table, TableState};

use super::super::app::{Action, Context, Screen};
use super::super::format;
use super::super::loading::Loading;
use super::{Stage, ask, back_home, cross, key, labeled, message, spawn, step, tick};

/// How wide the bar beside each item is
const BAR: u16 = 10;

/// From this width the boxes sit beside the list
const SIDE_WIDTH: u16 = 100;

/// One item at the top of the Trash
pub struct Entry {
    name: String,
    folder: bool,
    size: u64,
    items: u64,
}

/// What is in the Trash, from the scan
pub struct InTrash {
    entries: Vec<Entry>,
}

impl InTrash {
    fn size(&self) -> u64 {
        self.entries.iter().map(|entry| entry.size).sum()
    }

    fn items(&self) -> u64 {
        self.entries.iter().map(|entry| entry.items.max(1)).sum()
    }
}

/// The Trash screen
pub struct Trash {
    /// The Trash folder, to open in Finder
    path: PathBuf,
    /// macOS would not let the scan read it
    blocked: bool,
    stage: Stage<InTrash, Result<u64, String>>,
    table: TableState,
}

impl Trash {
    /// What the scan found in `trash`, largest first. `blocked` when macOS
    /// would not let it be read.
    pub fn new(tree: &Tree, trash: Option<NodeId>, path: PathBuf, blocked: bool) -> Self {
        let mut entries: Vec<Entry> = trash
            .map(|id| {
                tree.get(id)
                    .children
                    .iter()
                    .map(|&child| {
                        let node = tree.get(child);
                        Entry {
                            name: node.name.to_string_lossy().into_owned(),
                            folder: node.kind == NodeKind::Directory,
                            size: node.total_size,
                            items: node.total_items,
                        }
                    })
                    // Finder's own bookkeeping file is not something you put there.
                    .filter(|entry| entry.name != ".DS_Store")
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        Self {
            path,
            blocked,
            stage: Stage::Ready(InTrash { entries }),
            table: TableState::default().with_selected(Some(0)),
        }
    }

    fn start_emptying(&mut self) {
        let size = self.stage.found().map_or(0, InTrash::size);
        self.stage = Stage::Working(
            Instant::now(),
            spawn(move || {
                neet_core::trash::empty()
                    .map(|()| size)
                    .map_err(|error| error.to_string())
            }),
        );
    }

    /// Opens the Trash in Finder, where Put Back works
    fn open_in_finder(&self) {
        // Finder opens on its own; neet does not wait for it.
        let _ = std::process::Command::new("/usr/bin/open")
            .arg(&self.path)
            .spawn();
    }

    fn draw_list(table: &mut TableState, frame: &mut Frame, area: Rect, found: &InTrash) {
        let largest = found.entries.first().map_or(0, |entry| entry.size);
        let sizes = format::column_width(
            "Size",
            found.entries.iter().map(|entry| format::size(entry.size)),
        );
        let items = format::column_width(
            "Items",
            found.entries.iter().map(|entry| format::count(entry.items)),
        );
        let name_width = usize::from(area.width.saturating_sub(sizes + BAR + items + 6 * 2 + 6));
        let rows: Vec<Row> = found
            .entries
            .iter()
            .map(|entry| {
                let name = format::shorten_middle(&entry.name, name_width.saturating_sub(1));
                let name = if entry.folder {
                    Span::raw(format!("{name}/")).fg(super::super::visual::ACCENT)
                } else {
                    Span::raw(name)
                };
                Row::new([
                    Cell::from(
                        Line::from(format::size_span(entry.size, format::size(entry.size)))
                            .right_aligned(),
                    ),
                    Cell::from(format::size_bar(entry.size, largest, usize::from(BAR))),
                    Cell::from(Line::from(format::count(entry.items.max(1))).right_aligned()),
                    Cell::from(name),
                ])
            })
            .collect();
        let header = Row::new([
            Cell::from(Line::from("Size").right_aligned()),
            Cell::from(""),
            Cell::from(Line::from("Items").right_aligned()),
            Cell::from("Name"),
        ])
        .style(super::super::visual::HEADING)
        .bottom_margin(1);
        let widget = Table::new(
            rows,
            [
                Constraint::Length(sizes),
                Constraint::Length(BAR),
                Constraint::Length(items),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(
            super::super::visual::block()
                .title(format!(
                    " Trash · {} · {} ",
                    format::size(found.size()),
                    super::counted(found.entries.len(), "item")
                ))
                .padding(Padding::horizontal(1)),
        )
        .highlight_symbol("▸ ")
        .row_highlight_style(super::super::visual::SELECTED);
        frame.render_stateful_widget(widget, area, table);
    }

    /// The boxes beside the list: the total, the warning, the steps, and
    /// the disk after
    fn boxes(found: &InTrash, context: &Context, width: u16) -> Vec<Vec<Line<'static>>> {
        let field =
            |label: &'static str, value: Span<'static>| labeled(label, Color::Reset, vec![value]);
        let total = vec![
            field(
                "Size",
                format::size_span(found.size(), format::size(found.size())).bold(),
            ),
            field(
                "Items",
                Span::raw(super::counted(found.entries.len(), "item")),
            ),
            field(
                "Files",
                Span::raw(format!(
                    "{} in all, in folders too",
                    format::count(found.items())
                )),
            ),
        ];
        let warning = vec![
            Line::from("Warning").style(super::super::visual::HEADING),
            Line::from("Emptying deletes everything here for good.").red(),
            Line::from("Put Back stops working once it is empty."),
        ];
        let steps = vec![
            Line::from("Steps").style(super::super::visual::HEADING),
            step(1, vec![Span::raw("Look over what is here.")]),
            step(
                2,
                vec![
                    Span::raw("Press "),
                    key("e"),
                    Span::raw(" to empty the Trash."),
                ],
            ),
            step(
                3,
                vec![Span::raw("Press "), key("y"), Span::raw(" to confirm.")],
            ),
            Line::default(),
            Line::from(vec![
                Span::raw("To put something back, press "),
                key("o"),
                Span::raw(" to open the Trash in Finder."),
            ]),
        ];
        let after = super::super::visual::after_cleanup(
            context.disk.as_ref(),
            found.size(),
            "Emptying",
            width,
        );
        vec![total, warning, steps, after]
    }

    fn question(&self) -> Vec<Line<'static>> {
        let found = self.stage.found();
        let size = found.map_or(0, InTrash::size);
        let items = found.map_or(0, |found| found.entries.len());
        vec![
            Line::from(vec![
                Span::raw("Delete everything in the Trash: "),
                Span::raw(format::size(size)).red().bold(),
                Span::raw(format!(", {}.", super::counted(items, "item"))),
            ]),
            Line::default(),
            Line::from("This can't be undone. Put Back stops working.").red(),
            Line::from("Finder empties it, the same as its Empty Trash."),
        ]
    }
}

/// What to do when macOS will not let neet read the Trash
fn blocked_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("neet can't see inside the Trash.")
            .yellow()
            .bold(),
        Line::default(),
        Line::from(
            "macOS only lets your terminal read the Trash with Full Disk Access. Turn it on for your terminal app in System Settings, Privacy & Security, Full Disk Access, then restart it.",
        ),
        Line::default(),
        Line::from(vec![
            Span::raw("Press "),
            key("o"),
            Span::raw(" to open the Trash in Finder instead."),
        ]),
    ]
}

impl Screen for Trash {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        self.stage.poll();
        let title = "Trash";
        if self.blocked && matches!(self.stage, Stage::Ready(_)) {
            return message(frame, area, title, blocked_lines(), Color::Yellow);
        }
        match &self.stage {
            Stage::Working(started, _) => {
                return Loading {
                    title,
                    doing: "Emptying the Trash",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Finder deletes every item. A big Trash can take a minute.",
                }
                .draw(frame, area);
            }
            Stage::Done(Ok(freed)) => {
                return message(
                    frame,
                    area,
                    title,
                    vec![
                        tick("The Trash is empty".to_string()).bold(),
                        Line::default(),
                        Line::from(format!("About {} is free again.", format::size(*freed))),
                        Line::from("Other screens show the old numbers until neet scans again."),
                        Line::default(),
                        back_home(),
                    ],
                    Color::Green,
                );
            }
            Stage::Done(Err(reason)) => {
                return message(
                    frame,
                    area,
                    title,
                    vec![cross(reason.clone()), Line::default(), back_home()],
                    Color::Red,
                );
            }
            Stage::Ready(found) | Stage::Asking(found) if found.entries.is_empty() => {
                return message(
                    frame,
                    area,
                    title,
                    vec![
                        tick("The Trash is empty".to_string()).bold(),
                        Line::default(),
                        Line::from("There is nothing to delete."),
                    ],
                    Color::Green,
                );
            }
            _ => {}
        }
        let (Stage::Ready(found) | Stage::Asking(found)) = &self.stage else {
            return;
        };
        let rows = u16::try_from(found.entries.len()).unwrap_or(u16::MAX);
        let boxes = Self::boxes(found, context, area.width / 2);
        let (list, side) = if area.width >= SIDE_WIDTH {
            let [list, side] =
                Layout::horizontal([Constraint::Percentage(58), Constraint::Fill(1)]).areas(area);
            (list, side)
        } else {
            let [list, below] = Layout::vertical([
                Constraint::Length((rows + 4).min(area.height / 2)),
                Constraint::Fill(1),
            ])
            .areas(area);
            (list, below)
        };
        Self::draw_list(&mut self.table, frame, list, found);
        super::super::visual::sections(frame, side, "Total", boxes);
        if matches!(self.stage, Stage::Asking(_)) {
            ask(
                frame,
                area,
                "Empty the Trash",
                self.question(),
                "empty it for good",
                Color::Red,
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match &self.stage {
            Stage::Asking(_) => {
                match key.code {
                    KeyCode::Char('y') => self.start_emptying(),
                    KeyCode::Char('n') => self.stage.back_to_ready(),
                    _ => {}
                }
                return Action::None;
            }
            Stage::Done(_) => {
                return if key.code == KeyCode::Enter {
                    Action::Home
                } else {
                    Action::None
                };
            }
            Stage::Ready(_) => {}
            _ => return Action::None,
        }
        if key.code == KeyCode::Char('o') {
            self.open_in_finder();
            return Action::None;
        }
        let Some(found) = self.stage.found() else {
            return Action::None;
        };
        let last = found.entries.len().saturating_sub(1);
        let index = self.table.selected().unwrap_or(0);
        let empty = found.entries.is_empty();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.table.select(Some(index.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => self.table.select(Some((index + 1).min(last))),
            KeyCode::Char('g') | KeyCode::Home => self.table.select(Some(0)),
            KeyCode::Char('G') | KeyCode::End => self.table.select(Some(last)),
            KeyCode::Char('e') if !empty && !self.blocked => self.stage.ask(),
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        if matches!(self.stage, Stage::Asking(_)) {
            self.stage.back_to_ready();
            return Action::None;
        }
        Action::Back
    }

    fn is_dialog(&self) -> bool {
        self.stage.is_busy()
    }

    fn hints(&self) -> &'static str {
        match self.stage {
            Stage::Asking(_) => "y empty for good · n or esc go back",
            Stage::Done(_) => "enter home",
            _ => "↑↓ move · e empty the Trash · o open in Finder · esc back · ? help",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between items"),
            ("e", "Empty the Trash, after a question"),
            ("y", "In the question: empty it for good"),
            ("o", "Open the Trash in Finder, to put things back"),
            ("Esc", "Go back"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use crate::ui::visual::tests as view;
    use ratatui::crossterm::event::KeyModifiers;

    fn screen(blocked: bool) -> Trash {
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let trash = tree.add(root, ".Trash", NodeKind::Directory, 0);
        let folder = tree.add(trash, "old project", NodeKind::Directory, 0);
        let _ = tree.add(folder, "a.bin", NodeKind::File, 3_000_000);
        let _ = tree.add(trash, "notes.txt", NodeKind::File, 5_000);
        let _ = tree.add(trash, ".DS_Store", NodeKind::File, 6_000);
        Trash::new(
            &tree,
            Some(trash),
            PathBuf::from("/Users/test/.Trash"),
            blocked,
        )
    }

    fn render(trash: &mut Trash) -> String {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        view::text(&view::render("trash", trash, &context, 140, 34))
    }

    fn press(trash: &mut Trash, code: KeyCode) {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        trash.handle_key(KeyEvent::new(code, KeyModifiers::NONE), &context);
    }

    #[test]
    fn lists_what_is_in_the_trash_largest_first() {
        let mut trash = screen(false);
        let text = render(&mut trash);

        assert!(text.contains("old project/"), "{text}");
        assert!(text.contains("notes.txt"));
        assert!(!text.contains(".DS_Store"));
        assert!(text.find("old project") < text.find("notes.txt"));
        assert!(text.contains("Emptying deletes everything here for good."));
    }

    #[test]
    fn e_asks_first_and_n_goes_back() {
        let mut trash = screen(false);
        press(&mut trash, KeyCode::Char('e'));
        let text = render(&mut trash);
        assert!(text.contains("Empty the Trash"), "{text}");
        assert!(text.contains("This can't be undone."));
        assert!(trash.is_dialog());

        press(&mut trash, KeyCode::Char('n'));
        assert!(!trash.is_dialog());
        assert!(matches!(trash.back(), Action::Back));
    }

    #[test]
    fn a_blocked_trash_says_how_to_allow_it_and_cannot_be_emptied() {
        let mut trash = screen(true);
        let text = render(&mut trash);
        assert!(text.contains("Full Disk Access"), "{text}");
        press(&mut trash, KeyCode::Char('e'));
        assert!(!trash.is_dialog());
    }
}
