use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::SystemTime;

use neet_core::dotfiles::change::{self as core_change, Checked, Edit};
use neet_core::dotfiles::{self, Dotfile, Group, Listing, Managed, Repo};
use neet_core::rewrite::Backups;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::format;
use super::visual::{self, Columns};

mod actions;
mod change;
mod configure;
mod export;

use actions::{Act, Menu};
use change::{Diff, Fix, Mode, Review};
use configure::Configure;
use export::{Export, Step};
use neet_core::dotfiles::configure as settings;
use neet_core::dotfiles::export as publish;
use neet_core::rewrite::Backup;

/// From this width the selected file and its preview show on the right.
const MIN_SIDE_WIDTH: u16 = 96;

/// The file list's width beside the details: wide enough for every name and
/// status, never so wide that rows are mostly empty
const LIST_WIDTH: (u16, u16) = (48, 64);

/// Width of the status column, such as `+ not saved · secret`
const STATUS_WIDTH: u16 = 20;

/// What the repository line says while Git is still being read
enum RepoState {
    Reading(Receiver<Option<Repo>>),
    Read(Option<Repo>),
}

/// One row of the file table
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Group(Group),
    /// An index into the listing's files
    File(usize),
}

/// Lists your settings files grouped by program, with whether each is saved
/// in your dotfiles, which chezmoi keeps. `Enter` shows what can be done
/// with one; each action also has its own key.
pub struct Dotfiles {
    home: PathBuf,
    listing: Listing,
    backups: Backups,
    mode: Mode,
    repo: RepoState,
    show_missing: bool,
    table: TableState,
    /// The selected file's preview, read once per selection
    preview: Option<(usize, Result<String, String>)>,
    failed: Option<String>,
}

