use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::SystemTime;

use neet_core::apps;
use neet_core::clean::{self, Outcome};
use neet_core::safety::CleanupRoots;
use neet_core::trash;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::clean::{Planned, app_name, count, display_path, items, skip_reason, tier_span};
use super::format;
use super::loading::Loading;

/// A scrolling position that stops at the last line once drawn.
#[derive(Default)]
struct Scroll(u16);

impl Scroll {
    fn handle(&mut self, code: KeyCode) {
        self.0 = match code {
            KeyCode::Up | KeyCode::Char('k') => self.0.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.0.saturating_add(1),
            KeyCode::PageUp => self.0.saturating_sub(10),
            KeyCode::PageDown => self.0.saturating_add(10),
            KeyCode::Char('g') | KeyCode::Home => 0,
            KeyCode::Char('G') | KeyCode::End => u16::MAX,
            _ => self.0,
        };
    }

    fn clamp(&mut self, lines: usize, area: Rect) -> u16 {
        let visible = area.height.saturating_sub(2);
        let last = u16::try_from(lines)
            .unwrap_or(u16::MAX)
            .saturating_sub(visible);
        self.0 = self.0.min(last);
        self.0
    }
}

const SCROLL_HELP: [(&str, &str); 3] = [
    ("↑ ↓  j k", "Scroll"),
    ("PgUp PgDn", "Scroll a page"),
    ("g  G", "Jump to the top or bottom"),
];

/// Every path the selected rules found. It can only be left by going back,
/// or by moving on to the question.
pub struct Review {
    planned: Arc<Planned>,
    scroll: Scroll,
}

impl Review {
    pub fn new(planned: Arc<Planned>) -> Self {
        Self {
            planned,
            scroll: Scroll::default(),
        }
    }

