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
use ratatui::widgets::{Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::format;
use super::loading::Loading;
use super::review::Review;

/// A plan and the rules that could not be loaded, or why planning failed.
type Outcome = Result<Planned, String>;

#[derive(Clone)]
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

/// Every rule's plan, made once in the background and kept, so Home can
/// show the total and Clean opens without looking again. Planned again after
/// a cleanup, or when you ask Clean to refresh.
pub struct Estimate {
    result: Option<Receiver<Outcome>>,
    started: Instant,
    pub(super) planned: Option<Arc<Planned>>,
    failed: Option<String>,
}

impl Estimate {
    /// Plans every rule for `home` on its own thread. Nothing on disk is
    /// changed. Without a home folder there is nothing to plan.
    pub fn start(home: Option<PathBuf>) -> Self {
        let failed = home
            .is_none()
            .then(|| "HOME is not set, so there is no home folder to clean.".to_string());
        let result = home.map(|home| {
            let (sender, result) = mpsc::channel();
            thread::spawn(move || {
                let user_rules = home.join(".config/neet/rules");
                let _ = sender.send(make_plan(&home, Some(&user_rules)));
            });
            result
        });
        Self {
            result,
            started: Instant::now(),
            planned: None,
            failed,
        }
    }

    pub fn poll(&mut self) {
        if let Some(result) = &self.result
            && let Ok(outcome) = result.try_recv()
        {
            match outcome {
                Ok(planned) => self.planned = Some(Arc::new(planned)),
                Err(reason) => self.failed = Some(reason),
            }
            self.result = None;
        }
    }

    /// The total, once planned
    pub fn size(&self) -> Option<u64> {
        self.planned
            .as_ref()
            .map(|planned| planned.plan.rules.iter().map(RulePlan::size).sum())
    }
}

enum State {
    /// Waiting for the plan the app makes in the background
    Shared,
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
    /// Opens on the plan the app made in the background, without looking
    /// again.
    pub fn new() -> Self {
        Self::from_state(State::Shared)
    }

    /// Takes a copy of the shared plan once it is ready, so selecting rules
    /// here never changes it. Without one, such as in tests, plans by itself.
    fn adopt(&mut self, context: &Context) {
        if !matches!(self.state, State::Shared) {
            return;
        }
        match context.plan {
            Some(estimate) => {
                if let Some(planned) = &estimate.planned {
                    self.show(Planned::clone(planned));
                } else if let Some(reason) = &estimate.failed {
                    self.state = State::Failed(reason.clone());
                }
            }
            None => match std::env::var_os("HOME").map(PathBuf::from) {
                Some(home) => {
                    let user_rules = home.join(".config/neet/rules");
                    *self = Self::start(move || make_plan(&home, Some(&user_rules)));
                }
                None => {
                    self.state = State::Failed(
                        "HOME is not set, so there is no home folder to clean.".to_string(),
                    );
                }
            },
        }
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
            Line::from(format!("> {typed}█")).fg(super::visual::ACCENT),
        ];
        let [area] = Layout::vertical([Constraint::Length(
            super::visual::wrapped_rows(&lines, area.width.min(64).saturating_sub(4))
                .saturating_add(2),
        )])
        .flex(Flex::Center)
        .areas(area);
        let [area] = Layout::horizontal([Constraint::Length(64)])
            .flex(Flex::Center)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                super::visual::block()
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
        Tier::Safe => "Apps make these again. Picked for you.",
        Tier::Caution => "May need a re-download or sign-in. You pick these.",
        Tier::Expert => "May hold files you can't get back. Type its ID to pick it.",
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
        SkipReason::Overlaps => "found by another rule".to_string(),
        SkipReason::Link => "a link, kept".to_string(),
        SkipReason::AppOpen(app) => format!("{app} is open"),
        SkipReason::Replaced => "changed after you checked it".to_string(),
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
        (true, false) => Span::raw("[ ]"),
    }
}

