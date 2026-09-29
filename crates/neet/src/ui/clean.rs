use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Instant, SystemTime};

use neet_core::apps;
use neet_core::clean::{self, Plan, RulePlan, SkipReason};
use neet_core::rules::{self, RuleError, Tier};
use neet_core::safety::CleanupRoots;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::format;
use super::loading::Loading;
use super::review::Review;

/// A plan and the rules that could not be loaded, or why planning failed.
type Outcome = Result<Planned, String>;

pub struct Planned {
    pub plan: Plan,
    pub errors: Vec<RuleError>,
    /// The real home folder, for showing paths as `~`
    pub home: PathBuf,
}

/// Loads the rules and makes the dry run plan on its own thread. Nothing on
/// disk is changed.
fn make_plan(home: &Path, user_rules: Option<&Path>) -> Outcome {
    let roots = CleanupRoots::new(home)
        .map_err(|error| format!("The home folder could not be read: {error}"))?;
    let set = rules::load(&roots, user_rules);
    let plan = clean::plan(&set.rules, &roots, SystemTime::now());
    Ok(Planned {
        plan,
        errors: set.errors,
        home: roots.home().to_path_buf(),
    })
}

/// How much every rule found, planned in the background for Home
pub struct Estimate {
    result: Option<Receiver<Outcome>>,
    size: Option<u64>,
}

impl Estimate {
    /// Plans every rule for `home` on its own thread. Nothing on disk is
    /// changed. Without a home folder there is nothing to plan.
    pub fn start(home: Option<PathBuf>) -> Self {
        let result = home.map(|home| {
            let (sender, result) = mpsc::channel();
            thread::spawn(move || {
                let user_rules = home.join(".config/neet/rules");
                let _ = sender.send(make_plan(&home, Some(&user_rules)));
            });
            result
        });
        Self { result, size: None }
    }

    pub fn poll(&mut self) {
        if let Some(result) = &self.result
            && let Ok(outcome) = result.try_recv()
        {
            self.size = outcome
                .ok()
                .map(|planned| planned.plan.rules.iter().map(RulePlan::size).sum());
            self.result = None;
        }
    }

    /// The total, once planned
    pub fn size(&self) -> Option<u64> {
        self.size
    }
}

enum State {
    Planning {
        started: Instant,
        result: Receiver<Outcome>,
    },
    /// Shared with the review and the cleanup while they are open
    Ready(Arc<Planned>),
    Failed(String),
}

/// Lists what each cleanup rule found, and lets you choose rules.
pub struct Clean {
    state: State,
    list: TableState,
    /// What you have typed to select an `expert` rule, while the box is open
    typing: Option<String>,
    /// Whether an app is open. A rule whose app is open cannot be selected.
    is_running: fn(&str) -> bool,
    /// A message about the last key, such as an app to close first
    note: Option<String>,
}