impl Dotfiles {
    pub fn new() -> Self {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            let mut screen = Self::from_listing(PathBuf::new(), empty());
            screen.failed = Some("HOME is not set, so there are no settings files to list.".into());
            return screen;
        };
        let listing = dotfiles::list(&home);
        let mut screen = Self::from_listing(home, listing);
        screen.read_repo();
        screen
    }

    /// Reads the repository on its own thread, since Git can be slow.
    fn read_repo(&mut self) {
        let Some(chezmoi) = &self.listing.chezmoi else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let source = chezmoi.source.clone();
        thread::spawn(move || {
            // The receiver is gone only when the screen was closed.
            let _ = sender.send(dotfiles::repo(&source));
        });
        self.repo = RepoState::Reading(receiver);
    }

    /// Looks at every file again after a change. Files keep their place, so
    /// the selection stays on the same one.
    fn reload(&mut self) {
        self.listing = dotfiles::list(&self.home);
        self.preview = None;
        self.read_repo();
        if self.selected().is_none() {
            self.select_first();
        }
    }

    fn from_listing(home: PathBuf, listing: Listing) -> Self {
        let mut screen = Self {
            backups: Backups::dotfiles(&home),
            home,
            listing,
            mode: Mode::Browse,
            repo: RepoState::Read(None),
            show_missing: false,
            table: TableState::default(),
            preview: None,
            failed: None,
        };
        screen.select_first();
        screen
    }

    /// The rows shown: each group that has a file shown, then its files
    fn items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let mut group = None;
        for (index, file) in self.listing.files.iter().enumerate() {
            if file.found.is_none() && !self.show_missing {
                continue;
            }
            if group != Some(file.known.group) {
                group = Some(file.known.group);
                items.push(Item::Group(file.known.group));
            }
            items.push(Item::File(index));
        }
        items
    }

    fn selected(&self) -> Option<usize> {
        match self.items().get(self.table.selected()?) {
            Some(Item::File(index)) => Some(*index),
            _ => None,
        }
    }

    fn select_first(&mut self) {
        let first = self
            .items()
            .iter()
            .position(|item| matches!(item, Item::File(_)));
        self.table.select(first);
    }

    /// Moves to the next file up or down, past group names
    fn step(&mut self, down: bool) {
        let items = self.items();
        let Some(current) = self.table.selected() else {
            return;
        };
        let next = if down {
            (current + 1..items.len()).find(|&index| matches!(items[index], Item::File(_)))
        } else {
            (0..current)
                .rev()
                .find(|&index| matches!(items[index], Item::File(_)))
        };
        if let Some(next) = next {
            self.table.select(Some(next));
        }
    }

    fn jump(&mut self, last: bool) {
        let items = self.items();
        let found = if last {
            items.iter().rposition(|item| matches!(item, Item::File(_)))
        } else {
            items.iter().position(|item| matches!(item, Item::File(_)))
        };
        if found.is_some() {
            self.table.select(found);
        }
    }

    /// Keeps the same file selected when missing files are shown or hidden
    fn toggle_missing(&mut self) {
        let selected = self.selected();
        self.show_missing = !self.show_missing;
        let items = self.items();
        let index =
            selected.and_then(|file| items.iter().position(|item| *item == Item::File(file)));
        match index {
            Some(index) => self.table.select(Some(index)),
            None => self.select_first(),
        }
    }

    fn poll(&mut self) {
        if let RepoState::Reading(receiver) = &self.repo
            && let Ok(repo) = receiver.try_recv()
        {
            self.repo = RepoState::Read(repo);
        }
    }

    /// A path from the home folder as `~/...`
    fn tilde(&self, path: &Path) -> String {
        path.strip_prefix(&self.home).map_or_else(
            |_| path.display().to_string(),
            |rest| format!("~/{}", rest.display()),
        )
    }

    /// The line at the top: how many files are saved, changed, or not saved
    fn draw_top(&self, frame: &mut Frame, area: Rect) {
        let mut spans = Vec::new();
        if self.listing.chezmoi.is_some() {
            let found: Vec<&Dotfile> = self
                .listing
                .files
                .iter()
                .filter(|file| file.found.is_some())
                .collect();
            for state in [State::Saved, State::Changed, State::NotSaved] {
                let count = found.iter().filter(|file| State::of(file) == state).count();
                if count == 0 {
                    continue;
                }
                if !spans.is_empty() {
                    spans.push(Span::raw(" · "));
                }
                spans.push(state.styled(format!("{count} {}", state.word())));
            }
        } else {
            spans.push(Span::raw("chezmoi not found"));
        }
        spans.insert(0, Span::raw(" "));
        spans.push(Span::raw(" "));
        frame.render_widget(
            Block::new()
                .borders(Borders::TOP)
                .title(" Dotfiles ")
                .title_style(visual::HEADING)
                .title(Line::from(spans).right_aligned()),
            area,
        );
    }

    /// The file table, sized to its rows, with your dotfiles' repository
    /// below it when there is room
    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        // With no files, room for the message that says so
        let rows = u16::try_from(self.items().len()).unwrap_or(u16::MAX).max(3);
        let table_height = rows.saturating_add(2);
        let summary = self.summary();
        let summary_height =
            visual::wrapped_rows(&summary, area.width.saturating_sub(4)).saturating_add(2);
        if area.height < table_height.saturating_add(summary_height) {
            self.draw_table(frame, area);
            return;
        }
        // The box under the list runs to the bottom: where your dotfiles
        // are at its top, what the marks mean, and backups at its bottom.
        let [table, below] =
            Layout::vertical([Constraint::Length(table_height), Constraint::Fill(1)]).areas(area);
        self.draw_table(frame, table);
        visual::sections(
            frame,
            below,
            " Your dotfiles ",
            // The legend matters more than backups, so backups go first when
            // room is short: they sit at the bottom.
            vec![summary, self.legend(), self.backup_lines()],
        );
    }

    /// How many backups neet keeps of the listed files, the newest, and where
    fn backup_lines(&self) -> Vec<Line<'static>> {
        let all: Vec<Backup> = self
            .listing
            .files
            .iter()
            .filter_map(|file| self.backups.list(file.known.path).ok())
            .flatten()
            .collect();
        let newest = all.iter().max_by(|a, b| a.saved.cmp(&b.saved));
        let mut lines = vec![Line::from("Backups").style(visual::HEADING)];
        lines.push(field(
            "Saved",
            match newest {
                None => Span::raw("none yet, one before every change"),
                Some(newest) => {
                    Span::raw(format!("{} · newest {}", all.len(), change::saved(newest)))
                }
            },
        ));
        lines.push(field(
            "Kept in",
            Span::raw("~/.local/state/neet/backups/dotfiles"),
        ));
        lines
    }

    /// What each mark in the list means, for the marks it shows
    fn legend(&self) -> Vec<Line<'static>> {
        let shown: Vec<State> = self
            .items()
            .iter()
            .filter_map(|item| match item {
                Item::File(index) => Some(State::of(&self.listing.files[*index])),
                Item::Group(_) => None,
            })
            .collect();
        let mut lines = vec![Line::from("What the marks mean").style(visual::HEADING)];
        for (state, meaning) in [
            (State::Saved, "matches its saved copy"),
            (State::Changed, "differs from its saved copy"),
            (State::NotSaved, "not in your dotfiles yet"),
            (State::ViewOnly, "neet will not change it"),
            (State::LeftOut, ".chezmoiignore leaves it out"),
            (State::Missing, "not on this Mac"),
        ] {
            if shown.contains(&state) {
                lines.push(Line::from(vec![
                    state.styled(format!(
                        "{:<14}",
                        format!("{} {}", state.symbol(), state.word())
                    )),
                    Span::raw(meaning),
                ]));
            }
        }
        if self.items().iter().any(
            |item| matches!(item, Item::File(index) if self.listing.files[*index].may_hold_secrets),
        ) {
            lines.push(Line::from(vec![
                Span::raw(format!("{:<14}", "secret")).red(),
                Span::raw("may hold a token: kept private"),
            ]));
        }
        lines
    }

    /// Where your dotfiles are kept, and what waits to be exported
    fn summary(&self) -> Vec<Line<'static>> {
        let Some(chezmoi) = &self.listing.chezmoi else {
            return vec![
                Line::from("chezmoi was not found, so each file is changed on its own."),
                Line::from(
                    "chezmoi keeps your dotfiles in a Git repository and sets up a new Mac from it.",
                ),
            ];
        };
        let mut lines = match &self.repo {
            RepoState::Reading(_) => vec![field("Repo", Span::raw("looking…"))],
            RepoState::Read(None) => vec![field("Repo", Span::raw("not in a Git repository"))],
            RepoState::Read(Some(repo)) => {
                let remote = repo.remote.as_deref().unwrap_or("no remote");
                let name = match &repo.branch {
                    Some(branch) => format!("{remote} · {branch}"),
                    None => remote.to_string(),
                };
                vec![
                    field("Repo", Span::raw(name)),
                    field(
                        "Export",
                        match repo.uncommitted {
                            0 => Span::raw("nothing waiting").green(),
                            1 => Span::raw("1 file waiting · x exports it").yellow(),
                            count => Span::raw(format!("{count} files waiting · x exports them"))
                                .yellow(),
                        },
                    ),
                    field(
                        "Push",
                        match repo.ahead_behind {
                            None => Span::raw("not tracking a remote branch"),
                            Some((0, 0)) => Span::raw("up to date").green(),
                            Some((ahead, 0)) => {
                                Span::raw(format!("{ahead} to push · x pushes")).yellow()
                            }
                            Some((0, behind)) => Span::raw(format!("{behind} to pull")).yellow(),
                            Some((ahead, behind)) => {
                                Span::raw(format!("{ahead} to push, {behind} to pull")).yellow()
                            }
                        },
                    ),
                ]
            }
        };
        lines.push(field("Folder", Span::raw(self.tilde(&chezmoi.source))));
        if let Some(reason) = &chezmoi.may_not_run {
            lines.push(
                Line::from(format!(
                    "neet will not run chezmoi: {reason}. It writes the saved copies itself, and says what to run."
                ))
                .yellow(),
            );
        }
        lines
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect) {
        let items = self.items();
        let block = visual::block()
            .title(" Files ")
            .padding(Padding::horizontal(1));
        if items.is_empty() {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            centered(
                frame,
                inner,
                "No settings files found. Press . to see the files neet looks for.",
            );
            return;
        }
        let files = &self.listing.files;
        // Names take what the longest needs, so each status sits close to
        // its name.
        let longest = items
            .iter()
            .filter_map(|item| match item {
                Item::File(index) => Some(format::display_width(shown_name(&files[*index]))),
                Item::Group(_) => None,
            })
            .max()
            .unwrap_or(0);
        let name_width = u16::try_from(longest + 4).unwrap_or(u16::MAX).max(14);
        let columns = Columns::new(area.width, [(name_width, 0), (STATUS_WIDTH, 1)], &[], true);
        let rows: Vec<Row> = items
            .iter()
            .map(|item| match *item {
                Item::Group(group) => columns.row([
                    Cell::from(Span::styled(group.name(), visual::HEADING)),
                    Cell::from(""),
                ]),
                Item::File(index) => file_row(&files[index], &columns),
            })
            .collect();
        let table = Table::new(rows, columns.widths())
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// The selected file's name, and lines on where it is, how it stands,
    /// and what to do. Paths are shortened to fit `width`.
    fn details(&self, index: usize, width: usize) -> (String, Vec<Line<'static>>) {
        let file = &self.listing.files[index];
        let name = file
            .known
            .path
            .rsplit('/')
            .next()
            .unwrap_or(file.known.path)
            .to_string();
        let room = width.saturating_sub(9);
        let mut lines = vec![field(
            "Path",
            Span::raw(format::shorten_path(&self.tilde(&file.path), room)),
        )];
        if let Some(found) = &file.found {
            if let Some(link) = &found.link {
                lines.push(field(
                    "Link to",
                    Span::raw(format::shorten_path(&self.tilde(link), room)),
                ));
            }
            if let Some(source) = self.source_name(file) {
                lines.push(field(
                    "Saved as",
                    Span::raw(format::shorten_path(&source, room)),
                ));
            }
            let changed = found
                .modified
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .map_or_else(|| "unknown".to_string(), format::age);
            lines.push(field("Changed", Span::raw(changed)));
            let count = self.backup_count(file);
            lines.push(field("Backups", Span::raw(count.to_string())));
        }
        lines.push(Line::default());
        lines.extend(Self::notes(file));
        (name, lines)
    }

    fn backup_count(&self, file: &Dotfile) -> usize {
        self.backups
            .list(file.known.path)
            .map_or(0, |list| list.len())
    }

    /// What the file's status means, and what to do, in plain words
    fn notes(file: &Dotfile) -> Vec<Line<'static>> {
        let mut notes = Vec::new();
        if file.found.is_none() {
            notes.push(Line::from("Not on this Mac."));
            return notes;
        }
        if let Some(view_only) = &file.view_only {
            notes.push(Line::from(format!("{}. View only.", view_only.describe())).yellow());
        }
        match (&file.managed, file.view_only.is_some()) {
            (Some(Managed::InSync(_)), _) => {
                notes.push(Line::from("✓ Saved in your dotfiles. Both copies match.").green());
            }
            (Some(Managed::Differs(_)), false) => notes.push(
                Line::from(
                    "! Changed since it was saved. Enter saves this version or uses the saved one.",
                )
                .yellow(),
            ),
            (Some(Managed::Differs(_)), true) => {
                notes.push(Line::from("! Changed since it was saved.").yellow());
            }
            (Some(Managed::Ignored), _) => {
                notes.push(Line::from(".chezmoiignore leaves it out of your dotfiles."));
            }
            (Some(Managed::No), false) => {
                notes.push(Line::from("+ Not in your dotfiles yet. Enter adds it."));
            }
            (Some(Managed::No), true) => notes.push(Line::from("Not in your dotfiles.")),
            (Some(Managed::Built(..)) | None, _) => {}
        }
        if file.may_hold_secrets {
            notes.push(
                Line::from("It may hold a token. Its preview is hidden, and it starts left out of exports.")
                    .red(),
            );
        }
        notes
    }

    /// The right side: the selected file, sized to fit, and its preview
    /// below it
    fn draw_side(&mut self, frame: &mut Frame, area: Rect) {
        let Some(index) = self.selected() else {
            return;
        };
        let width = usize::from(area.width.saturating_sub(4));
        let (name, lines) = self.details(index, width);
        let height =
            (visual::wrapped_rows(&lines, area.width.saturating_sub(4)) + 2).min(area.height);
        let [details, preview] =
            Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                visual::block()
                    .title(Line::from(format!(" {name} ")).bold())
                    .padding(Padding::horizontal(1)),
            ),
            details,
        );
        self.draw_preview(frame, preview, index);
    }

    fn draw_preview(&mut self, frame: &mut Frame, area: Rect, index: usize) {
        let block = visual::block()
            .title(" Preview ")
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let file = &self.listing.files[index];
        let note = if file.found.is_none() {
            Some("Not on this Mac.")
        } else if file.may_hold_secrets {
            Some("Hidden, since it may hold a token.")
        } else if matches!(
            file.view_only,
            Some(dotfiles::ViewOnly::NotAFile | dotfiles::ViewOnly::BrokenLink)
        ) {
            Some("Nothing to show.")
        } else {
            None
        };
        if let Some(note) = note {
            centered(frame, inner, note);
            return;
        }
        if self
            .preview
            .as_ref()
            .is_none_or(|(shown, _)| *shown != index)
        {
            let text = dotfiles::preview(&file.path).map_err(|error| error.to_string());
            self.preview = Some((index, text));
        }
        let Some((_, text)) = &self.preview else {
            return;
        };
        let width = usize::from(inner.width);
        match text {
            Ok(text) if text.trim().is_empty() => centered(frame, inner, "The file is empty."),
            Ok(text) => {
                let lines: Vec<Line> = text
                    .lines()
                    .take(usize::from(inner.height))
                    .map(|line| Line::from(format::shorten_middle(line, width)))
                    .collect();
                frame.render_widget(Paragraph::new(lines), inner);
            }
            Err(error) => frame.render_widget(
                Paragraph::new(format!("It could not be read: {error}"))
                    .red()
                    .wrap(Wrap { trim: true }),
                inner,
            ),
        }
    }
}