/// One rule as a table row. `current` is the row the arrow is on, whose
/// name turns bold, so the checkbox and risk keep their own colors.
fn rule_row(rule: &RulePlan, current: bool, columns: &super::visual::Columns<5>) -> Row<'static> {
    let items = rule.items.len();
    let name = Span::raw(rule.rule.name.clone());
    let name = if current { name.bold() } else { name };
    if items == 0 {
        return columns.row([
            Cell::from(checkbox(false, false)),
            Cell::from(name),
            Cell::from(tier_span(rule.rule.tier)),
            Cell::from(Line::from("none").right_aligned()),
            Cell::from(Line::from("·").right_aligned()),
        ]);
    }
    columns.row([
        Cell::from(checkbox(rule.selected, true)),
        Cell::from(name),
        Cell::from(tier_span(rule.rule.tier)),
        Cell::from(Line::from(count(items)).right_aligned()),
        Cell::from(
            Line::from(format::size_span(rule.size(), format::size(rule.size()))).right_aligned(),
        ),
    ])
}

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<8}")).bold(), value])
}

/// How many screen rows `lines` take when wrapped to `width` at spaces, as
/// the details are
pub(super) fn rows_used(lines: &[Line], width: usize) -> usize {
    usize::from(super::visual::wrapped_rows(
        lines,
        u16::try_from(width).unwrap_or(u16::MAX),
    ))
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
            lines.push(Line::from(format!(
                "{:>9}  {reason}",
                format!("{} paths", count(paths.len()))
            )));
            continue;
        }
        for path in paths {
            let room = width.saturating_sub(format::display_width(&reason) + 13);
            lines.push(Line::from(format!(
                "{:>9}  {}  {reason}",
                "",
                detail_path(home, path, shared, room)
            )));
        }
    }
    lines
}

/// Width of the bar beside each item found
const ITEM_BAR: usize = 10;

/// What the selected rule removes, its risk, and what it needs, for the box
/// at the top of the details
fn about(rule: &RulePlan, home: &Path) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(rule.rule.description.clone()),
        Line::default(),
        Line::from(vec![
            Span::raw(format!("{:<8}", "Risk")).bold(),
            tier_span(rule.rule.tier).bold(),
            Span::raw(format!("  {}", tier_meaning(rule.rule.tier))),
        ]),
    ];
    if !rule.rule.requires_quit.is_empty() {
        lines.push(field("Quit", Span::raw(rule.rule.requires_quit.join(", "))));
    }
    if rule.rule.min_age_days > 0 {
        lines.push(field(
            "Keeps",
            Span::raw(format!(
                "files changed in the last {} days",
                rule.rule.min_age_days
            )),
        ));
    }
    let folder = shared_folder(rule).map_or_else(
        || {
            rule.rule
                .paths
                .iter()
                .map(|path| display_path(home, path))
                .collect::<Vec<_>>()
                .join(", ")
        },
        |folder| display_path(home, folder),
    );
    lines.push(field("Folder", Span::raw(folder).fg(super::visual::ACCENT)));
    lines
}

/// Every item the rule found, largest first, with a bar against the
/// largest, cut to `room` rows of `width`
fn found_lines(rule: &RulePlan, home: &Path, room: usize, width: usize) -> Vec<Line<'static>> {
    if rule.items.is_empty() {
        return vec![Line::from("Nothing found.")];
    }
    let shared = shared_folder(rule).is_some();
    let mut largest: Vec<_> = rule.items.iter().collect();
    largest.sort_by_key(|item| Reverse(item.size));
    let top = largest.first().map_or(0, |item| item.size);
    let path_room = width.saturating_sub(11 + ITEM_BAR + 2);
    let mut lines: Vec<Line<'static>> = largest
        .iter()
        .map(|item| {
            Line::from(vec![
                format::size_span(item.size, format!("{:>9}  ", format::size(item.size))),
                Span::raw(format::bar(item.size, top, ITEM_BAR)).green(),
                Span::raw("  "),
                Span::raw(detail_path(home, item.path.path(), shared, path_room)),
            ])
        })
        .collect();
    let room = room.max(2);
    if lines.len() > room {
        let more = lines.len() - (room - 1);
        lines.truncate(room - 1);
        lines.push(Line::from(format!("{:>9}  and {} more", "", count(more))));
    }
    lines
}