impl Clean {
    /// Starts planning for the home folder in `HOME`.
    pub fn new() -> Self {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return Self::from_state(State::Failed(
                "HOME is not set, so there is no home folder to clean.".to_string(),
            ));
        };
        let user_rules = home.join(".config/neet/rules");
        Self::start(move || make_plan(&home, Some(&user_rules)))
    }

    fn start(work: impl FnOnce() -> Outcome + Send + 'static) -> Self {
        let (sender, result) = mpsc::channel();
        thread::spawn(move || {
            // The receiver is gone only when the screen was closed.
            let _ = sender.send(work());
        });
        Self::from_state(State::Planning {
            started: Instant::now(),
            result,
        })
    }

    fn from_state(state: State) -> Self {
        Self {
            state,
            list: TableState::default().with_selected(Some(0)),
            typing: None,
            is_running: apps::is_running,
            note: None,
        }
    }

    /// Shows a finished plan. `safe` rules whose app is open start cleared.
    fn show(&mut self, mut planned: Planned) {
        // Largest first, and rules that found nothing last
        planned
            .plan
            .rules
            .sort_by_key(|rule| (rule.items.is_empty(), Reverse(rule.size())));
        let mut cleared = Vec::new();
        for rule in planned.plan.rules.iter_mut().filter(|rule| rule.selected) {
            if let Some(app) = self.open_app(rule) {
                rule.selected = false;
                cleared.push(format!("{} ({app} is open)", rule.rule.name));
            }
        }
        if !cleared.is_empty() {
            self.note = Some(format!("Not selected: {}.", cleared.join(", ")));
        }
        self.state = State::Ready(Arc::new(planned));
    }

    /// The first app this rule needs closed that is open
    fn open_app(&self, rule: &RulePlan) -> Option<String> {
        rule.rule
            .requires_quit
            .iter()
            .find(|app| (self.is_running)(app))
            .cloned()
    }

    #[cfg(test)]
    fn ready(planned: Planned) -> Self {
        Self::ready_with(planned, |_| false)
    }

    #[cfg(test)]
    fn ready_with(planned: Planned, is_running: fn(&str) -> bool) -> Self {
        let mut clean = Self::from_state(State::Failed(String::new()));
        clean.is_running = is_running;
        clean.show(planned);
        clean
    }

    fn poll(&mut self) {
        if let State::Planning { result, .. } = &self.state
            && let Ok(outcome) = result.try_recv()
        {
            match outcome {
                Ok(planned) => self.show(planned),
                Err(reason) => self.state = State::Failed(reason),
            }
        }
    }

    fn selected(&self) -> usize {
        self.list.selected().unwrap_or(0)
    }

    fn toggle(&mut self) {
        let index = self.selected();
        let open_app = self
            .selected_rule()
            .filter(|rule| !rule.selected)
            .and_then(|rule| self.open_app(rule));
        let State::Ready(planned) = &mut self.state else {
            return;
        };
        // Only the review holds the plan too, and it is closed while this
        // screen takes keys.
        let Some(planned) = Arc::get_mut(planned) else {
            return;
        };
        let Some(rule) = planned.plan.rules.get_mut(index) else {
            return;
        };
        if rule.items.is_empty() {
            return;
        }
        if let Some(app) = open_app {
            self.note = Some(format!(
                "Close {app} first, then select {}.",
                rule.rule.name
            ));
        } else if rule.rule.tier == Tier::Expert && !rule.selected {
            // Selecting an expert rule needs its ID typed out.
            self.typing = Some(String::new());
        } else {
            rule.selected = !rule.selected;
        }
    }

    fn selected_rule(&self) -> Option<&RulePlan> {
        match &self.state {
            State::Ready(planned) => planned.plan.rules.get(self.selected()),
            _ => None,
        }
    }

    /// Handles a key while the box for an `expert` rule is open.
    fn type_key(&mut self, code: KeyCode) {
        let Some(typed) = &mut self.typing else {
            return;
        };
        match code {
            KeyCode::Char(c) => typed.push(c),
            KeyCode::Backspace => {
                typed.pop();
            }
            KeyCode::Enter => {
                let typed = self.typing.take().unwrap_or_default();
                let index = self.selected();
                if let State::Ready(planned) = &mut self.state
                    && let Some(planned) = Arc::get_mut(planned)
                    && let Some(rule) = planned.plan.rules.get_mut(index)
                    && typed == rule.rule.id
                {
                    rule.selected = true;
                }
            }
            _ => {}
        }
    }

    fn draw_typing(&self, frame: &mut Frame, area: Rect) {
        let (Some(typed), Some(rule)) = (&self.typing, self.selected_rule()) else {
            return;
        };
        let lines = vec![
            Line::from(format!("{} is an expert rule.", rule.rule.name)).bold(),
            Line::from("Its files may not exist anywhere else.").red(),
            Line::default(),
            Line::from(vec![
                Span::raw("Type "),
                Span::raw(rule.rule.id.clone()).bold(),
                Span::raw(" and press Enter to select it."),
            ]),
            Line::from(format!("> {typed}█")).cyan(),
        ];
        let [area] = Layout::vertical([Constraint::Length(7)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(64)])
            .flex(Flex::Center)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" Select an expert rule ")
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
    }

    fn rule_count(&self) -> usize {
        match &self.state {
            State::Ready(planned) => planned.plan.rules.len(),
            _ => 0,
        }
    }
}

fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Safe => "safe",
        Tier::Caution => "caution",
        Tier::Expert => "expert",
    }
}

fn tier_span(tier: Tier) -> Span<'static> {
    let span = Span::raw(tier_name(tier));
    match tier {
        Tier::Safe => span.green(),
        Tier::Caution => span.yellow(),
        Tier::Expert => span.red(),
    }
}

