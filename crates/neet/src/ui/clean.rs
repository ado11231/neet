use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Instant, SystemTime};

use neet_core::clean::{self, Plan, RulePlan, SkipReason};
use neet_core::rules::{self, RuleError, Tier};
use neet_core::safety::CleanupRoots;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};

use super::app::{Action, Context, Screen};
use super::format;

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

enum State {
    Planning {
        started: Instant,
        result: Receiver<Outcome>,
    },
    Ready(Planned),
    Failed(String),
}

/// Lists what each cleanup rule found, and lets you choose rules.
pub struct Clean {
    state: State,
    list: ListState,
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
            list: ListState::default().with_selected(Some(0)),
        }
    }

    #[cfg(test)]
    fn ready(planned: Planned) -> Self {
        Self::from_state(State::Ready(planned))
    }

    fn poll(&mut self) {
        if let State::Planning { result, .. } = &self.state
            && let Ok(outcome) = result.try_recv()
        {
            self.state = match outcome {
                Ok(planned) => State::Ready(planned),
                Err(reason) => State::Failed(reason),
            };
        }
    }

    fn selected(&self) -> usize {
        self.list.selected().unwrap_or(0)
    }

    fn toggle(&mut self) {
        let index = self.selected();
        let State::Ready(planned) = &mut self.state else {
            return;
        };
        let Some(rule) = planned.plan.rules.get_mut(index) else {
            return;
        };
        // Expert rules need a typed confirmation, which is not built yet.
        if rule.rule.tier != Tier::Expert && !rule.items.is_empty() {
            rule.selected = !rule.selected;
        }
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
    let span = Span::raw(format!("{:<8}", tier_name(tier)));
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
        Tier::Expert => {
            "The files may not exist anywhere else. Selecting it needs a typed confirmation, \
             which is not built yet."
        }
    }
}

/// `path` with the home folder shown as `~`.
fn display_path(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn skip_reason(reason: &SkipReason, min_age_days: u32) -> String {
    match reason {
        SkipReason::Refused(error) => format!("refused: {error}"),
        SkipReason::TooNew => format!("changed in the last {min_age_days} days"),
        SkipReason::Unreadable(error) => format!("could not be read: {error}"),
        SkipReason::Overlaps => "already found by another rule".to_string(),
    }
}

fn count(value: usize) -> String {
    format::count(u64::try_from(value).unwrap_or(u64::MAX))
}

fn rule_row(rule: &RulePlan) -> ListItem<'static> {
    let mark = if rule.selected { "[x] " } else { "[ ] " };
    let items = rule.items.len();
    let found = if items == 0 {
        "nothing found".to_string()
    } else {
        format!(
            "{} {}",
            count(items),
            if items == 1 { "item " } else { "items" }
        )
    };
    let line = Line::from(vec![
        Span::raw(mark),
        Span::raw(format!("{:<24}", rule.rule.name)),
        tier_span(rule.rule.tier),
        Span::raw(format!("{found:>15}")).dark_gray(),
        Span::raw(format!("{:>11}", format::size(rule.size()))),
    ]);
    if items == 0 {
        ListItem::new(line).dark_gray()
    } else {
        ListItem::new(line)
    }
}