impl Clean {
    /// The selected rule on a tall screen, in boxes that keep their size
    /// whichever rule is selected: what it is, as tall as the longest rule's
    /// text; what it found, with what it left in place below, down to the
    /// chart; and the chart, at the bottom.
    fn draw_details_tall(
        frame: &mut Frame,
        area: Rect,
        planned: &Planned,
        rule: &RulePlan,
        notes: Vec<Line<'static>>,
    ) {
        let home = planned.home.as_path();
        let width = usize::from(area.width.saturating_sub(4));
        let tallest = planned
            .plan
            .rules
            .iter()
            .map(|other| {
                let mut lines = notes.clone();
                lines.extend(about(other, home));
                rows_used(&lines, width)
            })
            .max()
            .unwrap_or(0);
        let top_height = u16::try_from(tallest + 2)
            .unwrap_or(u16::MAX)
            .min(area.height / 3);
        let [about_area, found_area, chart_area] = Layout::vertical([
            Constraint::Length(top_height),
            Constraint::Fill(1),
            Constraint::Length(area.height * 9 / 20),
        ])
        .areas(area);
        let mut top = notes;
        top.extend(about(rule, home));
        frame.render_widget(
            Paragraph::new(top).wrap(Wrap { trim: false }).block(
                super::visual::block()
                    .title(format!(" {} ", rule.rule.name))
                    .padding(Padding::horizontal(1)),
            ),
            about_area,
        );

        // What was found, and below it what was left in place, each cut
        // short with a count when there is not room for all of it
        let inner = usize::from(found_area.height.saturating_sub(2));
        let mut skipped = skipped_lines(rule, home, width);
        let mut sections = Vec::new();
        if skipped.is_empty() {
            sections.push(found_lines(rule, home, inner, width));
        } else {
            let most = (inner / 2).saturating_sub(1).max(1);
            if skipped.len() > most {
                let more = skipped.len() - (most - 1).max(1);
                skipped.truncate((most - 1).max(1));
                skipped.push(Line::from(format!("{:>9}  and {} more", "", count(more))));
            }
            let room = inner.saturating_sub(skipped.len() + 2);
            sections.push(found_lines(rule, home, room, width));
            let mut left = vec![
                Line::from(format!("Skipped · {}", count(rule.skipped.len())))
                    .style(super::visual::HEADING),
            ];
            left.extend(skipped);
            sections.push(left);
        }
        super::visual::sections_in(frame, found_area, Self::found_block(rule), sections);
        let chart = chart_lines(planned, rule, width);
        draw_location(frame, chart_area, planned, chart);
    }