fn tier_meaning(tier: Tier) -> &'static str {
    match tier {
        Tier::Safe => "The files come back, and the only cost is time. Selected from the start.",
        Tier::Caution => "You may need to download, index, or sign in again. You select it.",
        Tier::Expert => "The files may not exist anywhere else. You type its ID to select it.",
    }
}

/// `path` with the home folder shown as `~`.
pub(super) fn display_path(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

pub(super) fn skip_reason(reason: &SkipReason, min_age_days: Option<u32>) -> String {
    match reason {
        SkipReason::Refused(error) => format!("refused: {error}"),
        SkipReason::TooNew => match min_age_days {
            // Newer than the moment the plan started: written while neet looked
            Some(0) => "changed while neet was looking".to_string(),
            Some(days) => format!("changed in the last {days} days"),
            None => "changed too recently".to_string(),
        },
        SkipReason::Unreadable(error) => format!("could not be read: {error}"),
        SkipReason::Overlaps => "already found by another rule".to_string(),
        SkipReason::Link => "a link, left in place".to_string(),
        SkipReason::AppOpen(app) => format!("{app} is open"),
        SkipReason::Replaced => "changed into a different file after the review".to_string(),
        SkipReason::MoveFailed(error) => format!("could not be moved: {error}"),
    }
}

pub(super) fn count(value: usize) -> String {
    format::count(u64::try_from(value).unwrap_or(u64::MAX))
}

/// A count and a noun, such as `1 item` or `27 items`
pub(super) fn items(value: usize) -> String {
    format!(
        "{} {}",
        count(value),
        if value == 1 { "item" } else { "items" }
    )
}

/// Below this width the details go under the list instead of beside it.
const MIN_SIDE_WIDTH: u16 = 100;

/// A checkbox: green when selected, and blank when there is nothing to select
pub(super) fn checkbox(selected: bool, selectable: bool) -> Span<'static> {
    match (selectable, selected) {
        (false, _) => Span::raw("   "),
        (true, true) => Span::raw("[✓]").green().bold(),
        (true, false) => Span::raw("[ ]").dark_gray(),
    }
}

/// One rule as a table row. `current` is the row the arrow is on, whose
/// name turns cyan, so the checkbox and risk keep their own colors.
fn rule_row(rule: &RulePlan, current: bool) -> Row<'static> {
    let items = rule.items.len();
    let name = Span::raw(rule.rule.name.clone());
    let name = if current { name.cyan().bold() } else { name };
    if items == 0 {
        return Row::new([
            Cell::from(checkbox(false, false)),
            Cell::from(name),
            Cell::from(tier_span(rule.rule.tier)),
            Cell::from(Line::from("none").right_aligned()),
            Cell::from(Line::from("·").right_aligned()),
        ])
        .dark_gray();
    }
    let found = self::items(items);
    Row::new([
        Cell::from(checkbox(rule.selected, true)),
        Cell::from(name),
        Cell::from(tier_span(rule.rule.tier)),
        Cell::from(Line::from(found).dark_gray().right_aligned()),
        Cell::from(Line::from(format::size(rule.size())).right_aligned()),
    ])
}

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<8}")).dark_gray(), value])
}

/// How many screen rows `lines` take when wrapped to `width`
fn rows_used(lines: &[Line], width: usize) -> usize {
    lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width.max(1)))
        .sum()
}

/// The folder every path found or skipped is in, when they share one
fn shared_folder(rule: &RulePlan) -> Option<&Path> {
    let mut parents = rule
        .items
        .iter()
        .map(|item| item.path.path())
        .chain(rule.skipped.iter().map(|skipped| skipped.path.as_path()))
        .map(Path::parent);
    let first = parents.next()??;
    parents.all(|parent| parent == Some(first)).then_some(first)
}

/// `path` as shown in the details: only its name when the rule's paths share
/// a folder, which is shown once above them
fn detail_path(home: &Path, path: &Path, shared: bool, width: usize) -> String {
    if shared && let Some(name) = path.file_name() {
        return format::shorten_middle(&name.to_string_lossy(), width);
    }
    format::shorten_path(&display_path(home, path), width)
}

