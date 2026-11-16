//! Browse local assistant files and review skill changes.

mod chats;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use neet_core::ai::skills::{Edit, Install, Relink, Unlink};
use neet_core::ai::{self, Item, Kind, Listing, Locations, Provider, Source};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Padding, Paragraph, Wrap};
use similar::{ChangeTag, TextDiff};

use super::app::{Action, Context, Screen};
use super::{format, visual};

const TABS: [&str; 3] = ["Files", "Chats", "Skills"];
const INSTALL_TARGETS: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Gemini];

/// The tools By tool counts, in the order shown
const TOOLS: [Provider; 4] = [
    Provider::Claude,
    Provider::Codex,
    Provider::Gemini,
    Provider::Shared,
];

/// Lines kept for what the keys do, under the selected item's fields
const KEY_LINES: u16 = 4;

/// How the list is ordered. `s` steps through them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sort {
    /// As found: by tool, then by path
    Listed,
    Newest,
    Oldest,
    Largest,
}

impl Sort {
    fn next(self) -> Self {
        match self {
            Self::Listed => Self::Newest,
            Self::Newest => Self::Oldest,
            Self::Oldest => Self::Largest,
            Self::Largest => Self::Listed,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Listed => "by tool",
            Self::Newest => "newest first",
            Self::Oldest => "oldest first",
            Self::Largest => "largest first",
        }
    }
}

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
    Unlink(Unlink),
    /// Archive or unarchive a Codex chat
    Archive(Item),
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
    sort: Sort,
    provider: Option<Provider>,
    input: Option<Input>,
    viewer: Option<Viewer>,
    review: Option<Review>,
    editing: Option<Edit>,
    cleanup: Option<chats::Cleanup>,
    /// The last skill link removed, to put back with `u`
    relink: Option<Relink>,
    /// Codex archiving a chat, on its own thread
    archiving: Option<Receiver<Result<bool, String>>>,
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
        screen.provider = match super::settings::saved_prefs().ai_tool.as_str() {
            "claude" => Some(Provider::Claude),
            "codex" => Some(Provider::Codex),
            "gemini" => Some(Provider::Gemini),
            "shared" => Some(Provider::Shared),
            _ => None,
        };
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
            sort: Sort::Listed,
            provider: None,
            input: None,
            viewer: None,
            review: None,
            editing: None,
            cleanup: None,
            relink: None,
            archiving: None,
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
        self.poll_archive();
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

    /// Whether `item` belongs on the open tab
    fn on_tab(&self, item: &Item) -> bool {
        match self.tab {
            1 => item.kind == Kind::Chat,
            2 => item.kind == Kind::Skill,
            _ => matches!(item.kind, Kind::Settings | Kind::Instructions),
        }
    }

    fn rows(&self) -> Vec<&Item> {
        let filter = self.filter.to_lowercase();
        let mut rows: Vec<&Item> = self
            .listing
            .items
            .iter()
            .filter(|item| {
                self.on_tab(item)
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
            .collect();
        // Stable, so items of one age or size keep the order they were found in.
        match self.sort {
            Sort::Listed => {}
            Sort::Newest => rows.sort_by_key(|item| std::cmp::Reverse(item.modified)),
            Sort::Oldest => rows.sort_by_key(|item| (item.modified.is_none(), item.modified)),
            Sort::Largest => rows.sort_by_key(|item| std::cmp::Reverse(item.size)),
        }
        rows
    }

    /// `s`: the next order, with the same item still selected
    fn next_sort(&mut self) {
        let selected = self.selected().map(|item| item.path);
        self.sort = self.sort.next();
        let row = selected
            .and_then(|path| self.rows().iter().position(|item| item.path == path))
            .unwrap_or(0);
        self.list.select(Some(row));
    }

    /// `m`: selects the shown chats whose project folder is gone, or clears
    /// them when every one is selected already
    fn toggle_orphans(&mut self) {
        let paths: Vec<PathBuf> = self
            .rows()
            .iter()
            .filter(|item| item.chat.as_ref().is_some_and(|chat| chat.missing_project))
            .map(|item| item.path.clone())
            .collect();
        if paths.is_empty() {
            self.note = "No shown chat has a missing project.".into();
            return;
        }
        let clear = paths.iter().all(|path| self.marked.contains(path));
        let count = paths.len();
        for path in paths {
            if clear {
                self.marked.remove(&path);
            } else {
                self.marked.insert(path);
            }
        }
        let chats = if count == 1 { "chat" } else { "chats" };
        self.note = if clear {
            format!("{count} {chats} with a missing project cleared.")
        } else {
            format!("{count} {chats} with a missing project selected. d reviews moving to Trash.")
        };
    }

    /// The size of every selected chat, shown or not
    fn marked_size(&self) -> u64 {
        self.listing
            .items
            .iter()
            .filter(|item| self.marked.contains(&item.path))
            .map(|item| item.size)
            .sum()
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
            Review::Archive(item) => {
                let (sender, receiver) = mpsc::channel();
                let locations = self.locations.clone();
                thread::spawn(move || {
                    let _ = sender.send(
                        ai::actions::archive(&locations, &item).map_err(|error| error.to_string()),
                    );
                });
                self.archiving = Some(receiver);
                self.viewer = None;
                return;
            }
            Review::Unlink(unlink) => match unlink.apply() {
                Ok(relink) => {
                    let note = format!(
                        "Link removed. {} stays. u puts the link back. Restart the assistant or reload its skills.",
                        self.shown_path(&unlink.source())
                    );
                    self.relink = Some(relink);
                    note
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

    /// Shows what removing the selected skill's link does, before `y`
    fn review_unlink(&mut self) {
        let Some(item) = self.selected().filter(|item| item.kind == Kind::Skill) else {
            return;
        };
        match Unlink::prepare(&self.locations, &item) {
            Ok(unlink) => {
                let text = format!(
                    "Remove the link {}
It points to {}, which stays as it is.

The assistant stops loading this skill. Restart it or reload its skills afterwards.
u puts the link back while neet is open, and i can link the folder again later.
y removes the link. Esc cancels.",
                    self.shown_path(unlink.link()),
                    self.shown_path(&unlink.source())
                );
                self.viewer = Some(Viewer::new("Review removing a skill link".into(), &text));
                self.review = Some(Review::Unlink(unlink));
            }
            Err(error) => self.note = error.to_string(),
        }
    }

    /// Leaves neet to continue the selected chat in its own tool
    fn continue_chat(&mut self) -> Action {
        let Some(item) = self.chat_in_view() else {
            self.note = "Select a chat first.".into();
            return Action::None;
        };
        match ai::actions::resume(&self.locations, &item) {
            Ok(resume) => Action::Run(vec![resume]),
            Err(error) => {
                self.note = error.to_string();
                Action::None
            }
        }
    }

    /// The chat open in the preview, or else the selected one
    fn chat_in_view(&self) -> Option<Item> {
        let open = self
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.source.as_ref())
            .and_then(|path| self.listing.items.iter().find(|item| &item.path == path))
            .cloned();
        open.or_else(|| self.selected())
            .filter(|item| item.kind == Kind::Chat)
    }

    /// Asks before Codex archives the selected chat, or brings it back
    fn review_archive(&mut self) {
        let Some(item) = self.chat_in_view() else {
            self.note = "Select a chat first.".into();
            return;
        };
        if item.provider != Provider::Codex {
            self.note = format!(
                "{} has no archive. d moves a chat to Trash instead.",
                item.provider.name()
            );
            return;
        }
        let archived = ai::actions::is_archived(&self.locations, &item);
        let title = item
            .chat
            .as_ref()
            .map_or("Untitled chat", |chat| chat.title.as_str())
            .to_owned();
        let text = if archived {
            format!(
                "Bring back from the archive: {title}

Codex runs codex unarchive, which moves it back to its chats.
y unarchives it. Esc cancels."
            )
        } else {
            format!(
                "Archive: {title}

Codex runs codex archive, which moves it out of its chat list. Nothing is deleted, and z brings it back.
y archives it. Esc cancels."
            )
        };
        self.viewer = Some(Viewer::new(
            if archived {
                "Unarchive chat"
            } else {
                "Archive chat"
            }
            .into(),
            &text,
        ));
        self.review = Some(Review::Archive(item));
    }

    fn poll_archive(&mut self) {
        let Some(receiver) = &self.archiving else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Codex stopped without a result. Check its chat list.".into())
            }
        };
        self.archiving = None;
        self.note = match result {
            Ok(true) => "Archived in Codex. z on it brings it back.".into(),
            Ok(false) => "Brought back from the Codex archive.".into(),
            Err(error) => error,
        };
        self.refresh();
    }

    /// Puts the last removed skill link back
    fn undo_unlink(&mut self) {
        let Some(relink) = self.relink.take() else {
            self.note = "Nothing to put back.".into();
            return;
        };
        let selected = relink.link().join("SKILL.md");
        match relink.apply() {
            Ok(()) => {
                self.note = "Link put back. Restart the assistant or reload its skills.".into();
                self.refresh_select(Some(selected));
            }
            Err(error) => {
                self.note = error.to_string();
                self.relink = Some(relink);
            }
        }
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
                let source = item
                    .source
                    .as_ref()
                    .map_or_else(String::new, |source| format!(" · {}", source.label()));
                let room = usize::from(area.width.saturating_sub(6));
                let name = format::shorten_middle(
                    &ai::display_text(&format!(
                        "{marker}{} · {}{source}",
                        title,
                        item.provider.name()
                    ))
                    .replace('\n', " "),
                    room,
                );
                let second = place_line(item, &subtitle, room);
                ListItem::new(vec![
                    Line::from(name).style(visual::HEADING),
                    Line::from(second),
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
                "{} selected · {hidden} hidden by filters · {}",
                self.marked.len(),
                format::size(self.marked_size())
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

    /// Where a skill comes from, in a sentence
    fn source_text(&self, source: &Source) -> String {
        match source {
            Source::Copy => "Source: your copy, in the tool's skill folder.".into(),
            Source::Link(target) => format!("Source: a link to {}.", self.shown_path(target)),
            Source::Bundled => "Source: ships with the tool.".into(),
            Source::Plugin(plugin) => {
                let from = if plugin.marketplace.is_empty() {
                    String::new()
                } else {
                    format!(" from {}", plugin.marketplace)
                };
                let version = if plugin.version.is_empty() {
                    String::new()
                } else {
                    format!(", version {}", plugin.version)
                };
                let state = match plugin.enabled {
                    Some(true) => ", turned on",
                    Some(false) => ", turned off",
                    None => "",
                };
                format!("Source: plugin {}{from}{version}{state}.", plugin.name)
            }
        }
    }

    /// The selected item's name, and its fields: the same rows for every
    /// item on a tab, so the box keeps its height while the selection moves
    fn details(&self, width: usize) -> (String, Vec<Line<'static>>) {
        let room = width.saturating_sub(9);
        let field = |label: &str, value: Span<'static>| {
            Line::from(vec![Span::raw(format!("{label:<9}")).bold(), value])
        };
        let Some(item) = self.selected() else {
            return (
                "Selected".into(),
                vec![
                    field("Tools", Span::raw("Claude Code · Codex · Gemini CLI")),
                    field("Shared", Span::raw("~/.agents/skills")),
                ],
            );
        };
        let path = if item.kind == Kind::Skill {
            item.path.parent().unwrap_or(&item.path).to_path_buf()
        } else {
            item.path.clone()
        };
        let name = item
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
                |chat| chat.title.replace('\n', " "),
            );
        let mut lines = vec![field(
            "Tool",
            Span::raw(format!("{} · {}", item.provider.name(), item.kind.name())),
        )];
        if self.tab == 1 {
            lines.push(field(
                "Project",
                match item.chat.as_ref().and_then(|chat| {
                    chat.project
                        .as_ref()
                        .map(|project| (project, chat.missing_project))
                }) {
                    Some((project, true)) => Span::raw(format::shorten_path(
                        &format!("gone: {}", self.shown_path(project)),
                        room,
                    ))
                    .yellow(),
                    Some((project, false)) => {
                        Span::raw(format::shorten_path(&self.shown_path(project), room))
                    }
                    None => Span::raw("unknown"),
                },
            ));
        }
        lines.push(field(
            "Path",
            Span::raw(format::shorten_path(&self.shown_path(&item.path), room)),
        ));
        lines.push(field(
            "Where",
            Span::raw(format::shorten_middle(&ai::display_text(&item.scope), room)),
        ));
        lines.push(field(
            "Size",
            format::size_span(item.size, format::size(item.size)),
        ));
        lines.push(field(
            "Updated",
            Span::raw(
                item.modified
                    .and_then(|time| time.elapsed().ok())
                    .map_or_else(|| "unknown".to_string(), format::age),
            ),
        ));
        if self.tab == 2 {
            let source = item
                .source
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |source| self.source_text(source));
            lines.extend(visual::hanging(
                vec![Span::raw(format!("{:<9}", "Source")).bold()],
                9,
                &ai::display_text(source.strip_prefix("Source: ").unwrap_or(&source)),
                ratatui::style::Style::default(),
                width,
            ));
        }
        if let Some(chat) = item.chat.as_ref().filter(|chat| !chat.note.is_empty()) {
            lines.push(field("Note", Span::raw(chat.note.clone())));
        }
        lines.push(Line::default());
        lines.push(Line::from(keys_for(&item)));
        (ai::display_text(&name), lines)
    }

    /// How many items of the open tab each tool has, and their size
    fn by_tool(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("By tool").style(visual::HEADING)];
        let totals: Vec<(Provider, usize, u64)> = TOOLS
            .iter()
            .map(|&tool| {
                let items = self
                    .listing
                    .items
                    .iter()
                    .filter(|item| self.on_tab(item) && item.provider == tool);
                let (count, size) =
                    items.fold((0, 0), |(count, size), item| (count + 1, size + item.size));
                (tool, count, size)
            })
            .collect();
        let largest = totals.iter().map(|total| total.2).max().unwrap_or(0);
        let bar = width.saturating_sub(31).min(24);
        for (tool, count, size) in &totals {
            let mut spans = vec![
                Span::raw(format!("{:<12}", tool.name())).bold(),
                Span::raw(format!("{count:>5}  ")),
                format::size_span(*size, format!("{:>9}", format::size(*size))),
            ];
            if bar >= 6 {
                spans.push(Span::raw("  "));
                spans.push(format::size_bar(*size, largest, bar));
            }
            lines.push(Line::from(spans));
        }
        let (count, size) = totals.iter().fold((0, 0), |(count, size), total| {
            (count + total.1, size + total.2)
        });
        lines.push(Line::from(vec![
            Span::raw(format!("{:<12}", "All")).bold(),
            Span::raw(format!("{count:>5}  ")).bold(),
            format::size_span(size, format!("{:>9}", format::size(size))).bold(),
        ]));
        lines
    }

    /// What is selected for removal, and the quick ways to select
    fn selection(&self) -> Vec<Line<'static>> {
        let orphans = self
            .listing
            .items
            .iter()
            .filter(|item| item.chat.as_ref().is_some_and(|chat| chat.missing_project))
            .count();
        vec![
            Line::from("Selected").style(visual::HEADING),
            Line::from(vec![
                Span::raw(format!("{:<12}", "Chats")).bold(),
                Span::raw(format!("{:>5}  ", self.marked.len())),
                format::size_span(
                    self.marked_size(),
                    format!("{:>9}", format::size(self.marked_size())),
                ),
            ]),
            Line::from(vec![
                Span::raw(format!("{:<12}", "Project gone")).bold(),
                Span::raw(format!("{orphans:>5}  ")),
                Span::raw("m").fg(visual::ACCENT).bold(),
                Span::raw(" selects them"),
            ]),
        ]
    }

    /// The selected item's box, with the tab's totals under it
    fn draw_side(&self, frame: &mut Frame, area: Rect) {
        let width = usize::from(area.width.saturating_sub(4));
        let (name, lines) = self.details(width);
        let block = visual::block()
            .title(Line::from(format!(" {} ", format::shorten_middle(&name, width))).bold())
            .padding(Padding::horizontal(1));
        // Fields, a blank row, and room for what the keys do
        let fields = match self.tab {
            1 => 6,
            // The source can take a second row.
            2 => 8,
            _ => 5,
        };
        let height = fields + 1 + KEY_LINES + 2;
        let mut extras = vec![self.by_tool(width)];
        if self.tab == 1 {
            extras.push(self.selection());
        }
        if self.selected().is_none() || area.height < height + 8 {
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(block),
                area,
            );
            return;
        }
        let [top, below] =
            Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            top,
        );
        visual::sections(frame, below, "", extras);
    }
}