/// Editing, and fixing files that differ from chezmoi's source file
impl Dotfiles {
    /// The source file's path inside chezmoi's folder, such as `dot_zshrc`
    fn source_name(&self, file: &Dotfile) -> Option<String> {
        let source = file.managed.as_ref()?.source()?;
        let chezmoi = self.listing.chezmoi.as_ref()?;
        Some(
            source
                .strip_prefix(&chezmoi.source)
                .unwrap_or(source)
                .display()
                .to_string(),
        )
    }

    /// `Enter`: shows what can be done with the selected file.
    fn open_actions(&mut self) {
        let Some(index) = self.selected() else {
            return;
        };
        let file = &self.listing.files[index];
        let acts = actions::for_file(file, self.listing.chezmoi.as_ref(), self.backup_count(file));
        self.mode = if acts.is_empty() {
            let mut lines = Self::notes(file);
            lines.push(Line::from("There is nothing neet can do with it."));
            Mode::Note {
                title: file.known.path.to_string(),
                lines,
                color: visual::ACCENT,
            }
        } else {
            Mode::Actions(Menu::new(index, acts))
        };
    }

    /// Does `act` on the selected file, from the menu or its own key.
    fn run_act(&mut self, act: Act) -> Action {
        self.mode = Mode::Browse;
        match act {
            Act::Edit => return self.start_edit(),
            Act::SaveMine => self.ask_fix(Fix::Keep),
            Act::UseSaved => self.ask_fix(Fix::PutBack),
            Act::Add => self.ask_add(),
            Act::Changes => self.show_diff(),
            Act::Configure => self.start_configure(),
            Act::Backups => self.open_backups(),
        }
        Action::None
    }

    /// `e`: opens a copy of the selected file in your editor.
    fn start_edit(&mut self) -> Action {
        let Some(index) = self.selected() else {
            return Action::None;
        };
        let file = &self.listing.files[index];
        let begun = Edit::begin(file, self.listing.chezmoi.as_ref()).and_then(|edit| {
            edit.copy(&self.home)
                .map(|copy| (edit, copy))
                .map_err(|error| format!("A copy to edit could not be made: {error}."))
        });
        match begun {
            Ok((edit, copy)) => {
                self.mode = Mode::Editing {
                    edit,
                    copy: copy.clone(),
                };
                Action::Edit(copy)
            }
            Err(error) => {
                self.mode = change::done(Err(error), "", None);
                Action::None
            }
        }
    }

    /// `r` or `p`: shows what the fix changes, and asks.
    fn ask_fix(&mut self, fix: Fix) {
        let Some(index) = self.selected() else {
            return;
        };
        let file = &self.listing.files[index];
        let Some(Managed::Differs(source)) = &file.managed else {
            self.mode = change::done(
                Err("It does not differ from a source file in chezmoi.".to_string()),
                "",
                None,
            );
            return;
        };
        if fix == Fix::PutBack
            && let Some(reason) = self
                .listing
                .chezmoi
                .as_ref()
                .and_then(|chezmoi| chezmoi.may_not_run.as_ref())
        {
            self.mode = change::done(
                Err(format!(
                    "neet will not run chezmoi: {reason}. Run chezmoi apply ~/{} yourself.",
                    file.known.path
                )),
                "",
                None,
            );
            return;
        }
        let (Ok(home), Ok(source)) = (fs::read(&file.path), fs::read(source)) else {
            self.mode = change::done(
                Err("The file or its source file could not be read.".to_string()),
                "",
                None,
            );
            return;
        };
        let diff = match fix {
            Fix::Keep | Fix::Add => Diff::new(&source, &home),
            Fix::PutBack => Diff::new(&home, &source),
        };
        self.mode = Mode::Ask { fix, index, diff };
    }

    /// `a`: shows the whole file as new in chezmoi, and asks.
    fn ask_add(&mut self) {
        let Some(index) = self.selected() else {
            return;
        };
        let file = &self.listing.files[index];
        let added = core_change::add_name(file, self.listing.chezmoi.as_ref()).and_then(|_| {
            fs::read(&file.path).map_err(|error| format!("It could not be read: {error}."))
        });
        self.mode = match added {
            Ok(home) => Mode::Ask {
                fix: Fix::Add,
                index,
                diff: Diff::new(b"", &home),
            },
            Err(error) => change::done(Err(error), "", None),
        };
    }

    /// `y` on the question: runs the fix.
    fn run_fix(&mut self, fix: Fix, index: usize) {
        let file = &self.listing.files[index];
        let chezmoi = self.listing.chezmoi.as_ref();
        let now = SystemTime::now();
        let result = match fix {
            Fix::Keep => core_change::keep_home(file, chezmoi, &self.backups, now),
            Fix::PutBack => core_change::put_back(file, chezmoi, &self.backups, now),
            Fix::Add => core_change::add(file, chezmoi),
        };
        let shown = format!("~/{}", file.known.path);
        let source = self.source_name(file);
        self.mode = change::done(result, &shown, source.as_deref());
        self.reload();
    }