/// Skipped paths, with a reason shared by more than two of them on one line
fn skipped_lines(rule: &RulePlan, home: &Path, width: usize) -> Vec<Line<'static>> {
    let shared = shared_folder(rule).is_some();
    let mut groups: Vec<(String, Vec<&Path>)> = Vec::new();
    for skipped in &rule.skipped {
        let reason = skip_reason(&skipped.reason, Some(rule.rule.min_age_days));
        match groups.iter_mut().find(|(known, _)| *known == reason) {
            Some((_, paths)) => paths.push(&skipped.path),
            None => groups.push((reason, vec![&skipped.path])),
        }
    }
    let mut lines = Vec::new();
    for (reason, paths) in groups {
        if paths.len() > 2 {
            lines.push(
                Line::from(format!(
                    "{:>9}  {reason}",
                    format!("{} paths", count(paths.len()))
                ))
                .dark_gray(),
            );
            continue;
        }
        for path in paths {
            let room = width.saturating_sub(reason.chars().count() + 13);
            lines.push(
                Line::from(format!(
                    "{:>9}  {}  {reason}",
                    "",
                    detail_path(home, path, shared, room)
                ))
                .dark_gray(),
            );
        }
    }
    lines
}

/// The selected rule, explained: what it removes, its risk, and every path
/// the dry run found or skipped, cut to fit `room` rows of `width`.
fn details(rule: &RulePlan, home: &Path, room: usize, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(rule.rule.description.clone()),
        Line::default(),
        field("Risk", tier_span(rule.rule.tier).bold()),
        Line::from(tier_meaning(rule.rule.tier)).dark_gray(),
        Line::default(),
    ];
    if !rule.rule.requires_quit.is_empty() {
        lines.push(field(
            "Close",
            Span::raw(rule.rule.requires_quit.join(", ")),
        ));
    }
    if rule.rule.min_age_days > 0 {
        lines.push(field(
            "Keeps",
            Span::raw(format!(
                "anything changed in the last {} days",
                rule.rule.min_age_days
            )),
        ));
    }
    let shared = shared_folder(rule);
    if let Some(folder) = shared {
        lines.push(field("Folder", Span::raw(display_path(home, folder))));
    }
    if lines.last().is_some_and(|line| line.width() > 0) {
        lines.push(Line::default());
    }

    if rule.items.is_empty() && rule.skipped.is_empty() {
        let paths: Vec<String> = rule
            .rule
            .paths
            .iter()
            .map(|path| display_path(home, path))
            .collect();
        lines.push(Line::from(format!("Nothing found in {}.", paths.join(", "))).dark_gray());
        return lines;
    }

    let mut largest: Vec<_> = rule.items.iter().collect();
    largest.sort_by_key(|item| Reverse(item.size));
    let path_room = width.saturating_sub(11);
    let mut found: Vec<Line<'static>> = largest
        .iter()
        .map(|item| {
            Line::from(vec![
                Span::raw(format!("{:>9}  ", format::size(item.size))),
                Span::raw(detail_path(
                    home,
                    item.path.path(),
                    shared.is_some(),
                    path_room,
                )),
            ])
        })
        .collect();
    let mut skipped = skipped_lines(rule, home, width);

    // Fit both lists in what is left, giving skipped paths at most a third.
    let headings = usize::from(!found.is_empty()) + 2 * usize::from(!skipped.is_empty());
    let left = room.saturating_sub(rows_used(&lines, width) + headings);
    let skipped_room = skipped.len().min((left / 3).max(2));
    let found_room = left.saturating_sub(skipped_room).max(2);
    for (list, room) in [(&mut found, found_room), (&mut skipped, skipped_room)] {
        if list.len() > room {
            let more = list.len() - (room - 1);
            list.truncate(room - 1);
            list.push(Line::from(format!("{:>9}  and {} more", "", count(more))).dark_gray());
        }
    }

    if !found.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("Found ").bold(),
            Span::raw(format!(
                "{} · {}",
                items(rule.items.len()),
                format::size(rule.size())
            ))
            .dark_gray(),
        ]));
        lines.extend(found);
    }
    if !skipped.is_empty() {
        if !rule.items.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(vec![
            Span::raw("Skipped ").bold(),
            Span::raw(format!("{} left in place", count(rule.skipped.len()))).dark_gray(),
        ]));
        lines.extend(skipped);
    }
    lines
}