    /// One group per name, so the files app removal finds in the same
    /// folder are listed together. The largest group first, and in each
    /// the largest item first.
    fn groups(&self) -> Vec<Group<'_>> {
        let mut groups: Vec<Group<'_>> = Vec::new();
        // A selected rule that found nothing has nothing to review.
        let rules = self.planned.plan.rules.iter();
        for rule in rules.filter(|rule| rule.selected && !rule.items.is_empty()) {
            match groups.iter_mut().find(|group| group.name == rule.rule.name) {
                Some(group) => group.items.extend(&rule.items),
                None => groups.push(Group {
                    name: &rule.rule.name,
                    items: rule.items.iter().collect(),
                }),
            }
        }
        for group in &mut groups {
            group.items.sort_by_key(|item| std::cmp::Reverse(item.size));
        }
        groups.sort_by_key(|group| std::cmp::Reverse(group.size()));
        groups
    }

    /// The header, then each group: its name and total, the folder its
    /// items share, and a row an item with its size, bar, and age. Items
    /// in one folder show by name, so long paths are not cut.
    fn table(&self, width: u16) -> (Line<'static>, Vec<Line<'static>>) {
        let home = &self.planned.home;
        let groups = self.groups();
        let largest = groups
            .iter()
            .flat_map(|group| group.items.iter().map(|item| item.size))
            .max()
            .unwrap_or(0);
        // The age goes first when room is short, then the bar.
        let width = usize::from(width);
        let age = if width >= 64 { AGE_WIDTH + 2 } else { 0 };
        let bar = if width >= 52 { ITEM_BAR + 2 } else { 0 };
        // Only as wide as the longest name, so sizes sit beside the names
        let longest = groups
            .iter()
            .flat_map(|group| {
                let shared = group.folder().is_some();
                group.items.iter().map(move |item| {
                    let path = item.path.path();
                    match (shared, path.file_name()) {
                        (true, Some(name)) => name.to_string_lossy().chars().count(),
                        _ => display_path(home, path).chars().count(),
                    }
                })
            })
            .max()
            .unwrap_or(0);
        let name_width = longest
            .max(24)
            .min(width.saturating_sub(2 + 2 + 9 + bar + age))
            .max(8);
        let mut header = format!("  {:<name_width$}  {:>9}", "Name", "Size");
        if bar > 0 {
            header.push_str(&" ".repeat(bar));
        }
        if age > 0 {
            header.push_str("  Changed");
        }
        let header = Line::from(header).style(super::visual::HEADING);
        let now = SystemTime::now();
        let mut rows = Vec::new();
        for (number, group) in groups.iter().enumerate() {
            if number > 0 {
                rows.push(Line::default());
            }
            let size = group.size();
            rows.push(Line::from(vec![
                Span::raw(group.name.to_string())
                    .bold()
                    .fg(super::visual::ACCENT),
                Span::raw(format!(" · {} · ", items(group.items.len()))),
                format::size_span(size, format::size(size)).bold(),
            ]));
            let folder = group.folder();
            if let Some(folder) = folder {
                rows.push(Line::from(format::shorten_path(
                    &display_path(home, folder),
                    width,
                )));
            }
            for item in &group.items {
                let path = item.path.path();
                let name = match (folder, path.file_name()) {
                    (Some(_), Some(name)) => name.to_string_lossy().into_owned(),
                    _ => display_path(home, path),
                };
                let name = format::shorten_middle(&name, name_width);
                let mut spans = vec![
                    Span::raw(format!("  {name:<name_width$}  ")),
                    format::size_span(item.size, format!("{:>9}", format::size(item.size))),
                ];
                if bar > 0 {
                    spans.push(Span::raw("  "));
                    spans.push(
                        Span::raw(format::bar(item.size, largest, ITEM_BAR))
                            .fg(super::visual::ACCENT),
                    );
                }
                if age > 0 {
                    let changed = now.duration_since(item.changed).unwrap_or_default();
                    spans.push(Span::raw(format!("  {}", format::age_aligned(changed))));
                }
                rows.push(Line::from(spans));
            }
        }
        (header, rows)
    }

    /// What each selected rule removes, its level, and the app to close
    fn about(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut seen: Vec<(&str, &str)> = Vec::new();
        let rules = self.planned.plan.rules.iter();
        for rule in rules.filter(|rule| rule.selected && !rule.items.is_empty()) {
            let rule = &rule.rule;
            // Rules that share a name and say the same thing show once.
            if seen.contains(&(rule.name.as_str(), rule.description.as_str())) {
                continue;
            }
            seen.push((&rule.name, &rule.description));
            if !lines.is_empty() {
                lines.push(Line::default());
            }
            lines.push(Line::from(vec![
                Span::raw(rule.name.clone()).bold(),
                Span::raw("  "),
                tier_span(rule.tier),
            ]));
            if !rule.description.is_empty() {
                lines.push(Line::from(rule.description.clone()));
            }
            if !rule.requires_quit.is_empty() {
                let apps: Vec<String> = rule
                    .requires_quit
                    .iter()
                    .map(|bundle| app_name(bundle))
                    .collect();
                lines.push(Line::from(vec![
                    Span::raw("Close first  ").bold(),
                    Span::raw(apps.join(", ")).yellow(),
                ]));
            }
        }
        lines
    }

    /// What the selected rules found but will not move, and why
    fn kept(&self) -> Vec<Line<'static>> {
        let home = &self.planned.home;
        self.planned
            .plan
            .rules
            .iter()
            .filter(|rule| rule.selected)
            .flat_map(|rule| {
                rule.skipped
                    .iter()
                    .map(move |skipped| (skipped, rule.rule.min_age_days))
            })
            .map(|(skipped, days)| {
                Line::from(vec![
                    Span::raw(display_path(home, &skipped.path)),
                    Span::raw("  "),
                    Span::raw(skip_reason(&skipped.reason, Some(days))).yellow(),
                ])
            })
            .collect()
    }

    /// The total, with each group's share
    fn totals(&self) -> Vec<Line<'static>> {
        let plan = &self.planned.plan;
        let total = plan.selected_size();
        let groups: Vec<(&str, u64)> = self
            .groups()
            .iter()
            .map(|group| (group.name, group.size()))
            .collect();
        let field = |label: &str, value: Span<'static>| {
            Line::from(vec![Span::raw(format!("{label:<11}")).bold(), value])
        };
        let mut lines = vec![
            field("Size", Span::raw(format::size(total)).green().bold()),
            field("Items", Span::raw(count(plan.selected_count()))),
            field("Goes to", Span::raw("Trash · Put Back undoes it").green()),
            field("Next", Span::raw("Enter, then y to confirm")),
        ];
        // One group's share is the whole total, so its bar says nothing.
        if groups.len() > 1 {
            lines.push(Line::default());
        }
        let several = groups.len() > 1;
        for (name, size) in groups.into_iter().filter(|_| several) {
            lines.push(Line::from(vec![
                format::size_span(size, format!("{:>9}  ", format::size(size))),
                Span::raw(format::bar(size, total, SUMMARY_BAR)).fg(super::visual::ACCENT),
                Span::raw(format!("  {name}")),
            ]));
        }
        lines
    }

    /// The table in `area`: the header stays, the groups scroll under it
    fn draw_table(&mut self, frame: &mut Frame, area: Rect) {
        let plan = &self.planned.plan;
        let block = super::visual::block()
            .title(format!(
                " Review · {} · {} ",
                items(plan.selected_count()),
                format::size(plan.selected_size())
            ))
            .title(Line::from(" Goes to the Trash ").green().right_aligned())
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        let (header, rows) = self.table(inner.width);
        let [head, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
        let offset = self.scroll.clamp(
            rows.len(),
            Rect {
                height: body.height + 2,
                ..body
            },
        );
        let total = rows.len();
        let last = (usize::from(offset) + usize::from(body.height)).min(total);
        // Where you are, only when the list is longer than the box
        let block = if total > usize::from(body.height) {
            block.title_bottom(
                Line::from(format!(" {}–{last} of {total} · ↑↓ scroll ", offset + 1))
                    .right_aligned(),
            )
        } else {
            block
        };
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(header), head);
        frame.render_widget(Paragraph::new(rows).scroll((offset, 0)), body);
    }
}