    /// The found box, titled with how much the rule found
    fn found_block(rule: &RulePlan) -> ratatui::widgets::Block<'static> {
        let title = if rule.items.is_empty() {
            " Found ".to_string()
        } else {
            format!(
                " Found · {} · {} ",
                items(rule.items.len()),
                format::size(rule.size())
            )
        };
        super::visual::block()
            .title(Line::from(title).bold())
            .title_bottom(Line::from(" Goes to the Trash ").right_aligned())
            .padding(Padding::horizontal(1))
    }

    /// The selected rule in up to three boxes: what it is, what it found,
    /// and what it left in place. `notes` go at the top.
    fn draw_details(
        frame: &mut Frame,
        area: Rect,
        planned: &Planned,
        rule: &RulePlan,
        notes: Vec<Line<'static>>,
    ) {
        if area.height >= TALL_DETAILS {
            Self::draw_details_tall(frame, area, planned, rule, notes);
            return;
        }
        let home = planned.home.as_path();
        let width = usize::from(area.width.saturating_sub(4));
        let mut top = notes;
        top.extend(about(rule, home));
        let top_height = u16::try_from(rows_used(&top, width) + 2).unwrap_or(u16::MAX);

        let mut skipped = skipped_lines(rule, home, width);
        let skipped_height = if skipped.is_empty() {
            0
        } else {
            let most = usize::from(area.height / 3).max(3);
            if skipped.len() + 2 > most {
                let keep = most.saturating_sub(3).max(1);
                let more = skipped.len() - keep;
                skipped.truncate(keep);
                skipped.push(Line::from(format!("{:>9}  and {} more", "", count(more))));
            }
            u16::try_from(skipped.len() + 2).unwrap_or(u16::MAX)
        };

        let found_all = found_lines(rule, home, usize::MAX, width);
        let chart = chart_lines(planned, rule, width);
        let [about_area, found_area, skipped_area, chart_area] = detail_areas(
            area,
            top_height,
            u16::try_from(found_all.len() + 2).unwrap_or(u16::MAX),
            skipped_height,
            u16::try_from(chart.len() + 3).unwrap_or(u16::MAX),
        );

        frame.render_widget(
            Paragraph::new(top).wrap(Wrap { trim: false }).block(
                super::visual::block()
                    .title(format!(" {} ", rule.rule.name))
                    .padding(Padding::horizontal(1)),
            ),
            about_area,
        );

        let room = usize::from(found_area.height.saturating_sub(2));
        frame.render_widget(
            Paragraph::new(found_lines(rule, home, room, width)).block(Self::found_block(rule)),
            found_area,
        );

        if !skipped.is_empty() {
            frame.render_widget(
                Paragraph::new(skipped).block(
                    super::visual::block()
                        .title(
                            Line::from(format!(
                                " Skipped · {} left in place ",
                                count(rule.skipped.len())
                            ))
                            .bold(),
                        )
                        .padding(Padding::horizontal(1)),
                ),
                skipped_area,
            );
        }

        if chart_area.height >= 3 {
            draw_location(frame, chart_area, planned, chart);
        }
    }
}

/// Where the space is: every rule's bar, each risk, and the rules that
/// found nothing
fn draw_location(frame: &mut Frame, area: Rect, planned: &Planned, chart: Vec<Line<'static>>) {
    let mut sections = vec![chart];
    sections.extend(by_risk(planned));
    sections.extend(found_nothing(planned));
    super::visual::sections(frame, area, " Location ", sections);
}

/// The details' boxes: about, found, skipped, and the chart. The found box
/// fits its items, and the chart takes what is left when there is room.
fn detail_areas(area: Rect, top: u16, found: u16, skipped: u16, chart: u16) -> [Rect; 4] {
    let free = area.height.saturating_sub(top).saturating_sub(skipped);
    let found = if free >= found + chart { found } else { free };
    Layout::vertical([
        Constraint::Length(top),
        Constraint::Length(found),
        Constraint::Length(skipped),
        Constraint::Fill(1),
    ])
    .areas(area)
}

/// From this many rows, the details' boxes keep their size whichever rule
/// is selected
const TALL_DETAILS: u16 = 34;

/// Every rule that found something, largest first, with a bar against the
/// largest. The selected rule's name is bold.
fn chart_lines(planned: &Planned, current: &RulePlan, width: usize) -> Vec<Line<'static>> {
    let mut rules: Vec<&RulePlan> = planned
        .plan
        .rules
        .iter()
        .filter(|rule| !rule.items.is_empty())
        .collect();
    rules.sort_by_key(|rule| Reverse(rule.size()));
    let top = rules.first().map_or(0, |rule| rule.size());
    let name_room = width.saturating_sub(11 + ITEM_BAR + 2);
    let total: u64 = rules.iter().map(|rule| rule.size()).sum();
    let mut lines: Vec<Line<'static>> = rules
        .into_iter()
        .map(|rule| {
            let name = Span::raw(format::shorten_middle(&rule.rule.name, name_room));
            let name = if rule.rule.id == current.rule.id {
                name.bold()
            } else {
                name
            };
            let bar = Span::raw(format::bar(rule.size(), top, ITEM_BAR));
            let bar = if rule.selected {
                bar.green()
            } else {
                bar.fg(super::visual::ACCENT)
            };
            Line::from(vec![
                format::size_span(rule.size(), format!("{:>9}  ", format::size(rule.size()))),
                bar,
                Span::raw("  "),
                name,
            ])
        })
        .collect();
    if !lines.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::raw(format!("{:>9}  ", format::size(total))).bold(),
            Span::raw("total · "),
            Span::raw(format::size(planned.plan.selected_size()))
                .green()
                .bold(),
            Span::raw(" selected"),
        ]));
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
        text
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
    let mut spans = vec![Span::raw(format!(
        " {} of {} rules found {} ",
        count(found),
        count(rules.len()),
        format::size(size)
    ))];
    if !planned.errors.is_empty() {
        spans.push(Span::raw(format!("· {} rule problems ", count(planned.errors.len()))).yellow());
    }
    Line::from(spans)
}