/// Rules that could not be loaded, so you know why one is missing.
fn problems(errors: &[RuleError]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = errors
        .iter()
        .map(|error| {
            let rule = error
                .rule
                .as_ref()
                .map_or_else(String::new, |id| format!(" rule `{id}`"));
            Line::from(format!(
                "Not loaded: {}{rule}: {}",
                error.file, error.message
            ))
            .yellow()
        })
        .collect();
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines
}

fn totals(planned: &Planned) -> Line<'static> {
    let plan = &planned.plan;
    let rules = plan
        .rules
        .iter()
        .filter(|rule| rule.selected && !rule.items.is_empty())
        .count();
    let text = Span::raw(format!(
        " Selected: {} {} · {} · {} ",
        count(rules),
        if rules == 1 { "rule" } else { "rules" },
        items(plan.selected_count()),
        format::size(plan.selected_size())
    ));
    let mut spans = vec![if rules == 0 {
        text.dark_gray()
    } else {
        text.green().bold()
    }];
    if !planned.errors.is_empty() {
        spans.push(
            Span::raw(format!(
                "· {} rule problems, listed in the details ",
                count(planned.errors.len())
            ))
            .yellow(),
        );
    }
    Line::from(spans)
}

/// What every rule found, for the list's bottom edge
fn found_overall(planned: &Planned) -> Line<'static> {
    let rules = &planned.plan.rules;
    let found = rules.iter().filter(|rule| !rule.items.is_empty()).count();
    let size: u64 = rules.iter().map(RulePlan::size).sum();
    let mut spans = vec![
        Span::raw(format!(
            " {} of {} rules found {} ",
            count(found),
            count(rules.len()),
            format::size(size)
        ))
        .dark_gray(),
    ];
    if !planned.errors.is_empty() {
        spans.push(Span::raw(format!("· {} rule problems ", count(planned.errors.len()))).yellow());
    }
    Line::from(spans)
}

/// What is selected so far, rule by rule, with the total
fn selection(planned: &Planned) -> Paragraph<'static> {
    let chosen: Vec<&RulePlan> = planned
        .plan
        .rules
        .iter()
        .filter(|rule| rule.selected && !rule.items.is_empty())
        .collect();
    let block = Block::bordered()
        .title(" Selected ")
        .padding(Padding::horizontal(1));
    if chosen.is_empty() {
        return Paragraph::new(vec![
            Line::from("Nothing selected yet.").dark_gray(),
            Line::default(),
            Line::from("Press Space to select a rule. Safe rules start selected, and caution rules are yours to choose.").dark_gray(),
        ])
        .wrap(Wrap { trim: false })
        .block(block);
    }
    let mut lines: Vec<Line<'static>> = chosen
        .iter()
        .map(|rule| {
            Line::from(vec![
                Span::raw(format!("{:>9}  ", format::size(rule.size()))),
                Span::raw(rule.rule.name.clone()),
            ])
        })
        .collect();
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::raw(format!(
            "{:>9}  ",
            format::size(planned.plan.selected_size())
        ))
        .green()
        .bold(),
        Span::raw(format!(
            "total, in {}",
            items(planned.plan.selected_count())
        ))
        .bold(),
    ]));
    lines.push(Line::default());
    lines.push(Line::from("Press Enter to see every path before anything moves.").dark_gray());
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block)
}

/// Every rule as a table, largest first, with `summary` on the bottom edge
fn rules_table(planned: &Planned, current: usize, summary: Line<'static>) -> Table<'static> {
    let rows: Vec<Row> = planned
        .plan
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| rule_row(rule, index == current))
        .collect();
    let header = Row::new([
        Cell::from(""),
        Cell::from("Rule"),
        Cell::from("Risk"),
        Cell::from(Line::from("Found").right_aligned()),
        Cell::from(Line::from("Size").right_aligned()),
    ])
    .bold()
    .bottom_margin(1);
    Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .column_spacing(2)
    .block(
        Block::bordered()
            .title(" Clean ")
            .title_bottom(summary)
            .padding(Padding::horizontal(1)),
    )
    .highlight_symbol("▸ ")
}

impl Screen for Clean {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        let planned = match &self.state {
            State::Planning { started, .. } => {
                Loading {
                    title: "Clean",
                    doing: "Finding files the rules cover",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Nothing is changed while neet looks.",
                }
                .draw(frame, area);
                return;
            }
            State::Failed(reason) => {
                let block = Block::bordered()
                    .title(" Clean ")
                    .padding(Padding::horizontal(1));
                frame.render_widget(
                    Paragraph::new(format!("Planning failed. {reason}"))
                        .red()
                        .block(block),
                    area,
                );
                return;
            }
            State::Ready(planned) => planned,
        };