/// What the keys do with `item`, in a line
fn keys_for(item: &Item) -> &'static str {
    match (item.kind, &item.source) {
        (Kind::Chat, _) if item.provider == Provider::Codex => {
            "Enter reads · c continues · z archives · Space selects · d reviews removal."
        }
        (Kind::Chat, _) if item.provider == Provider::Claude => {
            "Enter reads · c continues · Space selects · d reviews removal."
        }
        (Kind::Chat, _) => "Enter reads · Space selects · a selects all shown · d reviews removal.",
        (Kind::Settings, _) => "Enter shows settings, which may include tokens. View only.",
        (Kind::Instructions, _) => "Enter shows instructions. View only.",
        (Kind::Skill, Some(Source::Plugin(_))) => {
            "The tool updates plugin skills. Enter previews it. View only. Turn the plugin on or off in the tool, such as /plugin in Claude Code."
        }
        (Kind::Skill, _) if item.bundled => "Bundled skill. Enter previews it. View only.",
        (Kind::Skill, Some(Source::Link(_))) => {
            "Enter previews · e edits the source · b restores the latest backup · x removes the link. Every change is reviewed first."
        }
        (Kind::Skill, _) => {
            "Enter previews · e edits · b restores the latest backup. Every change is reviewed first."
        }
    }
}