/// What is selected so far, rule by rule, with the total
fn selection(planned: &Planned) -> Vec<Line<'static>> {
    let chosen: Vec<&RulePlan> = planned
        .plan
        .rules
        .iter()
        .filter(|rule| rule.selected && !rule.items.is_empty())
        .collect();
    if chosen.is_empty() {
        return vec![
            Line::from("Nothing selected yet.").bold(),
            Line::from("Press Space to pick a rule."),
        ];
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
    lines.push(Line::from(vec![
        Span::raw(format!(
            "{:>9}  ",
            format::size(planned.plan.selected_size())
        ))
        .green()
        .bold(),
        Span::raw(format!("in all, {}", items(planned.plan.selected_count()))).bold(),
    ]));
    lines
}

/// What each risk the rules use means, with how many rules have it
fn risks(planned: &Planned) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("Risk levels").style(super::visual::HEADING)];
    for tier in [Tier::Safe, Tier::Caution, Tier::Expert] {
        let rules = planned
            .plan
            .rules
            .iter()
            .filter(|rule| rule.rule.tier == tier)
            .count();
        if rules == 0 {
            continue;
        }
        let padding = " ".repeat(9 - tier_name(tier).len());
        lines.push(Line::from(vec![
            tier_span(tier).bold(),
            Span::raw(padding),
            Span::raw(tier_meaning(tier)),
        ]));
    }
    lines
}

/// Every rule's finds added up, and what happens next
fn in_all(planned: &Planned) -> Vec<Line<'static>> {
    let rules = &planned.plan.rules;
    let found = rules.iter().filter(|rule| !rule.items.is_empty()).count();
    let found_items: usize = rules.iter().map(|rule| rule.items.len()).sum();
    let size: u64 = rules.iter().map(RulePlan::size).sum();
    let selected = planned.plan.selected_size();
    let selected = Span::raw(format::size(selected));
    let selected = if planned.plan.selected_count() == 0 {
        selected
    } else {
        selected.green().bold()
    };
    vec![
        Line::from("Total").style(super::visual::HEADING),
        wide(
            "Found",
            Span::raw(format!(
                "{} · {} of {} rules · {}",
                format::size(size),
                count(found),
                count(rules.len()),
                items(found_items)
            )),
        ),
        wide("Selected", selected),
        wide("Next", Span::raw("Press Enter to check the list first.")),
        wide(
            "Goes to",
            Span::raw("the Trash, so you can put it back").green(),
        ),
    ]
}

/// A label and its value, with room for longer labels than [`field`]
fn wide(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<10}")).bold(), value])
}

/// Every risk the rules use, with the space its rules found, a bar of it
/// against everything found, and how many of its rules found something
fn by_risk(planned: &Planned) -> Option<Vec<Line<'static>>> {
    let rules = &planned.plan.rules;
    let total: u64 = rules.iter().map(RulePlan::size).sum();
    let mut lines = vec![Line::from("Risk").style(super::visual::HEADING)];
    for tier in [Tier::Safe, Tier::Caution, Tier::Expert] {
        let of_tier: Vec<&RulePlan> = rules.iter().filter(|rule| rule.rule.tier == tier).collect();
        if of_tier.is_empty() {
            continue;
        }
        let size: u64 = of_tier.iter().map(|rule| rule.size()).sum();
        let found = of_tier.iter().filter(|rule| !rule.items.is_empty()).count();
        let bar = Span::raw(format::bar(size, total, ITEM_BAR));
        let bar = match tier {
            Tier::Safe => bar.green(),
            Tier::Caution => bar.yellow(),
            Tier::Expert => bar.red(),
        };
        let padding = " ".repeat(9 - tier_name(tier).len());
        let rules_text = if found == 0 {
            format!("{}, nothing found", count_rules(of_tier.len()))
        } else {
            format!("{} of {} found", count(found), count_rules(of_tier.len()))
        };
        lines.push(Line::from(vec![
            format::size_span(size, format!("{:>9}  ", format::size(size))),
            bar,
            Span::raw("  "),
            tier_span(tier),
            Span::raw(padding),
            Span::raw(rules_text),
        ]));
    }
    (lines.len() > 1).then_some(lines)
}

