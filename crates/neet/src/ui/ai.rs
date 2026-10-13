//! Browse local assistant files and review skill changes.

mod chats;

use std::collections::BTreeSet;
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
    Chats(Vec<ai::remove::Removal>),
    Install {
        install: Install,
        provider: Provider,
    },
    Edit {
        edit: Edit,
        contents: Vec<u8>,
    },
}

struct Viewer {
    title: String,
    lines: Vec<Line<'static>>,
    scroll: u16,
    source: Option<PathBuf>,
    summary: Vec<Line<'static>>,
    footer: Vec<Line<'static>>,
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
            source: None,
            summary: Vec::new(),
            footer: Vec::new(),
        }
    }
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let area = if self.summary.is_empty() {
            area
        } else {
            let width = area.width.saturating_sub(4);
            let [top, body, bottom] = Layout::vertical([
                Constraint::Length(visual::wrapped_rows(&self.summary, width) + 2),
                Constraint::Min(3),
                Constraint::Length(visual::wrapped_rows(&self.footer, width) + 2),
            ])
            .areas(area);
            for (area, title, lines) in [
                (top, self.title.as_str(), &self.summary),
                (bottom, "Next", &self.footer),
            ] {
                frame.render_widget(
                    Paragraph::new(lines.clone())
                        .wrap(Wrap { trim: false })
                        .block(
                            visual::block()
                                .title(format!(" {title} "))
                                .padding(Padding::horizontal(1)),
                        ),
                    area,
                );
            }
            body
        };
        let block = visual::block()
            .title(if self.summary.is_empty() {
                format!(" {} ", ai::display_text(&self.title))
            } else {
                " Chat files ".to_string()
            })
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
    reselect: Option<PathBuf>,
    marked: BTreeSet<PathBuf>,
    tab: usize,
    filter: String,
    provider: Option<Provider>,
    input: Option<Input>,
    viewer: Option<Viewer>,
    review: Option<Review>,
    editing: Option<Edit>,
    cleanup: Option<chats::Cleanup>,
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
            reselect: None,
            marked: BTreeSet::new(),
            tab: 0,
            filter: String::new(),
            provider: None,
            input: None,
            viewer: None,
            review: None,
            editing: None,
            cleanup: None,
            note: String::new(),
        }
    }

    fn refresh(&mut self) {
        self.refresh_select(self.selected().map(|item| item.path));
    }

    fn refresh_select(&mut self, selected: Option<PathBuf>) {
        self.reselect = selected;
        let (sender, receiver) = mpsc::channel();
        let locations = self.locations.clone();
        let chats = self.tab == 1;
        thread::spawn(move || {
            let mut listing = locations.list();
            if chats {
                listing.summarize_chats(&locations);
            }
            let _ = sender.send(listing);
        });
        self.pending = Some(receiver);
        self.list.select(Some(0));
    }

    fn poll(&mut self) {
        self.poll_cleanup();
        let Some(receiver) = &self.pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(listing) => {
                self.listing = listing;
                self.marked.retain(|path| {
                    self.listing
                        .items
                        .iter()
                        .any(|item| item.kind == Kind::Chat && &item.path == path)
                });
                self.pending = None;
                let selected = self
                    .reselect
                    .take()
                    .and_then(|path| self.rows().iter().position(|item| item.path == path))
                    .unwrap_or(0);
                self.list.select(Some(selected));
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
                    && self.provider.is_none_or(|provider| {
                        item.provider == provider
                            || (item.provider == Provider::Shared
                                && matches!(provider, Provider::Codex | Provider::Gemini))
                    })
                    && format!(
                        "{} {} {} {} {}",
                        item.path.display(),
                        self.shown_path(&item.path),
                        item.provider.name(),
                        item.scope,
                        item.chat.as_ref().map_or_else(String::new, |chat| format!(
                            "{} {}",
                            chat.title,
                            chat.project_label()
                        ))
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

    fn toggle_chat(&mut self) {
        if let Some(item) = self.selected().filter(|item| item.kind == Kind::Chat)
            && !self.marked.remove(&item.path)
        {
            self.marked.insert(item.path);
        }
    }

    fn toggle_all_chats(&mut self) {
        let paths: Vec<_> = self
            .rows()
            .iter()
            .filter(|item| item.kind == Kind::Chat)
            .map(|item| item.path.clone())
            .collect();
        let clear = paths.iter().all(|path| self.marked.contains(path));
        for path in paths {
            if clear {
                self.marked.remove(&path);
            } else {
                self.marked.insert(path);
            }
        }
    }

    fn change_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.list.select(Some(0));
        if tab == 1
            && self
                .listing
                .items
                .iter()
                .any(|item| item.kind == Kind::Chat && item.chat.is_none())
        {
            self.refresh();
        }
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
                Ok(text) => {
                    let metadata = format!(
                        "{} · {}\n{}\n{} · {}\n\n{text}",
                        item.provider.name(),
                        item.kind.name(),
                        self.shown_path(&item.path),
                        item.scope,
                        format::size(item.size)
                    );
                    let metadata = item.chat.as_ref().map_or(metadata.clone(), |chat| {
                        format!("{}\n{}\n\n{metadata}", chat.title, chat.project_label())
                    });
                    let mut viewer = Viewer::new("File preview".into(), &metadata);
                    viewer.source = Some(item.path);
                    self.viewer = Some(viewer);
                }
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
        if let Err(error) = std::fs::write(edit.draft(), &contents) {
            self.draft_error(edit, &format!("Could not keep the draft: {error}"));
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

    fn prepare_install(&mut self, path: &str, target: usize) -> bool {
        let source = path.strip_prefix("~/").map_or_else(
            || PathBuf::from(path),
            |rest| self.locations.home.join(rest),
        );
        match Install::prepare(&self.locations, &source, INSTALL_TARGETS[target]) {
            Ok(install) => {
                let text = format!(
                    "Link {}\nto {}\n\nThe source must stay in this folder. The assistant can load its instructions and scripts.\nChanges to the source also change the loaded skill.\nRestart the assistant or reload its skills after loading.\ny loads this skill. Esc cancels.\n\n{}",
                    self.shown_path(install.source()),
                    self.shown_path(install.destination()),
                    String::from_utf8_lossy(install.contents())
                );
                self.viewer = Some(Viewer::new("Review local skill".into(), &text));
                self.review = Some(Review::Install {
                    install,
                    provider: INSTALL_TARGETS[target],
                });
                true
            }
            Err(error) => {
                self.note = error.to_string();
                false
            }
        }
    }

    fn apply_review(&mut self) {
        let Some(review) = self.review.take() else {
            return;
        };
        let mut selected = self.selected().map(|item| item.path);
        self.note = match review {
            Review::Chats(removals) => {
                self.start_chat_removal(removals);
                return;
            }
            Review::Install { install, provider } => match install.apply() {
                Ok(()) => {
                    selected = Some(install.destination().join("SKILL.md"));
                    self.tab = 2;
                    self.provider = Some(provider);
                    self.filter.clear();
                    "Skill linked. Restart the assistant or reload its skills.".into()
                }
                Err(error) => error.to_string(),
            },
            Review::Edit { edit, contents } => match edit.save(&contents) {
                Ok(backup) => format!("Skill saved. Backup: {}", self.shown_path(&backup)),
                Err(error) => {
                    self.draft_error(edit, &error.to_string());
                    return;
                }
            },
        };
        self.viewer = None;
        self.refresh_select(selected);
    }

    fn draft_error(&mut self, edit: Edit, error: &str) {
        self.viewer = Some(Viewer::new(
            "Skill draft kept".into(),
            &format!(
                "{error}\n\nDraft: {}\n\ne reopens this draft. Esc discards it.\nIf the original changed, copy your work before discarding and open the skill again.",
                edit.draft().display()
            ),
        ));
        self.editing = Some(edit);
    }

    fn input_key(&mut self, key: KeyCode) {
        let Some(mut input) = self.input.take() else {
            return;
        };
        self.note.clear();
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
                KeyCode::BackTab => {
                    *target = (*target + INSTALL_TARGETS.len() - 1) % INSTALL_TARGETS.len();
                }
                KeyCode::Enter => {
                    if self.prepare_install(path.trim(), *target) {
                        return;
                    }
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
        let path = self
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.source.clone())
            .or_else(|| self.selected().map(|item| item.path));
        if let Some(path) = path {
            let result = std::process::Command::new("/usr/bin/open")
                .arg("-R")
                .arg(path)
                .status();
            self.note = match result {
                Ok(status) if status.success() => "Revealed in Finder.".into(),
                Ok(_) => "Finder could not reveal this file.".into(),
                Err(error) => error.to_string(),
            };
            if let Some(viewer) = &mut self.viewer {
                viewer.lines.push(Line::default());
                viewer.lines.push(Line::from(self.note.clone()));
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
                let title = item
                    .chat
                    .as_ref()
                    .filter(|chat| !chat.title.is_empty())
                    .map_or_else(
                        || {
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        },
                        |chat| chat.title.clone(),
                    );
                let subtitle = item.chat.as_ref().map_or_else(
                    || self.shown_path(path),
                    ai::summary::Summary::project_label,
                );
                let marker = if item.kind == Kind::Chat {
                    if self.marked.contains(&item.path) {
                        "[x] "
                    } else {
                        "[ ] "
                    }
                } else {
                    ""
                };
                ListItem::new(vec![
                    Line::from(format::shorten_middle(
                        &ai::display_text(&format!("{marker}{} · {}", title, item.provider.name()))
                            .replace('\n', " "),
                        usize::from(area.width.saturating_sub(6)),
                    ))
                    .style(visual::HEADING),
                    Line::from(format::shorten_middle(
                        &subtitle,
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
            let text = if !self.filter.is_empty() {
                "No matches. Esc clears the search; p changes the tool."
            } else if self.provider.is_some() {
                "No files for this tool. p changes the tool."
            } else {
                match self.tab {
                    1 => "No saved chats found. r looks again.",
                    2 => "No skills found. i links a local skill folder.",
                    _ => "No settings or instructions found. r looks again.",
                }
            };
            frame.render_widget(
                Paragraph::new(text).wrap(Wrap { trim: false }).block(block),
                area,
            );
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

    fn status_lines(&self, width: u16) -> Vec<Line<'static>> {
        let short =
            |text: &str| format::shorten_middle(&ai::display_text(text), usize::from(width));
        let mut lines = match &self.input {
            Some(Input::Install { path, target }) => vec![
                Line::from(format!(
                    "Link to {} · Tab changes tool",
                    INSTALL_TARGETS[*target].name()
                )),
                Line::from(format!(
                    "{}▏",
                    format::shorten_middle(
                        &ai::display_text(path),
                        usize::from(width.saturating_sub(1))
                    )
                )),
                Line::from(short(&format!(
                    "Destination: {}",
                    self.shown_path(&self.locations.skill_root(INSTALL_TARGETS[*target]))
                ))),
            ],
            Some(Input::Search) => vec![Line::from(format!(
                "{}▏",
                short(&format!("Find: {}", self.filter))
            ))],
            None => vec![Line::from(short(&format!(
                "Find: {} · {} skipped paths · w details",
                if self.filter.is_empty() {
                    "all"
                } else {
                    &self.filter
                },
                self.listing.warnings.len()
            )))],
        };
        if self.tab == 1 && self.input.is_none() {
            let visible = self
                .rows()
                .iter()
                .filter(|item| self.marked.contains(&item.path))
                .count();
            let hidden = self.marked.len().saturating_sub(visible);
            lines.push(Line::from(format!(
                "{} selected · {hidden} hidden by filters",
                self.marked.len()
            )));
        }
        if !self.note.is_empty() {
            lines.extend(
                ai::display_text(&self.note)
                    .lines()
                    .map(|line| Line::from(line.to_owned())),
            );
        } else if self.input.is_none() {
            lines.push(Line::from(
                "Enter previews the selected file. ? shows all actions.",
            ));
        }
        lines
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
            Line::from(format!("{} file size", format::size(item.size))),
            Line::default(),
        ];
        if let Some(chat) = &item.chat {
            lines.insert(1, Line::from(chat.title.clone()));
            lines.insert(2, Line::from(chat.project_label()));
            if !chat.note.is_empty() {
                lines.push(Line::from(chat.note.clone()));
            }
            if let Some(age) = item.modified.and_then(|time| time.elapsed().ok()) {
                lines.push(Line::from(format!("Updated {}", format::age(age))));
            }
        }
        lines.push(Line::from(match item.kind {
            Kind::Chat => "Enter reads · Space selects · a selects all shown · d reviews removal.",
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
        if let Some(cleanup) = &self.cleanup {
            super::loading::Loading {
                title: "AI Tools",
                doing: "Moving chat files to Trash",
                progress: format!("{} of {} checked", cleanup.done, cleanup.total),
                note: "Changed or open files are skipped. Finder can restore moved files.",
            }
            .draw(frame, area);
            return;
        }
        if self.pending.is_some() {
            super::loading::Loading {
                title: "AI Tools",
                doing: "Finding assistant files",
                progress: "Claude Code, Codex, Gemini CLI, and shared skills".into(),
                note: "Reading file names and sizes. Chats also show local summaries.",
            }
            .draw(frame, area);
            return;
        }
        if let Some(viewer) = &mut self.viewer {
            viewer.draw(frame, area);
            return;
        }
        let status_lines = self.status_lines(area.width.saturating_sub(4));
        let status_height = visual::wrapped_rows(&status_lines, area.width.saturating_sub(4))
            .saturating_add(2)
            .min(area.height.saturating_sub(9).max(3));
        let [header, body, status] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(status_height),
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
        frame.render_widget(
            Paragraph::new(status_lines)
                .wrap(Wrap { trim: false })
                .block(
                    visual::block()
                        .title(if matches!(self.input, Some(Input::Install { .. })) {
                            " Local skill folder "
                        } else {
                            " Status "
                        })
                        .padding(Padding::horizontal(1)),
                ),
            status,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, _: &Context) -> Action {
        self.poll();
        if self.pending.is_some() || self.cleanup.is_some() {
            return Action::None;
        }
        if self.input.is_some() {
            self.input_key(key.code);
            return Action::None;
        }
        if let Some(edit) = &self.editing
            && key.code == KeyCode::Char('e')
        {
            return Action::Edit(edit.draft());
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
                KeyCode::Char('n') if matches!(self.review, Some(Review::Chats(_))) => {
                    self.back();
                }
                KeyCode::Char('o') if viewer.source.is_some() => self.reveal(),
                _ => {}
            }
            return Action::None;
        }
        self.note.clear();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Tab | KeyCode::Right => {
                self.change_tab((self.tab + 1) % TABS.len());
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.change_tab((self.tab + TABS.len() - 1) % TABS.len());
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
            KeyCode::Char(' ') if self.tab == 1 => self.toggle_chat(),
            KeyCode::Char('a') if self.tab == 1 => self.toggle_all_chats(),
            KeyCode::Char('n') if self.tab == 1 => self.marked.clear(),
            KeyCode::Char('d') | KeyCode::Delete if self.tab == 1 => self.review_chat_removal(),
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
                    &if self.listing.warnings.is_empty() {
                        "No paths were skipped.".into()
                    } else {
                        self.listing.warnings.join("\n\n")
                    },
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
        self.viewer = None;
        match result.and_then(|()| edit.edited().map_err(|error| error.to_string())) {
            Ok(contents) => self.review_edit(edit, contents),
            Err(error) => self.draft_error(edit, &error),
        }
    }

    fn back(&mut self) -> Action {
        if self.cleanup.is_some() {
            return Action::None;
        }
        if let Some(input) = self.input.take() {
            if matches!(input, Input::Search) {
                self.filter.clear();
                self.list.select(Some(0));
            }
            self.note.clear();
            return Action::None;
        }
        if self.viewer.take().is_some() {
            self.review = None;
            if self.editing.take().is_some() {
                self.note = "Draft discarded.".into();
            }
            return Action::None;
        }
        if !self.filter.is_empty() {
            self.filter.clear();
            self.list.select(Some(0));
            return Action::None;
        }
        Action::Back
    }

    fn takes_text(&self) -> bool {
        self.input.is_some()
    }
    fn is_dialog(&self) -> bool {
        self.review.is_some() || self.editing.is_some() || self.cleanup.is_some()
    }

    fn hints(&self) -> &'static str {
        if self.cleanup.is_some() {
            "Moving chat files to Trash. Please wait."
        } else if matches!(self.input, Some(Input::Search)) {
            "Enter keep filter · Esc clear"
        } else if self.input.is_some() {
            "Enter review · Tab tool · Esc cancel"
        } else if self.editing.is_some() {
            "e reopen draft · ↑↓ scroll · Esc discard · ? help"
        } else if matches!(self.review, Some(Review::Chats(_))) {
            "↑↓ scroll · PgUp/PgDn page · y move to Trash · n / Esc cancel"
        } else if self.review.is_some() {
            "↑↓ scroll · PgUp/PgDn page · y confirm · Esc cancel · ? help"
        } else if self
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.source.is_some())
        {
            "↑↓ scroll · PgUp/PgDn page · o Finder · Esc back · ? help · q quit"
        } else if self.viewer.is_some() {
            "↑↓ scroll · PgUp/PgDn page · Esc back · ? help · q quit"
        } else if self.tab == 1 {
            "↑↓ move · Enter read · Space select · a all shown · d delete · / find · p tool · Tab section · ? help · Esc back"
        } else if self.tab == 2 {
            "↑↓ move · Tab section · Enter read · i link skill · e edit · b restore · / find · p tool · ? help · Esc back"
        } else {
            "↑↓ move · Tab section · Enter read · / find · p tool · o Finder · r refresh · ? help · Esc back"
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("Tab / Shift-Tab", "Switch Files, Chats, and Skills."),
            (
                "p",
                "Filter by tool. Codex and Gemini include shared skills.",
            ),
            (
                "/",
                "Find by path, tool, or project. Esc clears the filter.",
            ),
            ("Enter", "Read the selected file or conversation locally."),
            ("Space", "Select or deselect a chat."),
            ("a / n", "Select all shown chats / clear every selection."),
            (
                "d / Delete",
                "Review moving selected chat files to Trash, including hidden selections.",
            ),
            (
                "i",
                "Link a local skill folder. Tab picks Claude, Codex, or Gemini.",
            ),
            (
                "e",
                "Edit your SKILL.md in $VISUAL or $EDITOR, then review.",
            ),
            ("b", "Review restoring the latest skill backup."),
            ("y", "Confirm the listed change on the review screen."),
            ("o", "Reveal the selected file in Finder."),
            ("r / w", "Refresh files / show skipped paths."),
            (
                "Esc",
                "Cancel, close a preview, clear a filter, then go back.",
            ),
        ]
    }
}

#[cfg(test)]
mod tests;