/// The items of the rules that share a name
struct Group<'a> {
    name: &'a str,
    items: Vec<&'a clean::PlanItem>,
}

impl Group<'_> {
    fn size(&self) -> u64 {
        self.items.iter().map(|item| item.size).sum()
    }

    /// The folder every item is in, when they share one
    fn folder(&self) -> Option<&Path> {
        let folder = self.items.first()?.path.path().parent()?;
        self.items
            .iter()
            .all(|item| item.path.path().parent() == Some(folder))
            .then_some(folder)
    }
}

/// How wide the age beside each item is, such as `11 months ago`
const AGE_WIDTH: usize = 13;

/// How wide the bar beside each path is
const ITEM_BAR: usize = 10;

/// How wide the bars in the review summary are.
const SUMMARY_BAR: usize = 10;

/// Below this width the summary goes away and the paths take the screen.
const MIN_SUMMARY_WIDTH: u16 = 100;

/// From this width the summary sits beside the paths, not under them
const MIN_SIDE_WIDTH: u16 = 140;

impl Screen for Review {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        let titled = |title: String| {
            super::visual::block()
                .title(title)
                .padding(Padding::horizontal(1))
        };
        let totals = self.totals();
        let freed = self.planned.plan.selected_size();
        if area.width < MIN_SIDE_WIDTH {
            // The table on top, the total and the disk after side by side
            // under it, when there is room
            let half = area.width / 2;
            let width = half.saturating_sub(4);
            let mut after =
                super::visual::after_cleanup(context.disk.as_ref(), freed, "Cleanup", width);
            after.remove(0);
            let height = |lines: &[Line<'_>]| super::visual::wrapped_rows(lines, width) + 2;
            let bottom = height(&totals).max(height(&after));
            if area.width < MIN_SUMMARY_WIDTH || area.height < bottom + 8 {
                self.draw_table(frame, area);
                return;
            }
            let [paths, under] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(bottom)]).areas(area);
            self.draw_table(frame, paths);
            let [left, right] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(under);
            frame.render_widget(
                Paragraph::new(totals)
                    .wrap(Wrap { trim: false })
                    .block(titled(" Total ".to_string())),
                left,
            );
            frame.render_widget(
                Paragraph::new(after)
                    .wrap(Wrap { trim: false })
                    .block(titled(" After cleanup ".to_string())),
                right,
            );
            return;
        }
        // The table on the left. On the right, the total and the disk after
        // at their own heights, and what stays filling the rest.
        let [paths, side] =
            Layout::horizontal([Constraint::Percentage(62), Constraint::Fill(1)]).areas(area);
        self.draw_table(frame, paths);
        let width = side.width.saturating_sub(4);
        let mut after =
            super::visual::after_cleanup(context.disk.as_ref(), freed, "Cleanup", width);
        after.remove(0);
        let kept = self.kept();
        let about = self.about();
        let height = |lines: &[Line<'_>]| super::visual::wrapped_rows(lines, width) + 2;
        // What is removed fills the rest. What stays shows only when
        // something does, and then takes the rest.
        let [total_area, after_area, about_area, kept_area] = Layout::vertical([
            Constraint::Length(height(&totals)),
            Constraint::Length(height(&after)),
            if kept.is_empty() {
                Constraint::Fill(1)
            } else {
                Constraint::Length(height(&about))
            },
            if kept.is_empty() {
                Constraint::Length(0)
            } else {
                Constraint::Fill(1)
            },
        ])
        .areas(side);
        frame.render_widget(
            Paragraph::new(totals)
                .wrap(Wrap { trim: false })
                .block(titled(" Total ".to_string())),
            total_area,
        );
        frame.render_widget(
            Paragraph::new(after)
                .wrap(Wrap { trim: false })
                .block(titled(" After cleanup ".to_string())),
            after_area,
        );
        frame.render_widget(
            Paragraph::new(about)
                .wrap(Wrap { trim: false })
                .block(titled(" What goes ".to_string())),
            about_area,
        );
        if !kept.is_empty() {
            let title = format!(" Kept · {} ", count(kept.len()));
            frame.render_widget(
                Paragraph::new(kept)
                    .wrap(Wrap { trim: false })
                    .block(titled(title)),
                kept_area,
            );
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if key.code == KeyCode::Enter {
            return Action::Open(Box::new(Confirm::new(Arc::clone(&self.planned))));
        }
        self.scroll.handle(key.code);
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ scroll · enter continue · esc back · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            SCROLL_HELP[0],
            SCROLL_HELP[1],
            SCROLL_HELP[2],
            ("Enter", "Move the items to the Trash"),
            ("Esc", "Back to Deep Clean"),
            ("q", "Quit"),
        ]
    }
}