/// Under an item's name: where it is, then its size and age, which stay
/// whole while the path gives way
fn place_line(item: &Item, place: &str, room: usize) -> Vec<Span<'static>> {
    let age = item
        .modified
        .and_then(|time| time.elapsed().ok())
        .map_or_else(String::new, |age| format!(" · {}", format::age(age)));
    let size = format!("{:>9}", format::size(item.size));
    let right = format::display_width(&size) + format::display_width(&age);
    let wide = room > right + 20;
    let place_room = if wide { room - right - 1 } else { room };
    let place = format::shorten_middle(place, place_room);
    let gap = place_room.saturating_sub(format::display_width(&place)) + 1;
    let mut spans = vec![Span::raw(place)];
    if wide {
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(format::size_span(item.size, size));
        spans.push(Span::raw(age));
    }
    spans
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
        if self.archiving.is_some() {
            super::loading::Loading {
                title: "AI Tools",
                doing: "Codex is archiving the chat",
                progress: "codex archive".into(),
                note: "Codex moves the chat file and updates its own list.",
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
        let mut tabs = Vec::new();
        for (index, name) in TABS.iter().enumerate() {
            if index > 0 {
                tabs.push(Span::raw("   "));
            }
            tabs.push(if index == self.tab {
                Span::raw(format!("[{name}]")).bold().fg(visual::ACCENT)
            } else {
                Span::raw((*name).to_string())
            });
        }
        tabs.push(Span::raw(format!(
            "   · {} · {}",
            self.provider.map_or("All tools", Provider::name),
            self.sort.name()
        )));
        frame.render_widget(
            Paragraph::new(Line::from(tabs)).block(
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
        self.draw_side(frame, details);
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
        if self.pending.is_some() || self.cleanup.is_some() || self.archiving.is_some() {
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
                KeyCode::Char('c') if self.review.is_none() && viewer.source.is_some() => {
                    return self.continue_chat();
                }
                KeyCode::Char('z') if self.review.is_none() && viewer.source.is_some() => {
                    self.review_archive();
                }
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
            KeyCode::Char('m') if self.tab == 1 => self.toggle_orphans(),
            KeyCode::Char('s') => self.next_sort(),
            KeyCode::Char('d') | KeyCode::Delete if self.tab == 1 => self.review_chat_removal(),
            KeyCode::Char('c') if self.tab == 1 => return self.continue_chat(),
            KeyCode::Char('z') if self.tab == 1 => self.review_archive(),
            KeyCode::Char('/') => self.input = Some(Input::Search),
            KeyCode::Char('i') => {
                self.input = Some(Input::Install {
                    path: String::new(),
                    target: 0,
                });
            }
            KeyCode::Char('x') if self.tab == 2 => self.review_unlink(),
            KeyCode::Char('u') if self.tab == 2 => self.undo_unlink(),
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

    fn ran(&mut self, result: Result<(), String>) {
        self.note = match result {
            Ok(()) => "Back from the chat.".into(),
            Err(error) => error,
        };
        self.refresh();
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
        if self.cleanup.is_some() || self.archiving.is_some() {
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
        self.review.is_some()
            || self.editing.is_some()
            || self.cleanup.is_some()
            || self.archiving.is_some()
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
            && self.chat_in_view().is_some()
        {
            "↑↓ scroll · PgUp/PgDn page · c continue · z archive · o Finder · Esc back · ? help"
        } else if self
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.source.is_some())
        {
            "↑↓ scroll · PgUp/PgDn page · o Finder · Esc back · ? help · q quit"
        } else if self.viewer.is_some() {
            "↑↓ scroll · PgUp/PgDn page · Esc back · ? help · q quit"
        } else if self.tab == 1 {
            "↑↓ move · Enter read · Space select · d delete · c continue · s sort · / find · p tool · Tab section · ? help · Esc back"
        } else if self.tab == 2 {
            "↑↓ move · Enter read · i link skill · e edit · x remove link · s sort · / find · p tool · Tab section · ? help · Esc back"
        } else {
            "↑↓ move · Enter read · s sort · / find · p tool · o Finder · r refresh · Tab section · ? help · Esc back"
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
            (
                "s",
                "Sort: by tool, newest first, oldest first, largest first.",
            ),
            ("Space", "Select or deselect a chat."),
            (
                "m",
                "Select the shown chats whose project folder is gone, or clear them.",
            ),
            (
                "a / n",
                "Select all shown chats / clear every selection. n also cancels chat removal.",
            ),
            (
                "c",
                "Continue the chat in Claude Code or Codex. neet comes back when it quits.",
            ),
            ("z", "Archive a Codex chat, or bring it back, with Codex."),
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
            (
                "x",
                "Review removing a skill's link. The folder it points to stays.",
            ),
            ("u", "Put the last removed skill link back."),
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
