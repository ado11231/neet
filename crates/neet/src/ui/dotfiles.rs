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

mod change;

use change::{Diff, Fix, Mode, Review};
use neet_core::rewrite::Backup;

/// From this width the selected file and its preview show on the right.
const MIN_SIDE_WIDTH: u16 = 120;

/// Width of the right side
const SIDE_WIDTH: u16 = 48;

/// Width of the last changed column, such as `11 months ago`
const AGE_WIDTH: u16 = 13;

/// Width of the status column, such as `may hold secrets`
const STATUS_WIDTH: u16 = 16;

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

/// Lists your settings files grouped by program, with how each stands with
/// chezmoi. `e` edits one, and `r` and `p` fix one that differs from
/// chezmoi's source file.
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

    /// The line at the top: chezmoi, its repository, and what waits to be
    /// committed or pushed
    fn draw_top(&self, frame: &mut Frame, area: Rect) {
        let mut spans = vec![Span::raw(" ")];
        match &self.listing.chezmoi {
            None => spans.push(Span::raw("chezmoi not found")),
            Some(chezmoi) => {
                spans.push(Span::raw("chezmoi").bold());
                match &self.repo {
                    RepoState::Reading(_) => spans.push(Span::raw(" · looking…")),
                    RepoState::Read(None) => spans.push(Span::raw(" · not in a Git repository")),
                    RepoState::Read(Some(repo)) => {
                        spans.push(Span::raw(" · "));
                        spans.push(Span::raw(
                            repo.remote
                                .clone()
                                .unwrap_or_else(|| "no remote".to_string()),
                        ));
                        spans.push(Span::raw(" · "));
                        spans.push(match repo.uncommitted {
                            0 => Span::raw("nothing to commit").green(),
                            count => Span::raw(format!("{count} to commit")).yellow(),
                        });
                        spans.push(Span::raw(" · "));
                        spans.push(match repo.ahead_behind {
                            None => Span::raw("not tracking a remote branch"),
                            Some((0, 0)) => Span::raw("up to date").green(),
                            Some((ahead, 0)) => Span::raw(format!("{ahead} to push")).yellow(),
                            Some((0, behind)) => Span::raw(format!("{behind} to pull")).yellow(),
                            Some((ahead, behind)) => {
                                Span::raw(format!("{ahead} to push, {behind} to pull")).yellow()
                            }
                        });
                    }
                }
                if chezmoi.may_not_run.is_some() {
                    spans.push(Span::raw(" · "));
                    spans.push(Span::raw("neet will not run chezmoi").yellow());
                }
            }
        }
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

    /// The file table, sized to its rows, with the summary below it when
    /// there is room
    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let rows = u16::try_from(self.items().len()).unwrap_or(u16::MAX).max(1);
        let table_height = rows.saturating_add(4);
        let summary = self.summary();
        let summary_height =
            visual::wrapped_rows(&summary, area.width.saturating_sub(4)).saturating_add(2);
        if area.height < table_height.saturating_add(summary_height) {
            self.draw_table(frame, area);
            return;
        }
        let [table, below, _] = Layout::vertical([
            Constraint::Length(table_height),
            Constraint::Length(summary_height),
            Constraint::Fill(1),
        ])
        .areas(area);
        self.draw_table(frame, table);
        frame.render_widget(
            Paragraph::new(summary).wrap(Wrap { trim: false }).block(
                visual::block()
                    .title(" chezmoi ")
                    .padding(Padding::horizontal(1)),
            ),
            below,
        );
    }

    /// How the files stand together, and what each status means
    fn summary(&self) -> Vec<Line<'static>> {
        let Some(chezmoi) = &self.listing.chezmoi else {
            return vec![
                Line::from("chezmoi was not found, so each file is shown on its own."),
                Line::from(
                    "chezmoi keeps your dotfiles in a Git repository and sets up a new Mac from it.",
                ),
            ];
        };
        let found: Vec<&Dotfile> = self
            .listing
            .files
            .iter()
            .filter(|file| file.found.is_some())
            .collect();
        let count = |status: &str| {
            found
                .iter()
                .filter(|file| status_text(file) == status)
                .count()
        };
        let mut lines = vec![
            field("Source", Span::raw(self.tilde(&chezmoi.source))),
            field(
                "May run",
                match &chezmoi.may_not_run {
                    None => Span::raw("apply and re-add, one file at a time").green(),
                    Some(reason) => Span::raw(format!("never: {reason}")).yellow(),
                },
            ),
            Line::default(),
        ];
        let statuses: [(&str, Span<'static>, &str); 5] = [
            (
                "in sync",
                Span::raw("in sync").green(),
                "matches its source file in chezmoi",
            ),
            (
                "differs",
                Span::raw("differs").yellow(),
                "does not match its source file",
            ),
            (
                "not in chezmoi",
                Span::raw("not in chezmoi"),
                "chezmoi does not manage it",
            ),
            (
                "view only",
                Span::raw("view only").yellow(),
                "neet will not change it; the details say why",
            ),
            (
                "may hold secrets",
                Span::raw("may hold secrets").red(),
                "preview hidden, left out of exports",
            ),
        ];
        for (status, span, about) in statuses {
            let number = count(status);
            if number == 0 {
                continue;
            }
            let label = span.content.len();
            lines.push(Line::from(vec![
                Span::raw(format!("{number:>2} ")).bold(),
                span,
                Span::raw(format!("{:width$}  {about}", "", width = 16 - label)),
            ]));
        }
        lines
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect) {
        let items = self.items();
        let found = self
            .listing
            .files
            .iter()
            .filter(|file| file.found.is_some())
            .count();
        let summary = format!(
            " {found} {} found ",
            if found == 1 { "file" } else { "files" }
        );
        let block = visual::block()
            .title(" Files ")
            .title_bottom(Line::from(summary).right_aligned())
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
        let sizes = format::column_width(
            "Size",
            files
                .iter()
                .filter_map(|file| file.found.as_ref())
                .map(|found| format::size(found.size)),
        );
        let columns = Columns::new(
            area.width,
            [(14, 1), (sizes, 0), (AGE_WIDTH, 0), (STATUS_WIDTH, 0)],
            &[2],
            true,
        );
        let now = SystemTime::now();
        let rows: Vec<Row> = items
            .iter()
            .map(|item| match *item {
                Item::Group(group) => columns.row([
                    Cell::from(Span::styled(group.name(), visual::HEADING)),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ]),
                Item::File(index) => file_row(&files[index], now, &columns),
            })
            .collect();
        let header = columns
            .row([
                Cell::from("Name"),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from("Changed"),
                Cell::from("Status"),
            ])
            .style(visual::HEADING)
            .bottom_margin(1);
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(block)
            .highlight_symbol("▸ ")
            .row_highlight_style(visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// The selected file's name, and lines on where it is, how it stands,
    /// and what that means. Paths are shortened to fit `width`.
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
            if let (Some(source), Some(chezmoi)) = (
                file.managed.as_ref().and_then(Managed::source),
                &self.listing.chezmoi,
            ) {
                let source = source.strip_prefix(&chezmoi.source).unwrap_or(source);
                lines.push(field(
                    "Source",
                    Span::raw(format::shorten_path(&source.display().to_string(), room)),
                ));
            }
            lines.push(field(
                "Size",
                format::size_span(found.size, format::size(found.size)),
            ));
            let changed = found
                .modified
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .map_or_else(|| "unknown".to_string(), format::age);
            lines.push(field("Changed", Span::raw(changed)));
            lines.push(field("Mode", Span::raw(format!("{:o}", found.mode))));
        }
        lines.push(field("Check", Span::raw(file.known.syntax.name())));
        if file.found.is_some() {
            let count = self
                .backups
                .list(file.known.path)
                .map_or(0, |list| list.len());
            lines.push(field("Backups", Span::raw(count.to_string())));
        }
        lines.push(Line::default());
        lines.extend(self.notes(file));
        (name, lines)
    }

    /// What the file's status means, in plain words
    fn notes(&self, file: &Dotfile) -> Vec<Line<'static>> {
        let mut notes = Vec::new();
        if file.found.is_none() {
            notes.push(Line::from("Not on this Mac."));
            return notes;
        }
        if file.may_hold_secrets {
            notes.push(
                Line::from("It may hold a token. Its preview is hidden, and it starts left out of exports.")
                    .red(),
            );
        }
        if let Some(view_only) = &file.view_only {
            notes.push(Line::from(format!("{}. View only.", view_only.describe())).yellow());
        }
        match &file.managed {
            Some(Managed::InSync(_)) => {
                notes.push(Line::from("It matches its source file in chezmoi.").green());
            }
            Some(Managed::Differs(_)) => {
                notes.push(Line::from("It differs from its source file in chezmoi.").yellow());
            }
            Some(Managed::Ignored) => notes.push(Line::from(".chezmoiignore leaves it out.")),
            Some(Managed::No) => notes.push(Line::from("chezmoi does not manage it.")),
            Some(Managed::Built(..)) | None => {}
        }
        if let (Some(managed), Some(chezmoi)) = (&file.managed, &self.listing.chezmoi)
            && managed.source().is_some()
            && let Some(reason) = &chezmoi.may_not_run
        {
            notes.push(Line::from(format!("neet will not run chezmoi: {reason}.")).yellow());
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
            Fix::Keep => Diff::new(&source, &home),
            Fix::PutBack => Diff::new(&home, &source),
        };
        self.mode = Mode::Ask { fix, index, diff };
    }

    /// `y` on the question: runs the fix.
    fn run_fix(&mut self, fix: Fix, index: usize) {
        let file = &self.listing.files[index];
        let chezmoi = self.listing.chezmoi.as_ref();
        let now = SystemTime::now();
        let result = match fix {
            Fix::Keep => core_change::keep_home(file, chezmoi, &self.backups, now),
            Fix::PutBack => core_change::put_back(file, chezmoi, &self.backups, now),
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

    /// Draws the review, the question, or the note on top of the list.
    fn draw_mode(&self, frame: &mut Frame, area: Rect) {
        match &self.mode {
            Mode::Browse | Mode::Editing { .. } => {}
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
                        "It may then differ from its source file in chezmoi. r keeps it there.",
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

    /// The question's lines for `r` or `p`
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
                format!(" Keep this version of {} ", file.known.path),
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
                format!(" Put chezmoi's version of {} back ", file.known.path),
                vec![
                    field("Replaces", format!("{shown} with {source}")),
                    field("Runs", format!("chezmoi apply {shown}")),
                    field("Backup", format!("{shown} first")),
                    Line::from(counts),
                ],
            ),
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
fn file_row(file: &Dotfile, now: SystemTime, columns: &Columns<4>) -> Row<'static> {
    let name = file
        .known
        .path
        .strip_prefix(".config/")
        .unwrap_or(file.known.path);
    let (size, changed) = match &file.found {
        Some(found) => (
            format::size_span(found.size, format::size(found.size)),
            found
                .modified
                .and_then(|modified| now.duration_since(modified).ok())
                .map_or_else(|| "unknown".to_string(), format::age),
        ),
        None => (Span::raw(""), String::new()),
    };
    columns.row([
        Cell::from(format!(
            "  {}",
            format::shorten_middle(name, columns.width(0).saturating_sub(2))
        )),
        Cell::from(Line::from(size).right_aligned()),
        Cell::from(changed),
        Cell::from(status(file)),
    ])
}

/// The status the list shows, in words
fn status_text(file: &Dotfile) -> &'static str {
    if file.found.is_none() {
        return "missing";
    }
    if file.may_hold_secrets {
        return "may hold secrets";
    }
    if file.view_only.is_some() {
        return "view only";
    }
    match &file.managed {
        Some(Managed::InSync(_)) => "in sync",
        Some(Managed::Differs(_)) => "differs",
        Some(Managed::Ignored) => "ignored",
        Some(Managed::No) => "not in chezmoi",
        Some(Managed::Built(..)) | None => "",
    }
}

/// The status, colored by what it means
fn status(file: &Dotfile) -> Span<'static> {
    let text = status_text(file);
    match text {
        "in sync" => Span::raw(text).green(),
        "differs" | "view only" => Span::raw(text).yellow(),
        "may hold secrets" => Span::raw(text).red(),
        _ => Span::raw(text),
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
        if body.width >= MIN_SIDE_WIDTH {
            let [list, side] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDE_WIDTH)])
                    .areas(body);
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
        match &mut self.mode {
            Mode::Browse => {}
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
            Mode::Backups {
                index,
                list,
                selected,
            } => {
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
            Mode::Show { diff, .. } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => diff.scroll(false),
                    KeyCode::Down | KeyCode::Char('j') => diff.scroll(true),
                    _ => self.mode = Mode::Browse,
                }
                return Action::None;
            }
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Char('g') | KeyCode::Home => self.jump(false),
            KeyCode::Char('G') | KeyCode::End => self.jump(true),
            KeyCode::Char('.') => self.toggle_missing(),
            KeyCode::Char('e') => return self.start_edit(),
            KeyCode::Char('r') => self.ask_fix(Fix::Keep),
            KeyCode::Char('p') => self.ask_fix(Fix::PutBack),
            KeyCode::Char('b') => self.open_backups(),
            KeyCode::Char('d') => self.show_diff(),
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
            Mode::Ask { .. }
            | Mode::Backups { .. }
            | Mode::Restore { .. }
            | Mode::Show { .. }
            | Mode::Note { .. } => Action::None,
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
            Ok(new) => {
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
        }
    }

    fn hints(&self) -> &'static str {
        match &self.mode {
            Mode::Browse => {
                "↑↓ move · e edit · d diff · b backups · r keep · p put back · . missing · esc home · ? help"
            }
            Mode::Editing { .. } => "Waiting for your editor to close",
            Mode::Review(review) if matches!(review.checked, Checked::Failed(_)) => {
                "e fix it · ↑↓ scroll · esc drop the change"
            }
            Mode::Review(_) => "y write · e edit again · ↑↓ scroll · esc drop the change",
            Mode::Ask { fix: Fix::Keep, .. } => "y keep this version · ↑↓ scroll · esc go back",
            Mode::Ask { .. } => "y put it back · ↑↓ scroll · esc go back",
            Mode::Backups { .. } => "↑↓ move · enter show the change · esc go back",
            Mode::Restore { .. } => "y restore it · ↑↓ scroll · esc go back",
            Mode::Show { .. } => "↑↓ scroll · any other key go back",
            Mode::Note { .. } => "any key continue",
        }
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("g  G", "Jump to the first or last file"),
            (
                "e",
                "Edit a copy in your editor, then check, review, and write it",
            ),
            ("r", "Keep this version in chezmoi, when it differs"),
            ("p", "Put chezmoi's version back, when it differs"),
            (
                "d",
                "Show how it differs from its source file or last backup",
            ),
            ("b", "List its backups, then restore one"),
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
        assert!(text.contains("4 files found"));
        for group in ["Shell", "Git", "Terminal", "Tools"] {
            assert!(text.contains(group), "{group}\n{text}");
        }
        assert!(!text.contains("Editors"));
        assert!(text.find("Shell") < text.find("  Git "));
        assert!(text.contains("in sync"));
        assert!(text.contains("differs"));
        assert!(text.contains("not in chezmoi"));
        assert!(text.contains("may hold secrets"));
        assert!(text.contains("kitty/kitty.conf"));
        assert!(!text.contains(".bashrc"));
    }

    #[test]
    fn shows_the_selected_file_with_its_source_and_preview() {
        let (_dir, mut screen) = screen();
        let text = render(&mut screen, 140, 30);

        assert!(text.contains("~/.zshrc"));
        assert!(text.contains("Source   dot_zshrc"));
        assert!(text.contains("Check    zsh -n"));
        assert!(text.contains("It matches its source file in chezmoi."));
        assert!(text.contains("export EDITOR=nvim"));
    }

    #[test]
    fn moving_skips_group_names_and_hides_a_token() {
        let (_dir, mut screen) = screen();
        press(&mut screen, KeyCode::Down);
        let text = render(&mut screen, 140, 30);
        assert!(text.contains("~/.gitconfig"));
        assert!(text.contains("It differs from its source file in chezmoi."));

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
        assert!(text.contains("missing"));
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
    fn fits_every_size() {
        for (width, height) in view::SIZES {
            let (_dir, mut screen) = screen();
            let text = render(&mut screen, width, height);
            assert!(text.contains("Files"), "{width}x{height}\n{text}");
            assert!(text.contains("zshrc"), "{width}x{height}\n{text}");
            if width >= MIN_SIDE_WIDTH {
                assert!(text.contains(" 1 differs "), "{width}x{height}\n{text}");
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
        assert!(render(&mut screen, 140, 30).contains("differs"));
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
        assert!(text.contains("Press r to keep this"), "{text}");
        press(&mut screen, KeyCode::Enter);

        press(&mut screen, KeyCode::Char('r'));
        let text = render(&mut screen, 120, 30);
        assert!(text.contains("Keep this version of .gitconfig"), "{text}");
        assert!(text.contains("-     name = Someone"));
        assert!(text.contains("+     name = You"));
        press(&mut screen, KeyCode::Char('y'));
        assert!(render(&mut screen, 120, 30).contains("chezmoi now keeps this version"));
        let source = screen.home.join(".local/share/chezmoi/dot_gitconfig");
        assert_eq!(
            fs::read_to_string(source).unwrap(),
            "[user]\n\tname = You\n"
        );

        press(&mut screen, KeyCode::Enter);
        assert!(render(&mut screen, 140, 30).contains("in sync"));
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
        assert!(text.contains("r keeps it there"));

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
}