/// The question before anything moves.
pub struct Confirm {
    planned: Arc<Planned>,
}

impl Confirm {
    pub fn new(planned: Arc<Planned>) -> Self {
        Self { planned }
    }
}

impl Screen for Confirm {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        let plan = &self.planned.plan;
        let lines = vec![
            Line::from(format!(
                "Move {}, {}, to the Trash?",
                items(plan.selected_count()),
                format::size(plan.selected_size())
            ))
            .bold(),
            Line::default(),
            Line::from("Each item is checked again first."),
            Line::from("Put Back in the Trash undoes it."),
            Line::default(),
            Line::from(vec![
                Span::raw("y").bold().fg(super::visual::ACCENT),
                Span::raw(" move to the Trash    "),
                Span::raw("n").bold(),
                Span::raw(" or "),
                Span::raw("Esc").bold(),
                Span::raw(" go back"),
            ]),
        ];
        let [area] = Layout::vertical([Constraint::Length(
            super::visual::wrapped_rows(&lines, area.width.min(60).saturating_sub(4))
                .saturating_add(2),
        )])
        .flex(Flex::Center)
        .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(60)])
            .flex(Flex::Center)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                super::visual::block()
                    .title(" Move to the Trash ")
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Char('y') => Action::Open(Box::new(Cleanup::start(Arc::clone(&self.planned)))),
            KeyCode::Char('n') => Action::Back,
            _ => Action::None,
        }
    }

    fn hints(&self) -> &'static str {
        "y move to the Trash · n or esc back"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("y", "Move the items to the Trash"),
            ("n  Esc", "Back to the review"),
        ]
    }

    fn is_dialog(&self) -> bool {
        true
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

enum Message {
    Progress(usize, usize),
    Done(Result<Outcome, String>),
}

enum State {
    Moving {
        done: usize,
        total: usize,
        messages: Receiver<Message>,
    },
    Done(Result<Outcome, String>),
}

/// Moves the items on its own thread, then shows what happened.
pub struct Cleanup {
    planned: Arc<Planned>,
    state: State,
    scroll: Scroll,
}

impl Cleanup {
    pub fn start(planned: Arc<Planned>) -> Self {
        Self::start_with(planned, apps::is_running, trash::move_to_trash)
    }

    fn start_with(
        planned: Arc<Planned>,
        is_running: fn(&str) -> bool,
        move_item: fn(&Path) -> io::Result<()>,
    ) -> Self {
        let (sender, messages) = mpsc::channel();
        let shared = Arc::clone(&planned);
        thread::spawn(move || {
            let result = CleanupRoots::new(&shared.home)
                .map_err(|error| format!("The home folder could not be read: {error}"))
                .map(|roots| {
                    clean::run(
                        &shared.plan,
                        &roots,
                        SystemTime::now(),
                        is_running,
                        move_item,
                        |done, total| {
                            let _ = sender.send(Message::Progress(done, total));
                        },
                    )
                });
            let _ = sender.send(Message::Done(result));
        });
        Self {
            state: State::Moving {
                done: 0,
                total: planned.plan.selected_count(),
                messages,
            },
            planned,
            scroll: Scroll::default(),
        }
    }

    fn poll(&mut self) {
        let State::Moving {
            done,
            total,
            messages,
        } = &mut self.state
        else {
            return;
        };
        let finished = loop {
            match messages.try_recv() {
                Ok(Message::Progress(now, of)) => (*done, *total) = (now, of),
                Ok(Message::Done(result)) => break result,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    break Err(
                        "The task ended without a result. Some items may have moved. \
                         Check the Trash before trying again."
                            .to_string(),
                    );
                }
            }
        };
        self.state = State::Done(finished);
    }

    fn is_moving(&self) -> bool {
        matches!(self.state, State::Moving { .. })
    }

    /// What happened, for the box in the middle of the screen. Paths are
    /// shortened to fit `width`.
    fn result_lines(&self, outcome: &Outcome, width: usize) -> Vec<Line<'static>> {
        let home = &self.planned.home;
        let moved = outcome.moved.len();
        let mut lines = if moved == 0 {
            vec![Line::from("Nothing was moved.").bold().yellow()]
        } else {
            vec![
                Line::from(format!("✓ Moved {} to the Trash", items(moved)))
                    .bold()
                    .green(),
                Line::default(),
                Line::from(vec![
                    Span::raw(format::size(outcome.moved_size())).bold(),
                    Span::raw(" is in the Trash now."),
                ]),
                Line::from("Empty the Trash to free that space."),
                Line::default(),
                Line::from("Put Back in the Trash restores an item."),
            ]
        };
        if !outcome.skipped.is_empty() {
            lines.push(Line::default());
            lines.push(
                Line::from(format!(
                    "Skipped {}, left where they were:",
                    items(outcome.skipped.len())
                ))
                .yellow()
                .bold(),
            );
            for skipped in &outcome.skipped {
                let path = display_path(home, &skipped.path);
                lines.push(Line::from(format!(
                    "  {}",
                    format::shorten_path(&path, width.saturating_sub(2))
                )));
                lines.push(Line::from(format!(
                    "    {}",
                    skip_reason(&skipped.reason, None)
                )));
            }
        }
        lines
    }
}

