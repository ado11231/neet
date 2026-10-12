//! Browse local assistant files and review skill changes.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use neet_core::ai::skills::{Edit, Install};
use neet_core::ai::{self, Item, Kind, Listing, Locations, Provider};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState, Padding, Paragraph, Wrap};
use similar::{ChangeTag, TextDiff};

use super::app::{Action, Context, Screen};
use super::{format, visual};

const TABS: [&str; 3] = ["Files", "Chats", "Skills"];
const INSTALL_TARGETS: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Gemini];

enum Input {
    Search,
    Install { path: String, target: usize },
}

enum Review {
    Install(Install),
    Edit { edit: Edit, contents: Vec<u8> },
}

struct Viewer {
    title: String,
    lines: Vec<Line<'static>>,
    scroll: u16,
}

impl Viewer {
    fn new(title: String, text: &str) -> Self {
        Self {
            title,
            lines: ai::display_text(text)
                .lines()
                .map(|line| Line::from(line.to_owned()))
                .collect(),
            scroll: 0,
        }
    }
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let block = visual::block()
            .title(format!(" {} ", ai::display_text(&self.title)))
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        let max = visual::wrapped_rows(&self.lines, inner.width).saturating_sub(inner.height);
        self.scroll = self.scroll.min(max);
        frame.render_widget(
            Paragraph::new(self.lines.clone())
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(block),
            area,
        );
    }
}

pub struct AiTools {
    locations: Locations,
    listing: Listing,
    pending: Option<Receiver<Listing>>,
    list: ListState,
    tab: usize,
    filter: String,
    provider: Option<Provider>,
    input: Option<Input>,
    viewer: Option<Viewer>,
    review: Option<Review>,
    editing: Option<Edit>,
    note: String,
}

impl AiTools {
    fn shown_path(&self, path: &std::path::Path) -> String {
        let text = path.strip_prefix(&self.locations.home).map_or_else(
            |_| path.display().to_string(),
            |relative| format!("~/{}", relative.display()),
        );
        ai::display_text(&text)
    }

    pub fn new() -> Self {
        let mut screen = Self::at(Locations::from_env());
        screen.refresh();
        screen
    }

    fn at(locations: Locations) -> Self {
        Self {
            locations,
            listing: Listing::default(),
            pending: None,
            list: ListState::default().with_selected(Some(0)),
            tab: 0,
            filter: String::new(),
            provider: None,
            input: None,
            viewer: None,
            review: None,
            editing: None,
            note: String::new(),
        }
    }

    fn refresh(&mut self) {
        let (sender, receiver) = mpsc::channel();
        let locations = self.locations.clone();
        thread::spawn(move || {
            let _ = sender.send(locations.list());
        });
        self.pending = Some(receiver);
        self.list.select(Some(0));
    }