/// `rules` rules, or 1 rule
fn count_rules(rules: usize) -> String {
    format!(
        "{} {}",
        count(rules),
        if rules == 1 { "rule" } else { "rules" }
    )
}

/// The rules that found nothing, by name, so you know they looked
fn found_nothing(planned: &Planned) -> Option<Vec<Line<'static>>> {
    let names: Vec<&str> = planned
        .plan
        .rules
        .iter()
        .filter(|rule| rule.items.is_empty())
        .map(|rule| rule.rule.name.as_str())
        .collect();
    if names.is_empty() {
        return None;
    }
    Some(vec![
        Line::from(format!(
            "No findings · {} {}",
            count(names.len()),
            if names.len() == 1 { "rule" } else { "rules" }
        ))
        .style(super::visual::HEADING),
        Line::from(names.join(", ")),
    ])
}

/// Every rule as a table, largest first, with `summary` on the bottom edge
fn rules_table(
    planned: &Planned,
    current: usize,
    summary: Line<'static>,
    width: u16,
) -> Table<'static> {
    let counts = format::column_width(
        "Items",
        planned
            .plan
            .rules
            .iter()
            .map(|rule| count(rule.items.len())),
    );
    let sizes = format::column_width(
        "Size",
        planned
            .plan
            .rules
            .iter()
            .map(|rule| format::size(rule.size())),
    );
    let columns = super::visual::Columns::new(
        width,
        [(3, 0), (12, 1), (7, 0), (counts, 0), (sizes, 0)],
        &[],
        true,
    );
    let rows: Vec<Row> = planned
        .plan
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| rule_row(rule, index == current, &columns))
        .collect();
    let header = columns
        .row([
            Cell::from(""),
            Cell::from("Rule"),
            Cell::from("Risk"),
            Cell::from(Line::from("Items").right_aligned()),
            Cell::from(Line::from("Size").right_aligned()),
        ])
        .style(super::visual::HEADING)
        .bottom_margin(1);
    Table::new(rows, columns.widths())
        .header(header)
        .column_spacing(2)
        .block(
            super::visual::block()
                .title(" Deep Clean ")
                .title_bottom(summary)
                .padding(Padding::horizontal(1)),
        )
        .highlight_symbol("▸ ")
        .row_highlight_style(super::visual::SELECTED)
}