    /// `y` on the review: writes the edit.
    fn write_edit(&mut self) {
        let Mode::Review(review) = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return;
        };
        if matches!(review.checked, Checked::Failed(_)) {
            self.mode = Mode::Review(review);
            return;
        }
        let result = change::finish(&review, &self.backups);
        let index = self.selected();
        let file = index.map(|index| &self.listing.files[index]);
        let shown = file.map_or_else(String::new, |file| format!("~/{}", file.known.path));
        let source = file.and_then(|file| self.source_name(file));
        self.mode = change::done(result, &shown, source.as_deref());
        self.reload();
    }

    /// What the review says is written
    fn writes(&self, review: &Review) -> String {
        let file = self.selected().map(|index| &self.listing.files[index]);
        let shown = file.map_or_else(String::new, |file| format!("~/{}", file.known.path));
        match file.and_then(|file| self.source_name(file)) {
            Some(source) if review.edit.edits_source() && review.edit.applies() => {
                format!("{source} in chezmoi, then chezmoi apply {shown}")
            }
            Some(source) if review.edit.edits_source() => {
                format!("{source} in chezmoi. Then run chezmoi apply {shown} yourself")
            }
            _ => shown,
        }
    }

    /// Keys on the menu `Enter` opened
    fn actions_key(&mut self, key: KeyEvent) -> Action {
        let Mode::Actions(menu) = &mut self.mode else {
            return Action::None;
        };
        let act = match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                menu.step(false);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                menu.step(true);
                None
            }
            KeyCode::Enter => menu.chosen(),
            KeyCode::Char(letter) => menu.acts.iter().copied().find(|act| act.key() == letter),
            _ => None,
        };
        act.map_or(Action::None, |act| self.run_act(act))
    }

    /// Keys while a review, question, list, or note is open
    fn mode_key(&mut self, key: KeyEvent) -> Action {
        match &mut self.mode {
            Mode::Browse => {}
            Mode::Actions(_) => return self.actions_key(key),
            Mode::Editing { .. } => return Action::None,
            Mode::Note { .. } => {
                self.mode = Mode::Browse;
                return Action::None;
            }
            Mode::Review(review) => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => review.diff.scroll(false),
                    KeyCode::Down | KeyCode::Char('j') => review.diff.scroll(true),
                    KeyCode::Char('y') => self.write_edit(),
                    KeyCode::Char('e') => {
                        let Mode::Review(review) = std::mem::replace(&mut self.mode, Mode::Browse)
                        else {
                            return Action::None;
                        };
                        let copy = review.copy.clone();
                        self.mode = Mode::Editing {
                            edit: review.edit,
                            copy: copy.clone(),
                        };
                        return Action::Edit(copy);
                    }
                    _ => {}
                }
                return Action::None;
            }
            Mode::Ask { fix, index, diff } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => diff.scroll(false),
                    KeyCode::Down | KeyCode::Char('j') => diff.scroll(true),
                    KeyCode::Char('y') => {
                        let (fix, index) = (*fix, *index);
                        self.run_fix(fix, index);
                    }
                    _ => {}
                }
                return Action::None;
            }
            Mode::Backups { .. } => {
                self.backups_key(key);
                return Action::None;
            }
            Mode::Restore {
                index,
                backup,
                diff,
            } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => diff.scroll(false),
                    KeyCode::Down | KeyCode::Char('j') => diff.scroll(true),
                    KeyCode::Char('y') => {
                        let (index, backup) = (*index, backup.clone());
                        self.run_restore(index, &backup);
                    }
                    _ => {}
                }
                return Action::None;
            }
            Mode::Export(export_view) => {
                if let Step::Commit = export_view.key(key) {
                    self.commit_export();
                }
                return Action::None;
            }
            Mode::Push { top, target, .. } => {
                if key.code == KeyCode::Char('y') {
                    let (top, target) = (top.clone(), target.clone());
                    self.push(&top, &target);
                }
                return Action::None;
            }
            Mode::Configure(configure) => {
                if configure.key(key) {
                    self.review_configure();
                }
                return Action::None;
            }
            Mode::Show { diff, .. } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => diff.scroll(false),
                    KeyCode::Down | KeyCode::Char('j') => diff.scroll(true),
                    _ => self.mode = Mode::Browse,
                }
                return Action::None;
            }
        }
        Action::None
    }

    /// Commits made here and not yet on the remote, as last read
    fn ahead(&self) -> Option<usize> {
        match &self.repo {
            RepoState::Read(Some(repo)) => repo.ahead_behind.map(|(ahead, _)| ahead),
            _ => None,
        }
    }

    /// `x`: reviews the source files waiting to be committed, or offers to
    /// push when only commits wait.
    fn start_export(&mut self) {
        if self.listing.chezmoi.is_none() {
            self.mode = Mode::Note {
                title: "Export".to_string(),
                lines: vec![Line::from(
                    "Export works with chezmoi's repository for now. Starting a repository without chezmoi comes next.",
                )],
                color: visual::ACCENT,
            };
            return;
        }
        match publish::plan(&self.listing) {
            Err(error) => self.mode = change::done(Err(error), "", None),
            Ok(plan) if plan.pending.is_empty() => {
                let ahead = self.ahead();
                if ahead.is_some_and(|ahead| ahead > 0) {
                    self.ask_push(plan.top, ahead);
                } else {
                    self.mode = Mode::Note {
                        title: "Nothing to export".to_string(),
                        lines: vec![Line::from(
                            "Every listed dotfile's source file is committed, and nothing waits to be pushed.",
                        )],
                        color: visual::ACCENT,
                    };
                }
            }
            Ok(plan) => {
                let target = publish::push_target(&plan.top)
                    .unwrap_or_else(|_| "no remote branch yet".to_string());
                self.mode = Mode::Export(Box::new(Export::new(plan, target)));
            }
        }
    }

    /// `y` in Export: commits the chosen files, then asks about pushing.
    fn commit_export(&mut self) {
        let Mode::Export(mut export_view) = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return;
        };
        if let Err(error) = export::commit(&export_view) {
            export_view.error = Some(error);
            self.mode = Mode::Export(export_view);
            return;
        }
        let ahead = self.ahead().map(|ahead| ahead + 1);
        let top = export_view.plan.top.clone();
        self.reload();
        self.ask_push(top, ahead);
    }

    fn ask_push(&mut self, top: PathBuf, ahead: Option<usize>) {
        self.mode = match publish::push_target(&top) {
            Ok(target) => Mode::Push { top, target, ahead },
            Err(error) => Mode::Note {
                title: "Committed".to_string(),
                lines: vec![
                    Line::from("✓ Committed").green().bold(),
                    Line::from(format!("{error} Push it yourself once it has one.")),
                ],
                color: visual::ACCENT,
            },
        };
    }

    /// `y` on the push question
    fn push(&mut self, top: &Path, target: &str) {
        self.mode = match publish::push(top) {
            Ok(()) => Mode::Note {
                title: "Pushed".to_string(),
                lines: vec![Line::from(format!("✓ Pushed to {target}")).green().bold()],
                color: ratatui::style::Color::Green,
            },
            Err(error) => change::done(Err(error), "", None),
        };
        self.read_repo();
    }

    /// Keys on the list of backups
    fn backups_key(&mut self, key: KeyEvent) {
        let Mode::Backups {
            index,
            list,
            selected,
        } = &mut self.mode
        else {
            return;
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(list.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let (index, backup) = (*index, list[*selected].clone());
                self.ask_restore(index, backup);
            }
            _ => {}
        }
    }

    /// Checks the new contents and shows the review.
    fn review(&mut self, edit: Edit, copy: PathBuf, new: Vec<u8>) {
        let checked = core_change::check(edit.syntax(), &copy, edit.file_name());
        let diff = Diff::new(edit.contents(), &new);
        let hidden = self
            .selected()
            .is_some_and(|index| self.listing.files[index].may_hold_secrets);
        self.mode = Mode::Review(Review {
            edit,
            copy,
            new,
            checked,
            diff,
            hidden,
        });
    }

    /// `c`: opens the selected file's settings, when Configure knows them.
    fn start_configure(&mut self) {
        let Some(index) = self.selected() else {
            return;
        };
        let file = &self.listing.files[index];
        let Some(program) = settings::program_for(file.known.path) else {
            self.mode = Mode::Note {
                title: "No settings".to_string(),
                lines: vec![Line::from(
                    "Configure knows Git's settings for now. Press e to edit this file.",
                )],
                color: visual::ACCENT,
            };
            return;
        };
        let begun = Edit::begin(file, self.listing.chezmoi.as_ref()).and_then(|edit| {
            edit.copy(&self.home)
                .map(|copy| (edit, copy))
                .map_err(|error| format!("A copy to change could not be made: {error}."))
        });
        self.mode = match begun {
            Ok((edit, copy)) => match settings::values(program, &copy) {
                Ok(values) => {
                    Mode::Configure(Box::new(Configure::new(edit, copy, program, values)))
                }
                Err(error) => {
                    core_change::discard(&copy);
                    change::done(Err(error), "", None)
                }
            },
            Err(error) => change::done(Err(error), "", None),
        };
    }

    /// `y` in Configure: makes the changes on the copy, then shows the
    /// review.
    fn review_configure(&mut self) {
        let Mode::Configure(configure) = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return;
        };
        if !configure.has_changes() {
            self.mode = Mode::Configure(configure);
            return;
        }
        if let Err(error) = configure.apply() {
            // Start again from the file as it was, so no half change stays.
            let _ = fs::write(&configure.copy, configure.edit.contents());
            let mut configure = configure;
            configure.error = Some(error);
            self.mode = Mode::Configure(configure);
            return;
        }
        let configure = *configure;
        match change::read_copy(&configure.copy) {
            Ok(new) => self.review(configure.edit, configure.copy, new),
            Err(error) => {
                core_change::discard(&configure.copy);
                self.mode = change::done(Err(error), "", None);
            }
        }
    }

    /// Draws the review, the question, or the note on top of the list.
    fn draw_mode(&self, frame: &mut Frame, area: Rect) {
        match &self.mode {
            Mode::Browse | Mode::Editing { .. } => {}
            Mode::Actions(menu) => menu.draw(frame, area, &self.listing.files[menu.index]),
            Mode::Review(review) => {
                change::draw_review(frame, area, review, self.writes(review));
            }
            Mode::Ask { fix, index, diff } => {
                let (title, lines) = self.ask_lines(*fix, *index, diff);
                let hidden = self.listing.files[*index].may_hold_secrets;
                change::draw_ask(frame, area, &title, lines, diff, hidden);
            }
            Mode::Backups {
                index,
                list,
                selected,
            } => {
                let name = self.listing.files[*index].known.path;
                change::draw_backups(frame, area, name, list, *selected);
            }
            Mode::Restore {
                index,
                backup,
                diff,
            } => {
                let file = &self.listing.files[*index];
                let shown = format!("~/{}", file.known.path);
                let mut lines = vec![
                    Line::from(vec![
                        Span::raw(format!("{:<9}", "Restores")).bold(),
                        Span::raw(format!("{shown} as it was at {}", change::saved(backup))),
                    ]),
                    Line::from(vec![
                        Span::raw(format!("{:<9}", "Backup")).bold(),
                        Span::raw(format!("{shown} as it is now, first")),
                    ]),
                    change_counts(diff),
                ];
                if file.managed.as_ref().and_then(Managed::source).is_some() {
                    lines.push(Line::from(
                        "It may then differ from its saved copy. r saves it there.",
                    ));
                }
                let title = format!(" Restore {} ", file.known.path);
                change::draw_ask(frame, area, &title, lines, diff, file.may_hold_secrets);
            }
            Mode::Show {
                title,
                lines,
                diff,
                hidden,
            } => change::draw_ask(frame, area, title, lines.clone(), diff, *hidden),
            Mode::Export(export_view) => export_view.draw(frame, area),
            Mode::Push { target, ahead, .. } => super::tools::message(
                frame,
                area,
                "Push",
                export::push_lines(target, *ahead),
                visual::ACCENT,
            ),
            Mode::Configure(configure) => {
                let shown = self.selected().map_or_else(String::new, |index| {
                    format!("~/{}", self.listing.files[index].known.path)
                });
                configure.draw(frame, area, &shown);
            }
            Mode::Note {
                title,
                lines,
                color,
            } => super::tools::message(frame, area, title, lines.clone(), *color),
        }
    }

    /// `b`: lists the selected file's backups.
    fn open_backups(&mut self) {
        let Some(index) = self.selected() else {
            return;
        };
        let name = self.listing.files[index].known.path;
        self.mode = match self.backups.list(name) {
            Ok(list) if list.is_empty() => Mode::Note {
                title: "No backups".to_string(),
                lines: vec![Line::from(format!(
                    "~/{name} has no backups yet. neet saves one before every change."
                ))],
                color: visual::ACCENT,
            },
            Ok(list) => Mode::Backups {
                index,
                list,
                selected: 0,
            },
            Err(error) => change::done(
                Err(format!("Its backups could not be read: {error}.")),
                "",
                None,
            ),
        };
    }

    /// `Enter` on a backup: shows what restoring it changes, and asks.
    fn ask_restore(&mut self, index: usize, backup: Backup) {
        let file = &self.listing.files[index];
        let (Ok(now), Ok(then)) = (fs::read(&file.path), fs::read(&backup.path)) else {
            self.mode = change::done(
                Err("The file or its backup could not be read.".to_string()),
                "",
                None,
            );
            return;
        };
        let diff = Diff::new(&now, &then);
        self.mode = Mode::Restore {
            index,
            backup,
            diff,
        };
    }

    /// `y` on the restore question
    fn run_restore(&mut self, index: usize, backup: &Backup) {
        let file = &self.listing.files[index];
        let result = core_change::restore(file, backup, &self.backups, SystemTime::now());
        let shown = format!("~/{}", file.known.path);
        self.mode = change::done(result, &shown, None);
        self.reload();
    }

    /// `d`: how the file differs from its source file, or else what changed
    /// since its last backup
    fn show_diff(&mut self) {
        let Some(index) = self.selected() else {
            return;
        };
        let file = &self.listing.files[index];
        let shown = format!("~/{}", file.known.path);
        let note = |text: String| Mode::Note {
            title: "Nothing to compare".to_string(),
            lines: vec![Line::from(text)],
            color: visual::ACCENT,
        };
        let Ok(now) = fs::read(&file.path) else {
            self.mode = note(format!("{shown} could not be read."));
            return;
        };
        if let Some(Managed::Differs(source)) = &file.managed {
            let Ok(then) = fs::read(source) else {
                self.mode = note("Its source file could not be read.".to_string());
                return;
            };
            let diff = Diff::new(&then, &now);
            let source = self.source_name(file).unwrap_or_default();
            self.mode = Mode::Show {
                title: format!(" {} and its source file ", file.known.path),
                lines: vec![
                    Line::from(vec![
                        Span::raw(format!("{:<9}", "From")).bold(),
                        Span::raw(format!("{source} in chezmoi")),
                    ]),
                    Line::from(vec![
                        Span::raw(format!("{:<9}", "To")).bold(),
                        Span::raw(format!("{shown}, as it is now")),
                    ]),
                    change_counts(&diff),
                ],
                diff,
                hidden: file.may_hold_secrets,
            };
            return;
        }
        let latest = self
            .backups
            .list(file.known.path)
            .ok()
            .and_then(|list| list.into_iter().next());
        let Some(backup) = latest else {
            self.mode = note(format!(
                "{shown} matches its source file or is not in chezmoi, and has no backups yet."
            ));
            return;
        };
        let Ok(then) = fs::read(&backup.path) else {
            self.mode = note("Its last backup could not be read.".to_string());
            return;
        };
        let diff = Diff::new(&then, &now);
        self.mode = Mode::Show {
            title: format!(" {} since its last backup ", file.known.path),
            lines: vec![
                Line::from(vec![
                    Span::raw(format!("{:<9}", "From")).bold(),
                    Span::raw(format!("the backup at {}", change::saved(&backup))),
                ]),
                Line::from(vec![
                    Span::raw(format!("{:<9}", "To")).bold(),
                    Span::raw(format!("{shown}, as it is now")),
                ]),
                change_counts(&diff),
            ],
            diff,
            hidden: file.may_hold_secrets,
        };
    }

    /// The question's lines for `r`, `p`, or `a`
    fn ask_lines(&self, fix: Fix, index: usize, diff: &Diff) -> (String, Vec<Line<'static>>) {
        let file = &self.listing.files[index];
        let shown = format!("~/{}", file.known.path);
        let source = self.source_name(file).unwrap_or_default();
        let may_run = self
            .listing
            .chezmoi
            .as_ref()
            .is_some_and(|chezmoi| chezmoi.may_not_run.is_none());
        let field = |label: &str, value: String| {
            Line::from(vec![
                Span::raw(format!("{label:<9}")).bold(),
                Span::raw(value),
            ])
        };
        let mut counts = vec![Span::raw(format!("{:<9}", "Changes")).bold()];
        counts.push(Span::raw(format!("+{}", diff.added)).green().bold());
        counts.push(Span::raw(" "));
        counts.push(Span::raw(format!("−{}", diff.removed)).red().bold());
        match fix {
            Fix::Keep => (
                format!(" Save my version of {} ", file.known.path),
                vec![
                    field("Keeps", format!("{shown} as it is now, in chezmoi")),
                    field(
                        "Writes",
                        if may_run {
                            format!("{source}, with chezmoi re-add")
                        } else {
                            source.clone()
                        },
                    ),
                    field("Backup", format!("{source} first")),
                    Line::from(counts),
                ],
            ),
            Fix::PutBack => (
                format!(" Use the saved version of {} ", file.known.path),
                vec![
                    field("Replaces", format!("{shown} with {source}")),
                    field("Runs", format!("chezmoi apply {shown}")),
                    field("Backup", format!("{shown} first")),
                    Line::from(counts),
                ],
            ),
            Fix::Add => {
                let name = core_change::add_name(file, self.listing.chezmoi.as_ref())
                    .unwrap_or_else(|error| error);
                let mut lines = vec![
                    field("Adds", format!("{name} in chezmoi")),
                    if may_run {
                        field("Runs", format!("chezmoi add {shown}"))
                    } else {
                        field(
                            "Writes",
                            "the source file itself, as chezmoi add would".to_string(),
                        )
                    },
                    field("Keeps", format!("{shown} as it is")),
                    Line::from(counts),
                ];
                if file.may_hold_secrets {
                    lines.push(
                        Line::from(
                            "It may hold a token. Export leaves it out unless you put it in.",
                        )
                        .red(),
                    );
                }
                (format!(" Add {} to your dotfiles ", file.known.path), lines)
            }
        }
    }
}

