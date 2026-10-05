use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::SystemTime;

use neet_core::dotfiles::{self, Dotfile, Group, Listing, Managed, Repo};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::app::{Action, Context, Screen};
use super::format;
use super::visual::{self, Columns};

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
/// chezmoi. View only for now.
pub struct Dotfiles {
    home: PathBuf,
    listing: Listing,
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
        if let Some(chezmoi) = &screen.listing.chezmoi {
            let (sender, receiver) = mpsc::channel();
            let source = chezmoi.source.clone();
            thread::spawn(move || {
                // The receiver is gone only when the screen was closed.
                let _ = sender.send(dotfiles::repo(&source));
            });
            screen.repo = RepoState::Reading(receiver);
        }
        screen
    }

    fn from_listing(home: PathBuf, listing: Listing) -> Self {
        let mut screen = Self {
            home,
            listing,
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
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Char('g') | KeyCode::Home => self.jump(false),
            KeyCode::Char('G') | KeyCode::End => self.jump(true),
            KeyCode::Char('.') => self.toggle_missing(),
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · . missing files · esc home · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move selection"),
            ("g  G", "Jump to the first or last file"),
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
        let listing = dotfiles::list(&home);
        (dir, Dotfiles::from_listing(home, listing))
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
}