impl Screen for Clean {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        self.adopt(context);
        self.poll();
        let planned = match &self.state {
            State::Shared => {
                let started = context
                    .plan
                    .map_or_else(Instant::now, |estimate| estimate.started);
                Loading {
                    title: "Deep Clean",
                    doing: "Finding files the rules cover",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Read-only scan. This happens once, and again when you press r.",
                }
                .draw(frame, area);
                return;
            }
            State::Planning { started, .. } => {
                Loading {
                    title: "Deep Clean",
                    doing: "Finding files the rules cover",
                    progress: format!("{}s", started.elapsed().as_secs()),
                    note: "Read-only scan.",
                }
                .draw(frame, area);
                return;
            }
            State::Failed(reason) => {
                let block = super::visual::block()
                    .title(" Deep Clean ")
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
            super::visual::sections(
                frame,
                cart,
                " Selected ",
                vec![selection(planned), risks(planned), in_all(planned)],
            );
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
        let table = rules_table(planned, current, summary, list_area.width);
        frame.render_stateful_widget(table, list_area, &mut self.list);

        let Some(rule) = planned.plan.rules.get(current) else {
            return;
        };
        let mut notes = problems(&planned.errors);
        if let Some(note) = &self.note {
            notes.insert(0, Line::from(note.clone()).yellow());
            notes.insert(1, Line::default());
        }
        Self::draw_details(frame, detail_area, planned, rule, notes);
        self.draw_typing(frame, area);
    }

    fn handle_key(&mut self, key: KeyEvent, context: &Context) -> Action {
        if self.typing.is_some() {
            self.type_key(key.code);
            return Action::None;
        }
        self.adopt(context);
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
            KeyCode::Char('r') if !matches!(self.state, State::Shared | State::Planning { .. }) => {
                self.state = State::Shared;
                self.list.select(Some(0));
                return Action::Replan;
            }
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
        "↑↓ move · space select · enter review · r look again · esc home · ? help"
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
            ("r", "Look again, such as after removing files yourself"),
            ("Esc", "Back to Home"),
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
            plan: None,
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
                plan: None,
            },
        );
    }

    #[test]
    fn opens_on_the_shared_plan_and_r_looks_again() {
        let dir = fake_home();
        let estimate = Estimate {
            result: None,
            started: Instant::now(),
            planned: Some(Arc::new(planned(&dir))),
            failed: None,
        };
        let scan = super::super::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: Some(&estimate),
        };
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let shared = |estimate: &Estimate| {
            estimate
                .planned
                .as_ref()
                .map(|planned| planned.plan.selected_count())
        };
        let before = shared(&estimate);

        let mut clean = Clean::new();
        let _ = clean.handle_key(key(KeyCode::Char(' ')), &context);

        let State::Ready(mine) = &clean.state else {
            panic!("Clean should open on the shared plan without looking again");
        };
        assert_ne!(Some(mine.plan.selected_count()), before);
        assert_eq!(
            shared(&estimate),
            before,
            "the shared plan should not change"
        );
        assert!(matches!(
            clean.handle_key(key(KeyCode::Char('r')), &context),
            Action::Replan
        ));
        assert!(matches!(clean.state, State::Shared));
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
        assert!(screen.contains("in all, 1 item"));
        assert!(screen.contains("Folder  ~/Library/Developer/Xcode/DerivedData"));
        assert!(screen.contains("App-abc"));
        assert!(screen.contains("Found · 1 item · 4.1 KB"));
        assert!(screen.contains("Location"));
        assert!(screen.contains("16.4 KB  total · 4.1 KB selected"));
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
            plan: None,
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
        assert!(render(&mut clean).contains("in all, 2 items"));

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
            plan: None,
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

        // Hold the plan back until the first frame, or a fast plan can
        // finish before it and skip the loading screen.
        let (go, wait) = mpsc::channel::<()>();
        let mut clean = Clean::start(move || {
            let _ = wait.recv();
            make_plan(&home, None)
        });
        assert!(render(&mut clean).contains("Finding files"));
        go.send(()).expect("plan should still be waiting");
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
    #[test]
    fn a_tall_screen_keeps_the_detail_boxes_in_place() {
        use crate::ui::visual::tests as view;
        let scan = crate::ui::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let dir = fake_home();
        let mut screen = Clean::ready(planned(&dir));
        // The rows where the found box and the chart start
        let edges = |screen: &mut Clean| {
            let text = view::text(&view::render("clean", screen, &context, 160, 46));
            let rows: Vec<&str> = text.lines().collect();
            let at = |needle: &str| rows.iter().position(|row| row.contains(needle));
            (at("┌ Found"), at("┌ Location"))
        };
        let first = edges(&mut screen);
        assert!(first.0.is_some() && first.1.is_some(), "{first:?}");
        for _ in 0..3 {
            press(&mut screen, KeyCode::Down);
            assert_eq!(edges(&mut screen), first);
        }
    }

    #[test]
    fn layout_stays_readable_across_terminal_sizes() {
        use crate::ui::visual::tests as view;
        let scan = crate::ui::scan::ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        let dir = fake_home();
        let mut screen = Clean::ready(planned(&dir));
        for (width, height) in view::SIZES {
            let buffer = view::render("clean", &mut screen, &context, width, height);
            view::aligned(&buffer, "Size", "4.1 KB");
            assert!(view::text(&buffer).contains("Risk"));
        }
    }
}