fn empty() -> Listing {
    Listing {
        files: Vec::new(),
        chezmoi: None,
    }
}

/// One file as a table row
/// A file's name in the list, without `.config/`
fn shown_name(file: &Dotfile) -> &'static str {
    file.known
        .path
        .strip_prefix(".config/")
        .unwrap_or(file.known.path)
}

fn file_row(file: &Dotfile, columns: &Columns<2>) -> Row<'static> {
    let name = shown_name(file);
    let state = State::of(file);
    let mut status = vec![state.styled(format!("{} {}", state.symbol(), state.word()))];
    if file.may_hold_secrets && file.found.is_some() {
        status.push(Span::raw(" · "));
        status.push(Span::raw("secret").red());
    }
    columns.row([
        Cell::from(format!(
            "  {}",
            format::shorten_middle(name, columns.width(0).saturating_sub(2))
        )),
        Cell::from(Line::from(status)),
    ])
}

/// How a file stands, as the list says it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Its saved copy in chezmoi matches.
    Saved,
    /// It differs from its saved copy.
    Changed,
    /// chezmoi has no copy of it.
    NotSaved,
    /// `.chezmoiignore` leaves it out.
    LeftOut,
    ViewOnly,
    Missing,
    /// chezmoi is not in use.
    OnItsOwn,
}

impl State {
    fn of(file: &Dotfile) -> Self {
        if file.found.is_none() {
            return Self::Missing;
        }
        if file.view_only.is_some() {
            return Self::ViewOnly;
        }
        match &file.managed {
            Some(Managed::InSync(_)) => Self::Saved,
            Some(Managed::Differs(_)) => Self::Changed,
            Some(Managed::No) => Self::NotSaved,
            Some(Managed::Ignored) => Self::LeftOut,
            Some(Managed::Built(..)) => Self::ViewOnly,
            None => Self::OnItsOwn,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Changed => "changed",
            Self::NotSaved => "not saved",
            Self::LeftOut => "left out",
            Self::ViewOnly => "view only",
            Self::Missing => "not on this Mac",
            Self::OnItsOwn => "on this Mac",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Saved => "✓",
            Self::Changed => "!",
            Self::NotSaved => "+",
            _ => "·",
        }
    }

    /// `text` in this state's color
    fn styled(self, text: String) -> Span<'static> {
        match self {
            Self::Saved => Span::raw(text).green(),
            Self::Changed | Self::ViewOnly => Span::raw(text).yellow(),
            Self::NotSaved => Span::styled(text, visual::ACCENT),
            _ => Span::raw(text),
        }
    }
}

/// `Changes  +3 −1`, colored
fn change_counts(diff: &Diff) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{:<9}", "Changes")).bold(),
        Span::raw(format!("+{}", diff.added)).green().bold(),
        Span::raw(" "),
        Span::raw(format!("−{}", diff.removed)).red().bold(),
    ])
}

/// A label and its value, lined up with the other labels
fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<9}")).bold(), value])
}

/// `text` in the middle of `area`
fn centered(frame: &mut Frame, area: Rect, text: &'static str) {
    let line = Line::from(text);
    let height = visual::wrapped_rows(std::slice::from_ref(&line), area.width);
    let [middle] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(
        Paragraph::new(line).centered().wrap(Wrap { trim: true }),
        middle,
    );
}