/// The selected rule, explained: what it removes, its tier, and every path
/// the dry run found or skipped.
fn details(rule: &RulePlan, home: &Path, room: usize, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(rule.rule.description.clone()),
        Line::from(vec![
            Span::raw(tier_name(rule.rule.tier)).bold(),
            Span::raw(format!(" · {}", tier_meaning(rule.rule.tier))).dark_gray(),
        ]),
    ];
    // Lines that wrap take more than one row.
    let extra_rows: usize = lines
        .iter()
        .map(|line| line.width().saturating_sub(1) / width.max(1))
        .sum();
    if !rule.rule.requires_quit.is_empty() {
        lines.push(
            Line::from(format!(
                "Close first: {}",
                rule.rule.requires_quit.join(", ")
            ))
            .dark_gray(),
        );
    }
    if rule.rule.min_age_days > 0 {
        lines.push(
            Line::from(format!(
                "Keeps anything changed in the last {} days.",
                rule.rule.min_age_days
            ))
            .dark_gray(),
        );
    }
    lines.push(Line::default());

    let mut rows: Vec<Line<'static>> = rule
        .items
        .iter()
        .map(|item| {
            Line::from(vec![
                Span::raw(format!("{:>10}  ", format::size(item.size))),
                Span::raw(display_path(home, item.path.path())),
            ])
        })
        .collect();
    rows.extend(rule.skipped.iter().map(|skipped| {
        Line::from(format!(
            "   skipped  {}  {}",
            display_path(home, &skipped.path),
            skip_reason(&skipped.reason, rule.rule.min_age_days)
        ))
        .dark_gray()
    }));
    if rows.is_empty() {
        let paths: Vec<String> = rule
            .rule
            .paths
            .iter()
            .map(|path| display_path(home, path))
            .collect();
        lines.push(Line::from(format!("Nothing found in {}.", paths.join(", "))).dark_gray());
        return lines;
    }

    let room = room.saturating_sub(lines.len() + extra_rows).max(1);
    if rows.len() > room {
        let more = rows.len() - (room - 1);
        rows.truncate(room - 1);
        rows.push(Line::from(format!("   and {} more", count(more))).dark_gray());
    }
    lines.extend(rows);
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
    let mut spans = vec![
        Span::raw(format!(
            " Selected: {} items, {} ",
            count(plan.selected_count()),
            format::size(plan.selected_size())
        ))
        .bold(),
        Span::raw("· they go to the Trash, where Put Back works ").dark_gray(),
    ];
    if !planned.errors.is_empty() {
        spans.push(
            Span::raw(format!(
                "· {} rule problems, listed below ",
                count(planned.errors.len())
            ))
            .yellow(),
        );
    }
    Line::from(spans)
}

impl Screen for Clean {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        let planned = match &self.state {
            State::Planning { started, .. } => {
                let text = format!(
                    "Finding files the rules cover… {}s\n\nNothing is changed while neet looks.",
                    started.elapsed().as_secs()
                );
                let block = Block::bordered()
                    .title(" Clean ")
                    .padding(Padding::horizontal(1));
                frame.render_widget(Paragraph::new(text).cyan().block(block), area);
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

        let list_height = u16::try_from(planned.plan.rules.len() + 2).unwrap_or(u16::MAX);
        let [list_area, detail_area] = Layout::vertical([
            Constraint::Length(list_height.min(area.height / 2).max(3)),
            Constraint::Fill(1),
        ])
        .areas(area);

        let rows: Vec<ListItem> = planned.plan.rules.iter().map(rule_row).collect();
        let list = List::new(rows)
            .block(
                Block::bordered()
                    .title(" Clean ")
                    .title_bottom(totals(planned))
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .highlight_style(Style::new().bold().cyan());
        frame.render_stateful_widget(list, list_area, &mut self.list);

        let Some(rule) = planned.plan.rules.get(self.selected()) else {
            return;
        };
        let mut lines = problems(&planned.errors);
        let room = usize::from(detail_area.height.saturating_sub(2)).saturating_sub(lines.len());
        let width = usize::from(detail_area.width.saturating_sub(4));
        lines.extend(details(rule, &planned.home, room, width));
        let body = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(format!(" {} ", rule.rule.name))
                .padding(Padding::horizontal(1)),
        );
        frame.render_widget(body, detail_area);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
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
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · space select · esc home · ? help · q quit"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between rules"),
            ("g  G", "Jump to the first or last rule"),
            ("Space", "Select or clear a rule"),
            ("Enter", "Review the paths. Not built yet"),
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

        let screen = render(&mut clean);

        assert!(screen.contains("[x] Xcode DerivedData"));
        assert!(screen.contains("[ ] npm cache"));
        assert!(screen.contains("caution"));
        assert!(screen.contains("nothing found"));
        assert!(screen.contains("Selected: 1 items"));
        assert!(screen.contains("~/Library/Developer/Xcode/DerivedData/App-abc"));
    }

    #[test]
    fn space_selects_a_rule_that_found_something() {
        let dir = fake_home();
        let mut clean = Clean::ready(planned(&dir));

        select_rule(&mut clean, "npm-cache");
        press(&mut clean, KeyCode::Char(' '));
        assert!(is_selected(&clean, "npm-cache"));
        assert!(render(&mut clean).contains("Selected: 2 items"));

        press(&mut clean, KeyCode::Char(' '));
        assert!(!is_selected(&clean, "npm-cache"));
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