/// How wide the boxes in the middle of the screen are, at most
const BOX_WIDTH: u16 = 72;

/// `area` shrunk to `width` by `height`, in the middle
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    area
}

impl Screen for Cleanup {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        match &self.state {
            State::Moving { done, total, .. } => {
                let bar = format::bar(*done as u64, (*total).max(1) as u64, 24);
                Loading {
                    title: "Cleanup",
                    doing: "Moving items to the Trash",
                    progress: format!("{bar}  {} of {}", count(*done), count(*total)),
                    note: "Each item is checked again, then Finder moves it. The first \
                           time, macOS asks whether your terminal may control Finder.",
                }
                .draw(frame, area);
            }
            State::Done(Err(reason)) => {
                let lines = vec![
                    Line::from("Cleanup stopped.").bold().red(),
                    Line::default(),
                    Line::from(reason.clone()),
                ];
                let height = super::visual::wrapped_rows(
                    &lines,
                    area.width.min(BOX_WIDTH).saturating_sub(4),
                )
                .saturating_add(4);
                let box_area = centered(area, BOX_WIDTH, height);
                frame.render_widget(Clear, box_area);
                frame.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                        super::visual::block()
                            .title(" Cleanup ")
                            .padding(Padding::uniform(1)),
                    ),
                    box_area,
                );
            }
            State::Done(Ok(outcome)) => {
                // Border and padding take 4 columns and 4 rows.
                let width = BOX_WIDTH.min(area.width);
                let lines = self.result_lines(outcome, usize::from(width.saturating_sub(4)));
                let rows = super::visual::wrapped_rows(&lines, width.saturating_sub(4));
                let wanted = rows.saturating_add(4);
                let box_area = centered(area, width, wanted);
                let offset = self.scroll.clamp(usize::from(rows) + 2, box_area);
                let title = if outcome.moved.is_empty() {
                    Line::from(" Cleanup done ").yellow()
                } else {
                    Line::from(" Cleanup done ").green()
                };
                frame.render_widget(Clear, box_area);
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .scroll((offset, 0))
                        .block(
                            super::visual::block()
                                .title(title)
                                .title_bottom(Line::from(" Enter or Esc for Home ").right_aligned())
                                .padding(Padding::uniform(1)),
                        ),
                    box_area,
                );
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if self.is_moving() {
            return Action::None;
        }
        if key.code == KeyCode::Enter {
            return Action::Home;
        }
        self.scroll.handle(key.code);
        Action::None
    }

    fn back(&mut self) -> Action {
        if self.is_moving() {
            Action::None
        } else {
            Action::Home
        }
    }

    fn hints(&self) -> &'static str {
        if self.is_moving() {
            "Moving to Trash…"
        } else {
            "↑↓ scroll · enter or esc home · q quit"
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            SCROLL_HELP[0],
            SCROLL_HELP[1],
            SCROLL_HELP[2],
            ("Enter  Esc", "Back to Home when done"),
            ("q", "Quit, when done"),
        ]
    }

    /// Blocks `q` until every item is dealt with.
    fn is_dialog(&self) -> bool {
        self.is_moving()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use neet_core::clean::plan;
    use neet_core::rules::{self, Tier};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use std::fs;
    use std::time::{Duration, Instant};
    use tempfile::{TempDir, tempdir};

    fn planned() -> (TempDir, Arc<Planned>) {
        let dir = tempdir().expect("temporary directory should be created");
        for file in [
            "Library/Developer/Xcode/DerivedData/App-abc/out",
            ".npm/_cacache/content-v2/aa",
        ] {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().expect("path should have a parent"))
                .expect("folder should be created");
            fs::write(path, vec![1; 5000]).expect("file should be written");
        }
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        let set = rules::load(&roots, None);
        let plan = plan(&set.rules, &roots, SystemTime::now());
        let planned = Planned {
            plan,
            errors: Vec::new(),
            home: roots.home().to_path_buf(),
        };
        (dir, Arc::new(planned))
    }

    fn render(screen: &mut dyn Screen) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
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

    fn press(screen: &mut dyn Screen, code: KeyCode) -> Action {
        let scan = ScanStatus::Failed(String::new());
        screen.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            &Context {
                scan: &scan,
                disk: None,
                cleanable: None,
                plan: None,
            },
        )
    }

    fn finish(cleanup: &mut Cleanup) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while cleanup.is_moving() {
            assert!(Instant::now() < deadline, "cleanup should finish");
            thread::sleep(Duration::from_millis(5));
            cleanup.poll();
        }
    }

    #[test]
    fn review_lists_every_path_of_the_selected_rules_only() {
        let (_dir, planned) = planned();
        let screen = render(&mut Review::new(planned));

        assert!(screen.contains("Review · 1 item ·"));
        assert!(screen.contains("Xcode build files"));
        assert!(screen.contains("App-abc"));
        assert!(
            screen.contains("~/Library/Developer/Xcode/DerivedData"),
            "{screen}"
        );
        assert!(!screen.contains("npm cache"));
    }

    #[test]
    fn review_leads_to_the_question_and_n_goes_back() {
        let (_dir, planned) = planned();
        let mut review = Review::new(Arc::clone(&planned));

        assert!(matches!(
            press(&mut review, KeyCode::Enter),
            Action::Open(_)
        ));
        let mut confirm = Confirm::new(planned);
        assert!(render(&mut confirm).contains("Move 1 item,"));
        assert!(matches!(
            press(&mut confirm, KeyCode::Char('n')),
            Action::Back
        ));
        assert!(matches!(press(&mut confirm, KeyCode::Enter), Action::None));
        assert!(confirm.is_dialog());
    }

    #[test]
    fn a_missing_cleanup_result_allows_leaving_without_claiming_success() {
        let (_dir, planned) = planned();
        let (sender, messages) = mpsc::channel();
        let mut cleanup = Cleanup {
            planned,
            state: State::Moving {
                done: 0,
                total: 2,
                messages,
            },
            scroll: Scroll::default(),
        };
        sender.send(Message::Progress(1, 2)).unwrap();
        cleanup.poll();
        assert!(matches!(
            cleanup.state,
            State::Moving {
                done: 1,
                total: 2,
                ..
            }
        ));
        assert!(cleanup.is_dialog());
        assert!(matches!(cleanup.back(), Action::None));
        assert!(matches!(press(&mut cleanup, KeyCode::Enter), Action::None));

        drop(sender);
        cleanup.poll();
        let screen = render(&mut cleanup);
        assert!(screen.contains("Cleanup stopped."));
        assert!(screen.contains("Some items may have moved."));
        assert!(screen.contains("Trash"));
        assert!(screen.contains("before trying again."));
        assert!(matches!(
            &cleanup.state,
            State::Done(Err(reason)) if reason.contains("Check the Trash before trying again.")
        ));
        assert!(!screen.contains("Nothing was moved."));
        assert!(!screen.contains("Cleanup done"));
        assert!(!cleanup.is_dialog());
        assert!(matches!(cleanup.back(), Action::Home));
        assert!(matches!(press(&mut cleanup, KeyCode::Enter), Action::Home));
    }

    #[test]
    fn queued_cleanup_results_survive_the_worker_exiting() {
        let (_dir, planned) = planned();
        let outcome = Outcome {
            moved: vec![clean::Moved {
                path: planned.home.join("cache"),
                size: 5000,
            }],
            skipped: Vec::new(),
        };
        for result in [Ok(outcome), Err("Cannot read the home folder.".to_string())] {
            let succeeded = result.is_ok();
            let (sender, messages) = mpsc::channel();
            sender.send(Message::Progress(1, 1)).unwrap();
            sender.send(Message::Done(result)).unwrap();
            drop(sender);
            let mut cleanup = Cleanup {
                planned: Arc::clone(&planned),
                state: State::Moving {
                    done: 0,
                    total: 1,
                    messages,
                },
                scroll: Scroll::default(),
            };
            cleanup.poll();
            cleanup.poll();
            let screen = render(&mut cleanup);
            if succeeded {
                assert!(screen.contains("✓ Moved 1 item to the Trash"));
            } else {
                assert!(screen.contains("Cannot read the home folder."));
            }
            assert!(!screen.contains("ended without a result"));
            assert!(matches!(cleanup.back(), Action::Home));
        }
    }

    #[test]
    fn cleanup_moves_items_then_goes_home() {
        let (_dir, planned) = planned();
        let mut cleanup = Cleanup::start_with(planned, |_| false, |_| Ok(()));
        assert!(matches!(cleanup.back(), Action::None) || !cleanup.is_moving());

        finish(&mut cleanup);

        let screen = render(&mut cleanup);
        assert!(screen.contains("✓ Moved 1 item to the Trash"));
        assert!(screen.contains("Empty the Trash to free that space."));
        assert!(screen.contains("Enter or Esc for Home"));
        assert!(matches!(cleanup.back(), Action::Home));
        assert!(!cleanup.is_dialog());
    }

    #[test]
    fn cleanup_lists_what_it_skipped() {
        let (_dir, planned) = planned();
        let mut cleanup = Cleanup::start_with(planned, |_| true, |_| Ok(()));

        finish(&mut cleanup);

        let screen = render(&mut cleanup);
        assert!(screen.contains("Nothing was moved."));
        assert!(screen.contains("Skipped 1 item,"));
        assert!(screen.contains("Xcode is open"));
    }

    #[test]
    fn a_selected_caution_rule_is_reviewed_too() {
        let (_dir, mut planned) = planned();
        let planned_mut = Arc::get_mut(&mut planned).expect("only owner");
        for rule in &mut planned_mut.plan.rules {
            if rule.rule.tier == Tier::Caution && !rule.items.is_empty() {
                rule.selected = true;
            }
        }

        let screen = render(&mut Review::new(planned));

        assert!(screen.contains("Review · 2 items"));
        assert!(screen.contains("npm cache"));
    }

    #[test]
    fn selected_rules_that_found_nothing_are_left_out() {
        let (_dir, mut planned) = planned();
        let planned_mut = Arc::get_mut(&mut planned).expect("only owner");
        for rule in &mut planned_mut.plan.rules {
            if rule.items.is_empty() {
                rule.selected = true;
            }
        }

        let screen = render(&mut Review::new(planned));

        assert!(!screen.contains("0 items"));
        assert!(!screen.contains("Yarn cache"));
    }

    #[test]
    fn a_wide_review_has_a_summary_beside_the_paths() {
        let (_dir, planned) = planned();
        let screen = render(&mut Review::new(planned));

        for text in ["┌ Total", "┌ After cleanup", "Enter, then y"] {
            assert!(screen.contains(text), "{text}");
        }
    }

    #[test]
    fn rules_with_the_same_name_are_one_group() {
        let (_dir, mut planned) = planned();
        let planned_mut = Arc::get_mut(&mut planned).expect("only owner");
        for rule in &mut planned_mut.plan.rules {
            if !rule.items.is_empty() {
                rule.selected = true;
                rule.rule.name = "HTTPStorages".to_string();
            }
        }

        let screen = render(&mut Review::new(planned));

        // Once, as the one group. A single group has no share to show.
        assert_eq!(screen.matches("HTTPStorages").count(), 1, "{screen}");
        assert!(screen.contains("HTTPStorages · 2 items"), "{screen}");
    }
    #[test]
    fn a_wide_review_lists_items_by_name_under_their_folder() {
        use crate::ui::visual::tests as view;
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let (dir, _) = planned();
        let more = dir
            .path()
            .join("Library/Developer/Xcode/DerivedData/ModuleCache.noindex/x");
        fs::create_dir_all(more.parent().unwrap()).unwrap();
        fs::write(more, vec![1; 90_000]).unwrap();
        let roots = CleanupRoots::new(dir.path()).unwrap();
        let set = rules::load(&roots, None);
        let mut plan = plan(&set.rules, &roots, SystemTime::now());
        for rule in &mut plan.rules {
            rule.selected = !rule.items.is_empty();
        }
        let planned = Arc::new(Planned {
            plan,
            errors: Vec::new(),
            home: roots.home().to_path_buf(),
        });
        let mut review = Review::new(planned);
        let text = view::text(&view::render("review", &mut review, &context, 160, 40));
        assert!(text.contains("Xcode build files · 2 items"), "{text}");
        assert!(
            text.contains("~/Library/Developer/Xcode/DerivedData"),
            "{text}"
        );
        // The largest first, by name, with nothing cut out of the middle
        let big = text.find("ModuleCache.noindex").expect("listed");
        let small = text.find("App-abc").expect("listed");
        assert!(big < small, "{text}");
        assert!(text.contains("┌ What goes"), "{text}");
        assert!(text.contains("Close first  Xcode"), "{text}");
        assert!(!text.contains("Nothing kept"), "{text}");
        assert!(!text.contains("┌ Kept"), "{text}");
    }

    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let (_dir, planned) = planned();
        let mut review = Review::new(Arc::clone(&planned));
        let mut confirm = Confirm::new(planned);
        for (width, height) in view::SIZES {
            let buffer = view::render("review", &mut review, &context, width, height);
            assert!(view::text(&buffer).contains("App-abc"));
            let buffer = view::render("confirm", &mut confirm, &context, width, height);
            assert!(view::text(&buffer).contains("Put Back"));
            assert!(view::text(&buffer).contains("go back"));
        }
    }
}