impl Screen for Dotfiles {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: &Context) {
        self.poll();
        if let Some(reason) = &self.failed {
            frame.render_widget(
                Paragraph::new(reason.clone())
                    .red()
                    .wrap(Wrap { trim: true })
                    .block(
                        visual::block()
                            .title(" Dotfiles ")
                            .padding(Padding::horizontal(1)),
                    ),
                area,
            );
            return;
        }
        let [top, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
        self.draw_top(frame, top);
        if body.width >= MIN_SIDE_WIDTH && self.selected().is_some() {
            let width = (body.width * 45 / 100).clamp(LIST_WIDTH.0, LIST_WIDTH.1);
            let [list, side] =
                Layout::horizontal([Constraint::Length(width), Constraint::Fill(1)]).areas(body);
            self.draw_list(frame, list);
            self.draw_side(frame, side);
            self.draw_mode(frame, area);
            return;
        }
        let below = self.selected().map(|index| {
            let width = body.width.saturating_sub(4);
            let (name, lines) = self.details(index, usize::from(width));
            let height = (visual::wrapped_rows(&lines, width) + 2).min(body.height / 2);
            (name, lines, height)
        });
        match below {
            Some((name, lines, height)) if body.height >= 20 => {
                let [list, below] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(body);
                self.draw_list(frame, list);
                frame.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                        visual::block()
                            .title(Line::from(format!(" {name} ")).bold())
                            .padding(Padding::horizontal(1)),
                    ),
                    below,
                );
            }
            _ => self.draw_list(frame, body),
        }
        self.draw_mode(frame, area);
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        if !matches!(self.mode, Mode::Browse) {
            return self.mode_key(key);
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Char('g') | KeyCode::Home => self.jump(false),
            KeyCode::Char('G') | KeyCode::End => self.jump(true),
            KeyCode::Char('.') => self.toggle_missing(),
            KeyCode::Enter => self.open_actions(),
            KeyCode::Char('e') => return self.run_act(Act::Edit),
            KeyCode::Char('r') => return self.run_act(Act::SaveMine),
            KeyCode::Char('p') => return self.run_act(Act::UseSaved),
            KeyCode::Char('a') => return self.run_act(Act::Add),
            KeyCode::Char('b') => return self.run_act(Act::Backups),
            KeyCode::Char('d') => return self.run_act(Act::Changes),
            KeyCode::Char('c') => return self.run_act(Act::Configure),
            KeyCode::Char('x') => self.start_export(),
            _ => {}
        }
        Action::None
    }

    fn back(&mut self) -> Action {
        match std::mem::replace(&mut self.mode, Mode::Browse) {
            Mode::Browse => Action::Back,
            Mode::Review(review) => {
                core_change::discard(&review.copy);
                Action::None
            }
            Mode::Editing { edit, copy } => {
                // The editor is still open; keep waiting for it.
                self.mode = Mode::Editing { edit, copy };
                Action::None
            }
            Mode::Export(mut export_view) => {
                if export_view.typing.take().is_some() {
                    self.mode = Mode::Export(export_view);
                }
                Action::None
            }
            Mode::Configure(mut configure) => {
                if configure.typing.take().is_some() {
                    self.mode = Mode::Configure(configure);
                } else {
                    core_change::discard(&configure.copy);
                }
                Action::None
            }
            Mode::Ask { .. }
            | Mode::Actions(_)
            | Mode::Push { .. }
            | Mode::Backups { .. }
            | Mode::Restore { .. }
            | Mode::Show { .. }
            | Mode::Note { .. } => Action::None,
        }
    }

    fn takes_text(&self) -> bool {
        match &self.mode {
            Mode::Configure(configure) => configure.typing.is_some(),
            Mode::Export(export_view) => export_view.typing.is_some(),
            _ => false,
        }
    }

    fn is_dialog(&self) -> bool {
        !matches!(self.mode, Mode::Browse)
    }

    fn edited(&mut self, result: Result<(), String>) {
        let Mode::Editing { edit, copy } = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return;
        };
        let new = result.and_then(|()| change::read_copy(&copy));
        match new {
            Err(error) => {
                core_change::discard(&copy);
                self.mode = change::done(Err(error), "", None);
            }
            Ok(new) if new == edit.contents() => {
                core_change::discard(&copy);
                self.mode = Mode::Note {
                    title: "No changes".to_string(),
                    lines: vec![Line::from("Nothing was written.")],
                    color: visual::ACCENT,
                };
            }
            Ok(new) => self.review(edit, copy, new),
        }
    }

    fn hints(&self) -> &'static str {
        match &self.mode {
            Mode::Browse => "↑↓ move · enter actions · x export · . missing · esc home · ? help",
            Mode::Actions(_) => "↑↓ move · enter choose · esc go back",
            Mode::Editing { .. } => "Waiting for your editor to close",
            Mode::Review(review) if matches!(review.checked, Checked::Failed(_)) => {
                "e fix it · ↑↓ scroll · esc drop the change"
            }
            Mode::Review(_) => "y write · e edit again · ↑↓ scroll · esc drop the change",
            Mode::Ask { fix: Fix::Keep, .. } => "y save it · ↑↓ scroll · esc go back",
            Mode::Ask { fix: Fix::Add, .. } => "y add it · ↑↓ scroll · esc go back",
            Mode::Ask { .. } => "y use it · ↑↓ scroll · esc go back",
            Mode::Backups { .. } => "↑↓ move · enter show the change · esc go back",
            Mode::Restore { .. } => "y restore it · ↑↓ scroll · esc go back",
            Mode::Show { .. } => "↑↓ scroll · any other key go back",
            Mode::Configure(configure) if configure.typing.is_some() => {
                "type the value · enter keep it · esc cancel"
            }
            Mode::Configure(_) => "↑↓ move · enter change · y review · esc drop the changes",
            Mode::Export(export_view) if export_view.typing.is_some() => {
                "type the message · enter keep it · esc cancel"
            }
            Mode::Export(_) => "↑↓ move · space in or out · m message · y commit · esc go back",
            Mode::Push { .. } => "y push · esc not now",
            Mode::Note { .. } => "any key continue",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("g  G", "Jump to the first or last file"),
            ("Enter", "Show what you can do with the file"),
            (
                "e",
                "Edit a copy in your editor, then check, review, and write it",
            ),
            ("r", "Save my version in your dotfiles, when it changed"),
            ("p", "Use the saved version, when it changed"),
            ("a", "Add it to your dotfiles"),
            ("c", "Configure its settings, for Git files"),
            (
                "d",
                "Show the changes since it was saved, or since its last backup",
            ),
            ("b", "List its backups, then restore one"),
            ("x", "Export: review for secrets, commit, then push"),
            (".", "Show or hide the files that are not on this Mac"),
            ("Esc", "Go back"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use crate::ui::visual::tests as view;
    use ratatui::crossterm::event::KeyModifiers;
    use std::fs;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A home folder with a chezmoi source folder, one file in sync, one
    /// that differs, one chezmoi does not manage, and one that may hold a
    /// token
    fn screen() -> (tempfile::TempDir, Dotfiles) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        let source = home.join(".local/share/chezmoi");
        write(&home.join(".zshrc"), "export EDITOR=nvim\n");
        write(&source.join("dot_zshrc"), "export EDITOR=nvim\n");
        write(&home.join(".gitconfig"), "[user]\n\tname = You\n");
        write(&source.join("dot_gitconfig"), "[user]\n\tname = Someone\n");
        write(&home.join(".config/kitty/kitty.conf"), "font_size 14\n");
        write(
            &home.join(".npmrc"),
            "//registry.npmjs.org/:_authToken=npm_secret\n",
        );
        // A template that could run a program, so neet never runs chezmoi
        // in these tests, even where it is installed.
        write(&source.join(".chezmoiignore"), "{{ output \"true\" }}\n");
        let listing = dotfiles::list(&home);
        assert!(listing.chezmoi.as_ref().unwrap().may_not_run.is_some());
        (dir, Dotfiles::from_listing(home, listing))
    }

    /// Plays your editor: writes `text` into the copy `e` made, then says
    /// the editor closed.
    fn edit_to(screen: &mut Dotfiles, text: &str) -> PathBuf {
        let Action::Edit(copy) = screen.start_edit() else {
            panic!("e must ask for the editor");
        };
        fs::write(&copy, text).unwrap();
        screen.edited(Ok(()));
        copy
    }

    fn render(screen: &mut Dotfiles, width: u16, height: u16) -> String {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        view::text(&view::render("dotfiles", screen, &context, width, height))
    }

    fn press(screen: &mut Dotfiles, code: KeyCode) {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        screen.handle_key(KeyEvent::new(code, KeyModifiers::NONE), &context);
    }

    #[test]
    fn lists_files_by_program_with_their_status() {
        let (_dir, mut screen) = screen();
        let text = render(&mut screen, 140, 30);

        assert!(text.contains("Dotfiles"));
        assert!(text.contains("1 saved · 1 changed · 2 not saved"), "{text}");
        for group in ["Shell", "Git", "Terminal", "Tools"] {
            assert!(text.contains(group), "{group}\n{text}");
        }
        assert!(!text.contains("Editors"));
        assert!(text.find("Shell") < text.find("  Git "));
        assert!(text.contains("✓ saved"));
        assert!(text.contains("! changed"));
        assert!(text.contains("+ not saved"));
        assert!(text.contains("+ not saved · secret"));
        assert!(text.contains("kitty/kitty.conf"));
        assert!(!text.contains(".bashrc"));
    }

    #[test]
    fn shows_the_selected_file_with_its_source_and_preview() {
        let (_dir, mut screen) = screen();
        let text = render(&mut screen, 140, 30);

        assert!(text.contains("~/.zshrc"));
        assert!(text.contains("Saved as dot_zshrc"));
        assert!(text.contains("Saved in your dotfiles. Both copies match."));
        assert!(text.contains("Your dotfiles"));
        assert!(text.contains("Folder   ~/.local/share/chezmoi"));
        assert!(text.contains("export EDITOR=nvim"));
    }

    #[test]
    fn moving_skips_group_names_and_hides_a_token() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Down);
        let text = render(&mut screen, 140, 30);
        assert!(text.contains("~/.gitconfig"));
        assert!(text.contains("Changed since it was saved."));

        press(&mut screen, KeyCode::Char('G'));
        let text = render(&mut screen, 140, 30);
        assert!(text.contains("~/.npmrc"));
        assert!(text.contains("Hidden, since it may hold a token."));
        assert!(!text.contains("npm_secret"));

        press(&mut screen, KeyCode::Down);
        assert_eq!(
            screen
                .selected()
                .map(|index| screen.listing.files[index].known.path),
            Some(".npmrc")
        );
        press(&mut screen, KeyCode::Char('g'));
        assert_eq!(
            screen
                .selected()
                .map(|index| screen.listing.files[index].known.path),
            Some(".zshrc")
        );
    }

    #[test]
    fn dot_shows_missing_files_and_keeps_the_selection() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Char('.'));
        let text = render(&mut screen, 140, 60);

        assert!(text.contains(".bashrc"));
        assert!(text.contains("Editors"));
        assert!(text.contains("not on this Mac"));
        assert_eq!(
            screen
                .selected()
                .map(|index| screen.listing.files[index].known.path),
            Some(".gitconfig")
        );

        press(&mut screen, KeyCode::Char('.'));
        assert!(!render(&mut screen, 140, 60).contains(".bashrc"));
    }

    #[test]
    fn says_so_when_no_file_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        let mut screen = Dotfiles::from_listing(home.clone(), dotfiles::list(&home));
        let text = render(&mut screen, 100, 20);

        assert!(text.contains("chezmoi not found"));
        assert!(text.contains("No settings files found. Press . to see the files neet looks for."));
    }

    #[test]
    fn a_tall_wide_screen_fills_the_box_under_the_list() {
        let (_dir, mut screen) = screen();
        let text = render(&mut screen, 200, 50);
        let rows: Vec<&str> = text.lines().collect();
        let row_of = |needle: &str| {
            rows.iter()
                .position(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} missing:\n{text}"))
        };
        // The list is only as wide as it needs; the preview gets the rest.
        let files = rows[1].find('┐').unwrap();
        assert!(files <= usize::from(LIST_WIDTH.1) * 3, "{}", rows[1]);
        assert!(row_of("Repo") < row_of("What the marks mean"));
        assert!(row_of("What the marks mean") < row_of("Saved    none yet"));
        assert!(
            text.contains("+ not saved   not in your dotfiles yet"),
            "{text}"
        );
        assert!(text.contains("secret        may hold a token: kept private"));
        assert!(row_of("Kept in") >= rows.len() - 4, "{text}");
    }

    #[test]
    fn fits_every_size() {
        for (width, height) in view::SIZES {
            let (_dir, mut screen) = screen();
            let text = render(&mut screen, width, height);
            assert!(text.contains("Files"), "{width}x{height}\n{text}");
            assert!(text.contains("zshrc"), "{width}x{height}\n{text}");
            if width >= MIN_SIDE_WIDTH {
                assert!(text.contains("1 changed"), "{width}x{height}\n{text}");
            }
        }
    }

    #[test]
    fn edit_shows_the_check_and_diff_then_writes_the_source_file() {
        let (_dir, mut screen) = screen();
        let copy = edit_to(&mut screen, "export EDITOR=hx\n");
        let text = render(&mut screen, 120, 30);

        assert!(text.contains("Change to .zshrc"), "{text}");
        assert!(text.contains("dot_zshrc in chezmoi. Then run chezmoi apply ~/.zshrc yourself"));
        assert!(text.contains("zsh -n  passed"));
        assert!(text.contains("+1 −1"));
        assert!(text.contains("- export EDITOR=nvim"));
        assert!(text.contains("+ export EDITOR=hx"));
        assert!(screen.is_dialog());

        press(&mut screen, KeyCode::Char('y'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Saved dot_zshrc"), "{text}");
        assert!(text.contains("Run chezmoi apply ~/.zshrc to write ~/.zshrc."));
        let source = screen.home.join(".local/share/chezmoi/dot_zshrc");
        assert_eq!(fs::read_to_string(source).unwrap(), "export EDITOR=hx\n");
        assert_eq!(
            fs::read_to_string(screen.home.join(".zshrc")).unwrap(),
            "export EDITOR=nvim\n"
        );
        assert!(!copy.exists());
        assert_eq!(screen.backups.list(".zshrc").unwrap().len(), 1);
        assert_eq!(screen.backups.list("chezmoi/dot_zshrc").unwrap().len(), 1);

        press(&mut screen, KeyCode::Enter);
        assert!(!screen.is_dialog());
        assert!(render(&mut screen, 140, 30).contains("! changed"));
    }

    #[test]
    fn a_failed_check_cannot_be_written() {
        let (_dir, mut screen) = screen();
        edit_to(&mut screen, "if x\n");
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("zsh -n  failed"), "{text}");
        assert!(text.contains("Press e to fix it."));

        press(&mut screen, KeyCode::Char('y'));
        assert!(matches!(screen.mode, Mode::Review(_)));
        let source = screen.home.join(".local/share/chezmoi/dot_zshrc");
        assert_eq!(fs::read_to_string(source).unwrap(), "export EDITOR=nvim\n");

        let Mode::Review(review) = &screen.mode else {
            unreachable!()
        };
        let copy = review.copy.clone();
        assert!(matches!(screen.back(), Action::None));
        assert!(!copy.exists());
        assert!(!screen.is_dialog());
    }

    #[test]
    fn an_unchanged_edit_writes_nothing() {
        let (_dir, mut screen) = screen();
        edit_to(&mut screen, "export EDITOR=nvim\n");
        assert!(render(&mut screen, 120, 30).contains("Nothing was written."));
        assert_eq!(screen.backups.list(".zshrc").unwrap(), []);
    }

    #[test]
    fn a_file_that_differs_is_fixed_before_it_is_edited() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Down);
        assert!(matches!(screen.start_edit(), Action::None));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Press Enter to save"), "{text}");
        press(&mut screen, KeyCode::Enter);

        press(&mut screen, KeyCode::Char('r'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Save my version of .gitconfig"), "{text}");
        assert!(text.contains("-     name = Someone"));
        assert!(text.contains("+     name = You"));
        press(&mut screen, KeyCode::Char('y'));
        assert!(
            render(&mut screen, 120, 30)
                .contains("Saved this version of ~/.gitconfig in your dotfiles")
        );
        let source = screen.home.join(".local/share/chezmoi/dot_gitconfig");
        assert_eq!(
            fs::read_to_string(source).unwrap(),
            "[user]\n\tname = You\n"
        );

        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen, 140, 30).contains("2 saved"));
    }

    #[test]
    fn a_adds_a_file_after_showing_it_whole() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Char('a'));
        assert!(render(&mut screen, 120, 30).contains("chezmoi already manages it"));
        press(&mut screen, KeyCode::Enter);

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Down);
        assert!(render(&mut screen, 140, 30).contains("Enter adds it"));
        press(&mut screen, KeyCode::Char('a'));
        let text = render(&mut screen, 120, 30);
        assert!(
            text.contains("Add .config/kitty/kitty.conf to your dotfiles"),
            "{text}"
        );
        assert!(
            text.contains("dot_config/kitty/kitty.conf in chezmoi"),
            "{text}"
        );
        assert!(text.contains("+ font_size 14"), "{text}");
        press(&mut screen, KeyCode::Char('y'));
        let text = render(&mut screen, 120, 30);
        assert!(
            text.contains("~/.config/kitty/kitty.conf is now in your dotfiles"),
            "{text}"
        );
        let source = screen
            .home
            .join(".local/share/chezmoi/dot_config/kitty/kitty.conf");
        assert_eq!(fs::read_to_string(source).unwrap(), "font_size 14\n");

        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen, 140, 30).contains("2 saved"));
    }

    #[test]
    fn enter_offers_only_what_can_be_done_with_the_file() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Edit  e"), "{text}");
        assert!(!text.contains("Save my version"));
        press(&mut screen, KeyCode::Esc);
        screen.back();

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen, 120, 30);
        for offered in [
            "Save my version",
            "Use the saved version",
            "Show the changes",
        ] {
            assert!(text.contains(offered), "{offered}\n{text}");
        }
        assert!(!text.contains("Edit  e"), "{text}");
        assert!(!text.contains("Configure Git settings"), "{text}");
        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen, 120, 30).contains("Save my version of .gitconfig"));
        screen.back();

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen, 120, 30).contains("Add to your dotfiles"));
        press(&mut screen, KeyCode::Char('a'));
        assert!(
            render(&mut screen, 120, 30).contains("Add .config/kitty/kitty.conf to your dotfiles")
        );
    }

    #[test]
    fn enter_says_so_when_nothing_can_be_done() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Char('.'));
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen, 120, 40);
        assert!(
            text.contains("There is nothing neet can do with it."),
            "{text}"
        );
    }

    #[test]
    fn put_back_says_what_to_run_when_neet_may_not_run_chezmoi() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Char('p'));
        // The reason differs by Mac, so the note is read before it wraps.
        let Mode::Note { lines, .. } = &screen.mode else {
            panic!("p must say why it cannot run");
        };
        let note: String = lines.iter().map(ToString::to_string).collect();
        assert!(
            note.contains("Run chezmoi apply ~/.gitconfig yourself"),
            "{note}"
        );
        assert_eq!(
            fs::read_to_string(screen.home.join(".gitconfig")).unwrap(),
            "[user]\n\tname = You\n"
        );
    }

    #[test]
    fn a_token_never_shows_in_the_diff() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Char('G'));
        edit_to(&mut screen, "//registry.npmjs.org/:_authToken=npm_other\n");
        let text = render(&mut screen, 120, 30);
        assert!(
            text.contains("Hidden, since it may hold a token."),
            "{text}"
        );
        assert!(!text.contains("npm_"));
    }

    #[test]
    fn b_lists_backups_and_restores_one_after_showing_the_change() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Char('b'));
        assert!(render(&mut screen, 120, 30).contains("has no backups yet"));
        press(&mut screen, KeyCode::Enter);

        edit_to(&mut screen, "export EDITOR=hx\n");
        press(&mut screen, KeyCode::Char('y'));
        press(&mut screen, KeyCode::Enter);
        // The edit wrote chezmoi's source file, so write the home file too,
        // as chezmoi apply would.
        fs::write(screen.home.join(".zshrc"), "export EDITOR=hx\n").unwrap();
        screen.reload();
        assert!(render(&mut screen, 140, 30).contains("Backups  1"));

        press(&mut screen, KeyCode::Char('b'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Backups of .zshrc"), "{text}");
        assert!(text.contains(" UTC"));
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Restore .zshrc"), "{text}");
        assert!(text.contains("- export EDITOR=hx"));
        assert!(text.contains("+ export EDITOR=nvim"));
        assert!(text.contains("r saves it there"));

        press(&mut screen, KeyCode::Char('y'));
        assert!(render(&mut screen, 120, 30).contains("Saved ~/.zshrc"));
        assert_eq!(
            fs::read_to_string(screen.home.join(".zshrc")).unwrap(),
            "export EDITOR=nvim\n"
        );
        assert_eq!(screen.backups.list(".zshrc").unwrap().len(), 2);
    }

    #[test]
    fn d_shows_how_a_file_differs_from_its_source() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Char('d'));
        assert!(render(&mut screen, 120, 30).contains("Nothing to compare"));
        press(&mut screen, KeyCode::Enter);

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Char('d'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains(".gitconfig and its source file"), "{text}");
        assert!(text.contains("dot_gitconfig in chezmoi"));
        assert!(text.contains("-     name = Someone"));
        assert!(text.contains("+     name = You"));
        press(&mut screen, KeyCode::Char('x'));
        assert!(!screen.is_dialog());
    }

    /// A home folder with only a `.gitconfig`, and no chezmoi
    fn git_screen() -> (tempfile::TempDir, Dotfiles) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        write(&home.join(".gitconfig"), "# mine\n[user]\n\tname = You\n");
        let listing = dotfiles::list(&home);
        (dir, Dotfiles::from_listing(home, listing))
    }

    fn typed(screen: &mut Dotfiles, text: &str) {
        for character in text.chars() {
            press(screen, KeyCode::Char(character));
        }
    }

    #[test]
    fn configure_changes_git_settings_then_reviews_them() {
        let (_dir, mut screen) = git_screen();
        press(&mut screen, KeyCode::Char('c'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Git settings · ~/.gitconfig"), "{text}");
        assert!(text.contains("user.name"));
        assert!(text.contains("You"));
        assert!(text.contains("not set"));
        assert!(text.contains("Nothing changed yet."));

        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Enter);
        assert!(screen.takes_text());
        typed(&mut screen, "you@example.com");
        press(&mut screen, KeyCode::Enter);
        assert!(!screen.takes_text());
        for _ in 0..3 {
            press(&mut screen, KeyCode::Down);
        }
        press(&mut screen, KeyCode::Enter);
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("user.email  you@example.com"), "{text}");
        assert!(text.contains("pull.rebase  true"));

        press(&mut screen, KeyCode::Char('y'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Change to .gitconfig"), "{text}");
        assert!(text.contains("git config --list  passed"));
        assert!(text.contains("email = you@example.com"));
        assert!(text.contains("rebase = true"));

        press(&mut screen, KeyCode::Char('y'));
        assert!(render(&mut screen, 120, 30).contains("Saved ~/.gitconfig"));
        let saved = fs::read_to_string(screen.home.join(".gitconfig")).unwrap();
        assert!(
            saved.starts_with("# mine\n[user]\n\tname = You\n"),
            "{saved}"
        );
        assert!(saved.contains("email = you@example.com"));
        assert_eq!(screen.backups.list(".gitconfig").unwrap().len(), 1);
    }

    #[test]
    fn esc_cancels_typing_then_drops_the_changes() {
        let (_dir, mut screen) = git_screen();
        press(&mut screen, KeyCode::Char('c'));
        let Mode::Configure(configure) = &screen.mode else {
            panic!("c must open Configure");
        };
        let copy = configure.copy.clone();
        press(&mut screen, KeyCode::Enter);
        typed(&mut screen, "Other");
        assert!(matches!(screen.back(), Action::None));
        assert!(matches!(screen.mode, Mode::Configure(_)));
        assert!(!screen.takes_text());
        assert!(render(&mut screen, 120, 30).contains("Nothing changed yet."));

        assert!(matches!(screen.back(), Action::None));
        assert!(!screen.is_dialog());
        assert!(!copy.exists());
        assert_eq!(
            fs::read_to_string(screen.home.join(".gitconfig")).unwrap(),
            "# mine\n[user]\n\tname = You\n"
        );
    }

    #[test]
    fn configure_says_which_programs_it_knows() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Char('c'));
        assert!(render(&mut screen, 120, 30).contains("Configure knows Git's settings"));
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// A home folder whose chezmoi source folder is a repository with a
    /// remote, and two source files changed since the last commit
    fn export_screen() -> (tempfile::TempDir, Dotfiles, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let source = home.join(".local/share/chezmoi");
        write(&source.join("dot_zshrc"), "export A=1\n");
        write(&source.join("dot_npmrc"), "x=1\n");
        let remote = root.join("remote.git");
        git(
            &root,
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                remote.to_str().unwrap(),
            ],
        );
        git(&source, &["init", "-q", "-b", "main"]);
        git(&source, &["config", "user.name", "You"]);
        git(&source, &["config", "user.email", "you@example.com"]);
        git(&source, &["add", "."]);
        git(&source, &["commit", "-q", "-m", "first"]);
        git(
            &source,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "-u", "origin", "main"]);
        write(&source.join("dot_zshrc"), "export A=2\n");
        write(&home.join(".zshrc"), "export A=2\n");
        let token = "//registry.npmjs.org/:_authToken=npm_abcdefghijklmnopqrstuvwxyz";
        write(&source.join("dot_npmrc"), &format!("{token}\n"));
        write(&home.join(".npmrc"), &format!("{token}\n"));
        let listing = dotfiles::list(&home);
        (dir, Dotfiles::from_listing(home, listing), remote)
    }

    #[test]
    fn export_reviews_commits_the_chosen_files_and_pushes() {
        let (_dir, mut screen, remote) = export_screen();
        press(&mut screen, KeyCode::Char('x'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Export"), "{text}");
        assert!(text.contains("[x] dot_zshrc"), "{text}");
        assert!(text.contains("nothing found"));
        assert!(text.contains("[ ] dot_npmrc"));
        assert!(text.contains("Update zshrc"));
        assert!(text.contains("to origin/main, after a question"));
        assert!(!text.contains("abcdefgh"));

        press(&mut screen, KeyCode::Down);
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("npm token  npm_****"), "{text}");

        press(&mut screen, KeyCode::Char('m'));
        assert!(screen.takes_text());
        for _ in 0.."Update zshrc".len() {
            press(&mut screen, KeyCode::Backspace);
        }
        for character in "Use hx".chars() {
            press(&mut screen, KeyCode::Char(character));
        }
        press(&mut screen, KeyCode::Enter);
        press(&mut screen, KeyCode::Char('y'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Push your commits to origin/main?"), "{text}");
        let source = screen.home.join(".local/share/chezmoi");
        assert_eq!(git(&source, &["log", "-1", "--format=%s"]).trim(), "Use hx");
        assert_eq!(
            git(&source, &["show", "--name-only", "--format="]).trim(),
            "dot_zshrc"
        );

        press(&mut screen, KeyCode::Char('y'));
        assert!(render(&mut screen, 120, 30).contains("Pushed to origin/main"));
        assert_eq!(
            git(&remote, &["log", "-1", "--format=%s", "main"]).trim(),
            "Use hx"
        );
    }

    #[test]
    fn export_says_when_nothing_waits() {
        let (_dir, mut screen, _remote) = export_screen();
        let source = screen.home.join(".local/share/chezmoi");
        git(&source, &["checkout", "-q", "--", "."]);
        screen.reload();
        press(&mut screen, KeyCode::Char('x'));
        assert!(render(&mut screen, 120, 30).contains("Nothing to export"));
    }
}