    fn poll(&mut self) {
        let Some(receiver) = &self.pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(listing) => {
                self.listing = listing;
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.note = "Discovery stopped unexpectedly. Press r to try again.".into();
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    fn rows(&self) -> Vec<&Item> {
        let filter = self.filter.to_lowercase();
        self.listing
            .items
            .iter()
            .filter(|item| {
                let category = match self.tab {
                    1 => item.kind == Kind::Chat,
                    2 => item.kind == Kind::Skill,
                    _ => matches!(item.kind, Kind::Settings | Kind::Instructions),
                };
                category
                    && self
                        .provider
                        .is_none_or(|provider| item.provider == provider)
                    && format!(
                        "{} {} {}",
                        item.path.display(),
                        item.provider.name(),
                        item.scope
                    )
                    .to_lowercase()
                    .contains(&filter)
            })
            .collect()
    }

    fn selected(&self) -> Option<Item> {
        self.rows()
            .get(self.list.selected().unwrap_or(0))
            .map(|item| (*item).clone())
    }

    fn step(&mut self, down: bool) {
        let count = self.rows().len();
        if count == 0 {
            return;
        }
        let current = self.list.selected().unwrap_or(0).min(count - 1);
        self.list.select(Some(if down {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        }));
    }

    fn open(&mut self) {
        if let Some(item) = self.selected() {
            match self.locations.preview(&item) {
                Ok(text) => self.viewer = Some(Viewer::new(self.shown_path(&item.path), &text)),
                Err(error) => self.note = error.to_string(),
            }
        }
    }

    fn edit(&mut self, restore: bool) -> Action {
        let Some(item) = self.selected() else {
            return Action::None;
        };
        match Edit::begin(&self.locations, &item) {
            Ok(edit) if restore => match edit.latest_backup() {
                Ok(contents) => self.review_edit(edit, contents),
                Err(error) => self.note = error.to_string(),
            },
            Ok(edit) => {
                let path = edit.draft();
                self.editing = Some(edit);
                return Action::Edit(path);
            }
            Err(error) => self.note = error.to_string(),
        }
        Action::None
    }

    fn review_edit(&mut self, edit: Edit, contents: Vec<u8>) {
        if contents == edit.contents() {
            self.note = "No changes to save.".into();
            return;
        }
        let old = String::from_utf8_lossy(edit.contents());
        let new = String::from_utf8_lossy(&contents);
        let mut viewer = Viewer::new(
            "Review skill change".into(),
            &format!(
                "{}\n\ny saves this change with a backup. Esc cancels.\n",
                self.shown_path(edit.path())
            ),
        );
        for change in TextDiff::from_lines(old.as_ref(), new.as_ref()).iter_all_changes() {
            let text = ai::display_text(change.value());
            let line = match change.tag() {
                ChangeTag::Delete => Line::from(format!("- {}", text.trim_end())).red(),
                ChangeTag::Insert => Line::from(format!("+ {}", text.trim_end())).green(),
                ChangeTag::Equal => Line::from(format!("  {}", text.trim_end())),
            };
            viewer.lines.push(line);
        }
        self.viewer = Some(viewer);
        self.review = Some(Review::Edit { edit, contents });
    }

    fn prepare_install(&mut self, path: &str, target: usize) {
        let source = path.strip_prefix("~/").map_or_else(
            || PathBuf::from(path),
            |rest| self.locations.home.join(rest),
        );
        match Install::prepare(&self.locations, &source, INSTALL_TARGETS[target]) {
            Ok(install) => {
                let text = format!(
                    "Link {}\nto {}\n\nThe assistant can load this skill and its scripts.\nChanges to the source also change the loaded skill.\nRestart the assistant or reload its skills after loading.\ny loads this skill. Esc cancels.\n\n{}",
                    self.shown_path(install.source()),
                    self.shown_path(install.destination()),
                    String::from_utf8_lossy(install.contents())
                );
                self.viewer = Some(Viewer::new("Review local skill".into(), &text));
                self.review = Some(Review::Install(install));
            }
            Err(error) => self.note = error.to_string(),
        }
    }

    fn apply_review(&mut self) {
        let Some(review) = self.review.take() else {
            return;
        };
        self.note = match review {
            Review::Install(install) => match install.apply() {
                Ok(()) => "Skill loaded. Restart the assistant or reload its skills.".into(),
                Err(error) => error.to_string(),
            },
            Review::Edit { edit, contents } => match edit.save(&contents) {
                Ok(backup) => format!("Skill saved. Backup: {}", self.shown_path(&backup)),
                Err(error) => error.to_string(),
            },
        };
        self.viewer = None;
        self.refresh();
    }

    fn input_key(&mut self, key: KeyCode) {
        let Some(mut input) = self.input.take() else {
            return;
        };
        match &mut input {
            Input::Search => match key {
                KeyCode::Enter => return,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => self.filter.push(c),
                _ => {}
            },
            Input::Install { path, target } => match key {
                KeyCode::Tab => *target = (*target + 1) % INSTALL_TARGETS.len(),
                KeyCode::Enter => {
                    self.prepare_install(path.trim(), *target);
                    return;
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Char(c) => path.push(c),
                _ => {}
            },
        }
        self.list.select(Some(0));
        self.input = Some(input);
    }

    fn reveal(&mut self) {
        if let Some(item) = self.selected() {
            match std::process::Command::new("/usr/bin/open")
                .arg("-R")
                .arg(item.path)
                .status()
            {
                Ok(status) if status.success() => self.note = "Revealed in Finder.".into(),
                Ok(_) => self.note = "Finder could not reveal this file.".into(),
                Err(error) => self.note = error.to_string(),
            }
        }
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.rows();
        let items: Vec<_> = rows
            .iter()
            .map(|item| {
                let path = if item.kind == Kind::Skill {
                    item.path.parent().unwrap_or(&item.path)
                } else {
                    &item.path
                };
                ListItem::new(vec![
                    Line::from(format!("{} · {}", item.provider.name(), item.kind.name()))
                        .style(visual::HEADING),
                    Line::from(format::shorten_middle(
                        &self.shown_path(path),
                        usize::from(area.width.saturating_sub(6)),
                    )),
                ])
            })
            .collect();
        let count = items.len();
        let block = visual::block()
            .title(format!(" {} · {count} ", TABS[self.tab]))
            .padding(Padding::horizontal(1));
        if count == 0 {
            frame.render_widget(Paragraph::new("No matching files.\n\nr refreshes the list.\ni loads a local skill folder.\n\nLooks in user folders and the current project's parent folders up to its Git root.").wrap(Wrap { trim: false }).block(block), area);
        } else {
            frame.render_stateful_widget(
                List::new(items)
                    .block(block)
                    .highlight_symbol("▸ ")
                    .highlight_style(visual::SELECTED),
                area,
                &mut self.list,
            );
        }
    }

    fn details(&self) -> Vec<Line<'static>> {
        let Some(item) = self.selected() else {
            return vec![
                Line::from("Claude Code · Codex · Gemini CLI"),
                Line::from("Shared skills: ~/.agents/skills"),
            ];
        };
        let mut lines = vec![
            Line::from(format!("{} · {}", item.provider.name(), item.kind.name()))
                .style(visual::HEADING),
            Line::from(ai::display_text(&self.shown_path(&item.path))),
            Line::from(ai::display_text(&item.scope)),
            Line::from(format!("{} on disk", format::size(item.size))),
            Line::default(),
        ];
        lines.push(Line::from(match item.kind {
            Kind::Chat => "Enter reads this conversation. Chat files stay view only.",
            Kind::Settings => "Enter shows settings, which may include tokens. View only.",
            Kind::Instructions => "Enter shows instructions. View only.",
            Kind::Skill if item.bundled => "Bundled skill. Enter previews it. View only.",
            Kind::Skill => "Enter previews · e edits · b restores the latest backup. Every change is reviewed first.",
        }));
        lines.push(Line::from("o reveals the file in Finder."));
        lines
    }
}

impl Screen for AiTools {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _: &Context) {
        self.poll();
        if self.pending.is_some() {
            super::loading::Loading {
                title: "AI Tools",
                doing: "Finding assistant files",
                progress: "Claude Code, Codex, Gemini CLI, and shared skills".into(),
                note: "Reading file names and sizes.",
            }
            .draw(frame, area);
            return;
        }
        if let Some(viewer) = &mut self.viewer {
            viewer.draw(frame, area);
            return;
        }
        let [header, body, status] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(4),
        ])
        .areas(area);
        let tabs = TABS
            .iter()
            .enumerate()
            .map(|(index, name)| {
                if index == self.tab {
                    format!("[{name}]")
                } else {
                    (*name).into()
                }
            })
            .collect::<Vec<String>>()
            .join("   ");
        frame.render_widget(
            Paragraph::new(format!(
                "{tabs}   · {}",
                self.provider.map_or("All tools", Provider::name)
            ))
            .block(
                visual::block()
                    .title(" AI Tools ")
                    .padding(Padding::horizontal(1)),
            ),
            header,
        );
        let [list, details] = if area.width >= 96 {
            Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)]).areas(body)
        } else {
            let detail_height = body.height.saturating_sub(6).min(8);
            Layout::vertical([Constraint::Fill(1), Constraint::Length(detail_height)]).areas(body)
        };
        self.draw_list(frame, list);
        frame.render_widget(
            Paragraph::new(self.details())
                .wrap(Wrap { trim: false })
                .block(
                    visual::block()
                        .title(" Selected ")
                        .padding(Padding::horizontal(1)),
                ),
            details,
        );
        let text = match &self.input {
            Some(Input::Install { path, target }) => format!(
                "Load into {} · Tab changes tool\nLocal skill folder: {path}▏",
                INSTALL_TARGETS[*target].name()
            ),
            Some(Input::Search) => format!("Find: {}▏", self.filter),
            None if !self.note.is_empty() => self.note.clone(),
            None => format!(
                "Filter: {} · {} skipped paths (w shows details)\nCredentials and databases are excluded.",
                if self.filter.is_empty() {
                    "none"
                } else {
                    &self.filter
                },
                self.listing.warnings.len()
            ),
        };
        frame.render_widget(
            Paragraph::new(ai::display_text(&text))
                .wrap(Wrap { trim: false })
                .block(
                    visual::block()
                        .title(" Status ")
                        .padding(Padding::horizontal(1)),
                ),
            status,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, _: &Context) -> Action {
        self.poll();
        if self.pending.is_some() {
            return Action::None;
        }
        if self.input.is_some() {
            self.input_key(key.code);
            return Action::None;
        }
        if let Some(viewer) = &mut self.viewer {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    viewer.scroll = viewer.scroll.saturating_add(1);
                }
                KeyCode::Up | KeyCode::Char('k') => viewer.scroll = viewer.scroll.saturating_sub(1),
                KeyCode::PageDown => viewer.scroll = viewer.scroll.saturating_add(10),
                KeyCode::PageUp => viewer.scroll = viewer.scroll.saturating_sub(10),
                KeyCode::Home => viewer.scroll = 0,
                KeyCode::End => viewer.scroll = u16::MAX,
                KeyCode::Char('y') => self.apply_review(),
                _ => {}
            }
            return Action::None;
        }
        self.note.clear();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Tab | KeyCode::Right => {
                self.tab = (self.tab + 1) % TABS.len();
                self.list.select(Some(0));
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.tab = (self.tab + TABS.len() - 1) % TABS.len();
                self.list.select(Some(0));
            }
            KeyCode::Char('p') => {
                self.provider = match self.provider {
                    None => Some(Provider::Claude),
                    Some(Provider::Claude) => Some(Provider::Codex),
                    Some(Provider::Codex) => Some(Provider::Gemini),
                    Some(Provider::Gemini) => Some(Provider::Shared),
                    Some(Provider::Shared) => None,
                };
                self.list.select(Some(0));
            }
            KeyCode::Char('/') => self.input = Some(Input::Search),
            KeyCode::Char('i') => {
                self.input = Some(Input::Install {
                    path: String::new(),
                    target: 0,
                });
            }
            KeyCode::Char('e') => return self.edit(false),
            KeyCode::Char('b') => return self.edit(true),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('o') => self.reveal(),
            KeyCode::Char('w') => {
                self.viewer = Some(Viewer::new(
                    "Skipped paths".into(),
                    &self.listing.warnings.join("\n\n"),
                ));
            }
            KeyCode::Enter => self.open(),
            _ => {}
        }
        Action::None
    }

    fn edited(&mut self, result: Result<(), String>) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        match result {
            Err(error) => self.note = error,
            Ok(()) => match edit.edited() {
                Ok(contents) => self.review_edit(edit, contents),
                Err(error) => self.note = error.to_string(),
            },
        }
    }

    fn back(&mut self) -> Action {
        if self.input.take().is_some() {
            self.filter.clear();
            return Action::None;
        }
        if self.viewer.take().is_some() {
            self.review = None;
            return Action::None;
        }
        Action::Back
    }

    fn takes_text(&self) -> bool {
        self.input.is_some()
    }
    fn is_dialog(&self) -> bool {
        self.review.is_some()
    }

    fn hints(&self) -> &'static str {
        if self.input.is_some() {
            "Enter accept · Tab tool · Esc cancel"
        } else if self.review.is_some() {
            "↑↓ scroll · PgUp/PgDn page · y confirm · Esc cancel · ? help"
        } else if self.viewer.is_some() {
            "↑↓ scroll · PgUp/PgDn page · Esc back · ? help · q quit"
        } else {
            "↑↓ move · Tab section · Enter read · / find · p tool · i load · e edit · b restore · o Finder · r refresh · Esc back · ? help"
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("Tab / Shift-Tab", "Switch Files, Chats, and Skills."),
            ("p", "Filter by tool, including Shared (.agents/skills)."),
            (
                "/",
                "Find by path, tool, or project. Esc clears the filter.",
            ),
            ("Enter", "Read the selected file or conversation locally."),
            (
                "i",
                "Load a local skill folder. Tab picks Claude, Codex, or Gemini.",
            ),
            (
                "e",
                "Edit your SKILL.md in $VISUAL or $EDITOR, then review.",
            ),
            ("b", "Review restoring the latest skill backup."),
            ("y", "Confirm the skill change on the review screen."),
            ("o", "Reveal the selected file in Finder."),
            ("r / w", "Refresh files / show skipped paths."),
            (
                "Esc",
                "Cancel input or review, close the preview, or go back.",
            ),
        ]
    }
}

#[cfg(test)]
mod tests;