        // Border, header, gap, then one row per rule
        let list_height = u16::try_from(planned.plan.rules.len() + 4).unwrap_or(u16::MAX);
        let (list_area, detail_area, summary) = if area.width >= MIN_SIDE_WIDTH {
            let [left, detail] =
                Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)]).areas(area);
            let [list, cart] = Layout::vertical([
                Constraint::Length(list_height.min(left.height * 2 / 3)),
                Constraint::Fill(1),
            ])
            .areas(left);
            frame.render_widget(selection(planned), cart);
            (list, detail, found_overall(planned))
        } else {
            let [list, detail] = Layout::vertical([
                Constraint::Length(list_height.min(area.height / 2).max(5)),
                Constraint::Fill(1),
            ])
            .areas(area);
            (list, detail, totals(planned))
        };

        let current = self.selected();
        let table = rules_table(planned, current, summary);
        frame.render_stateful_widget(table, list_area, &mut self.list);

        let Some(rule) = planned.plan.rules.get(current) else {
            return;
        };
        let mut lines = problems(&planned.errors);
        if let Some(note) = &self.note {
            lines.insert(0, Line::from(note.clone()).yellow());
            lines.insert(1, Line::default());
        }
        let width = usize::from(detail_area.width.saturating_sub(4));
        let room = usize::from(detail_area.height.saturating_sub(2))
            .saturating_sub(rows_used(&lines, width));
        lines.extend(details(rule, &planned.home, room, width));
        let body = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(format!(" {} ", rule.rule.name))
                .title_bottom(
                    Line::from(" Items go to the Trash, where Put Back works ").dark_gray(),
                )
                .padding(Padding::horizontal(1)),
        );
        frame.render_widget(body, detail_area);
        self.draw_typing(frame, area);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if self.typing.is_some() {
            self.type_key(key.code);
            return Action::None;
        }
        self.note = None;
        let last = self.rule_count().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.list.select(Some(self.selected().saturating_sub(1)));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.list.select(Some((self.selected() + 1).min(last)));
            }
            KeyCode::Char('g') | KeyCode::Home => self.list.select(Some(0)),
            KeyCode::Char('G') | KeyCode::End => self.list.select(Some(last)),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter => {
                if let State::Ready(planned) = &self.state
                    && planned.plan.selected_count() > 0
                {
                    return Action::Open(Box::new(Review::new(Arc::clone(planned))));
                }
            }
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        if self.typing.take().is_some() {
            Action::None
        } else {
            Action::Back
        }
    }

    fn takes_text(&self) -> bool {
        self.typing.is_some()
    }

    fn hints(&self) -> &'static str {
        if self.typing.is_some() {
            return "type the ID · enter select · esc cancel";
        }
        "↑↓ move · space select · enter review · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between rules"),
            ("g  G", "Jump to the first or last rule"),
            (
                "Space",
                "Select or clear a rule. An expert rule asks you to type its ID",
            ),
            ("Enter", "Review every path the selected rules found"),
            ("Esc", "Go back to Home"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neet_core::rules::load;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use std::fs;
    use std::time::Duration;
    use tempfile::{TempDir, tempdir};

    fn fake_home() -> TempDir {
        let dir = tempdir().expect("temporary directory should be created");
        for (file, bytes) in [
            ("Library/Developer/Xcode/DerivedData/App-abc/out", 4000),
            (".npm/_cacache/content-v2/aa", 9000),
            ("Documents/keep.txt", 10),
        ] {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().expect("path should have a parent"))
                .expect("folder should be created");
            fs::write(path, vec![1; bytes]).expect("file should be written");
        }
        dir
    }

    fn planned(dir: &TempDir) -> Planned {
        make_plan(dir.path(), None).expect("plan should be made")
    }

    fn render(clean: &mut Clean) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 40)).unwrap();
        let scan = super::super::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
        };
        terminal
            .draw(|frame| clean.draw(frame, frame.area(), &context))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    fn press(clean: &mut Clean, code: KeyCode) {
        let scan = super::super::scan::ScanStatus::Failed(String::new());
        clean.handle_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            &Context {
                scan: &scan,
                disk: None,
                cleanable: None,
            },
        );
    }

    fn select_rule(clean: &mut Clean, id: &str) {
        let State::Ready(planned) = &clean.state else {
            panic!("plan should be ready");
        };
        let index = planned
            .plan
            .rules
            .iter()
            .position(|rule| rule.rule.id == id)
            .expect("rule should be planned");
        clean.list.select(Some(index));
    }

    fn is_selected(clean: &Clean, id: &str) -> bool {
        let State::Ready(planned) = &clean.state else {
            panic!("plan should be ready");
        };
        planned
            .plan
            .rules
            .iter()
            .find(|rule| rule.rule.id == id)
            .expect("rule should be planned")
            .selected
    }

    #[test]
    fn lists_rules_with_tiers_and_selects_only_safe_ones() {
        let dir = fake_home();
        let mut clean = Clean::ready(planned(&dir));
        select_rule(&mut clean, "xcode-derived-data");

        let screen = render(&mut clean);

        assert!(screen.contains("[✓]  Xcode DerivedData"));
        assert!(screen.contains("[ ]  npm cache"));
        assert!(screen.contains("caution"));
        assert!(screen.contains("none"));
        assert!(screen.contains("total, in 1 item"));
        assert!(screen.contains("Folder  ~/Library/Developer/Xcode/DerivedData"));
        assert!(screen.contains("App-abc"));
    }

    #[test]
    fn the_checkbox_on_the_current_row_turns_green() {
        let dir = fake_home();
        let mut clean = Clean::ready(planned(&dir));
        select_rule(&mut clean, "npm-cache");
        press(&mut clean, KeyCode::Char(' '));

        let mut terminal = Terminal::new(TestBackend::new(110, 40)).unwrap();
        let scan = super::super::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
        };
        terminal
            .draw(|frame| clean.draw(frame, frame.area(), &context))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        let cells: Vec<&str> = buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        let row = cells
            .chunks(width)
            .position(|line| line.concat().contains("▸ [✓]  npm cache"))
            .expect("the selected rule should be on the current row");
        let column = cells[row * width..]
            .iter()
            .position(|cell| *cell == "✓")
            .expect("the check should be drawn");

        assert_eq!(
            buffer.content()[row * width + column].fg,
            ratatui::style::Color::Green
        );
    }

    #[test]
    fn space_selects_a_rule_that_found_something() {
        let dir = fake_home();
        let mut clean = Clean::ready(planned(&dir));

        select_rule(&mut clean, "npm-cache");
        press(&mut clean, KeyCode::Char(' '));
        assert!(is_selected(&clean, "npm-cache"));
        assert!(render(&mut clean).contains("total, in 2 items"));

        press(&mut clean, KeyCode::Char(' '));
        assert!(!is_selected(&clean, "npm-cache"));
    }

    #[test]
    fn enter_reviews_only_when_something_is_selected() {
        let dir = fake_home();
        let mut clean = Clean::ready(planned(&dir));
        let scan = super::super::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
        };
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

        assert!(matches!(clean.handle_key(enter, &context), Action::Open(_)));

        select_rule(&mut clean, "xcode-derived-data");
        press(&mut clean, KeyCode::Char(' '));
        assert!(matches!(clean.handle_key(enter, &context), Action::None));
    }

    #[test]
    fn an_expert_rule_needs_its_id_typed() {
        let dir = fake_home();
        let mut planned = planned(&dir);
        let npm = planned
            .plan
            .rules
            .iter_mut()
            .find(|rule| rule.rule.id == "npm-cache")
            .expect("rule should be planned");
        npm.rule.tier = Tier::Expert;
        let mut clean = Clean::ready(planned);
        select_rule(&mut clean, "npm-cache");

        press(&mut clean, KeyCode::Char(' '));
        assert!(clean.takes_text());
        assert!(render(&mut clean).contains("Type npm-cache"));
        for c in "npm-cach".chars() {
            press(&mut clean, KeyCode::Char(c));
        }
        press(&mut clean, KeyCode::Enter);
        assert!(
            !is_selected(&clean, "npm-cache"),
            "a wrong ID should not select"
        );

        press(&mut clean, KeyCode::Char(' '));
        for c in "npm-cachx".chars() {
            press(&mut clean, KeyCode::Char(c));
        }
        press(&mut clean, KeyCode::Backspace);
        press(&mut clean, KeyCode::Char('e'));
        press(&mut clean, KeyCode::Enter);
        assert!(is_selected(&clean, "npm-cache"));
        assert!(!clean.takes_text());

        press(&mut clean, KeyCode::Char(' '));
        assert!(
            !is_selected(&clean, "npm-cache"),
            "clearing needs no typing"
        );
    }

    #[test]
    fn esc_closes_the_expert_box_before_leaving() {
        let dir = fake_home();
        let mut planned = planned(&dir);
        for rule in &mut planned.plan.rules {
            rule.rule.tier = Tier::Expert;
        }
        let mut clean = Clean::ready(planned);
        select_rule(&mut clean, "npm-cache");
        press(&mut clean, KeyCode::Char(' '));

        assert!(matches!(clean.back(), Action::None));
        assert!(!clean.takes_text());
        assert!(matches!(clean.back(), Action::Back));
    }

    #[test]
    fn a_rule_cannot_be_selected_while_its_app_is_open() {
        let dir = fake_home();
        let mut clean = Clean::ready_with(planned(&dir), |app| app == "com.apple.dt.Xcode");

        assert!(!is_selected(&clean, "xcode-derived-data"));
        assert!(render(&mut clean).contains("Not selected: Xcode DerivedData"));

        select_rule(&mut clean, "xcode-derived-data");
        press(&mut clean, KeyCode::Char(' '));
        assert!(!is_selected(&clean, "xcode-derived-data"));
        assert!(render(&mut clean).contains("Close com.apple.dt.Xcode first"));

        select_rule(&mut clean, "npm-cache");
        press(&mut clean, KeyCode::Char(' '));
        assert!(
            is_selected(&clean, "npm-cache"),
            "rules without the app still work"
        );
    }

    #[test]
    fn space_does_nothing_on_a_rule_that_found_nothing() {
        let dir = fake_home();
        let mut clean = Clean::ready(planned(&dir));

        select_rule(&mut clean, "yarn-cache");
        press(&mut clean, KeyCode::Char(' '));

        assert!(!is_selected(&clean, "yarn-cache"));
    }

    #[test]
    fn planning_runs_in_the_background_and_changes_nothing() {
        let dir = fake_home();
        let home = dir.path().to_path_buf();
        let before = fs::read_dir(home.join(".npm/_cacache"))
            .expect("folder should be read")
            .count();

        let mut clean = Clean::start(move || make_plan(&home, None));
        assert!(render(&mut clean).contains("Finding files"));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(clean.state, State::Ready(_)) {
            assert!(Instant::now() < deadline, "plan should finish");
            thread::sleep(Duration::from_millis(5));
            clean.poll();
        }

        assert!(render(&mut clean).contains("npm cache"));
        let after = fs::read_dir(dir.path().join(".npm/_cacache"))
            .expect("folder should be read")
            .count();
        assert_eq!(before, after);
    }

    #[test]
    fn estimate_adds_up_every_rule_in_the_background() {
        let dir = fake_home();
        let mut estimate = Estimate::start(Some(dir.path().to_path_buf()));
        let deadline = Instant::now() + Duration::from_secs(10);
        while estimate.size().is_none() {
            assert!(Instant::now() < deadline, "estimate should finish");
            thread::sleep(Duration::from_millis(5));
            estimate.poll();
        }

        let planned = planned(&dir);
        let all: u64 = planned.plan.rules.iter().map(RulePlan::size).sum();
        assert_eq!(estimate.size(), Some(all));
        assert!(all > 0);
        assert_eq!(Estimate::start(None).size(), None);
    }

    #[test]
    fn shows_rule_problems() {
        let dir = fake_home();
        let rules_dir = dir.path().join("rules");
        fs::create_dir(&rules_dir).expect("folder should be created");
        fs::write(rules_dir.join("broken.toml"), "[[rule]\n").expect("file should be written");
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        let set = load(&roots, Some(&rules_dir));
        let mut planned = planned(&dir);
        planned.errors = set.errors;

        let screen = render(&mut Clean::ready(planned));

        assert!(screen.contains("1 rule problems"));
        assert!(screen.contains("Not loaded:"));
    }
}
