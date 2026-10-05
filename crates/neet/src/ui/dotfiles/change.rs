//! Editing a dotfile, and fixing one that differs from chezmoi's source
//! file: the review with its check and diff, the question, and what
//! happened.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use neet_core::dotfiles::change::{self, Checked, Done, Edit};
use neet_core::rewrite::Backup;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph, Wrap};
use similar::{ChangeTag, TextDiff};

use super::super::visual;

/// Which fix `r` or `p` asked for
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Fix {
    /// `r`: chezmoi's source file takes the home folder's version.
    Keep,
    /// `p`: the home folder's file takes chezmoi's version.
    PutBack,
}

/// What the screen is doing besides listing files
pub(super) enum Mode {
    Browse,
    /// Your editor is open on `copy`.
    Editing {
        edit: Edit,
        copy: PathBuf,
    },
    Review(Review),
    Ask {
        fix: Fix,
        index: usize,
        diff: Diff,
    },
    /// The selected file's backups, newest first
    Backups {
        index: usize,
        list: Vec<Backup>,
        selected: usize,
    },
    /// A backup to put back, waiting for `y`
    Restore {
        index: usize,
        backup: Backup,
        diff: Diff,
    },
    /// `d`: how the file differs from its source file or its last backup
    Show {
        title: String,
        lines: Vec<Line<'static>>,
        diff: Diff,
        hidden: bool,
    },
    /// `c`: a program's settings, open for changes
    Configure(Box<super::configure::Configure>),
    /// What happened, until a key is pressed
    Note {
        title: String,
        lines: Vec<Line<'static>>,
        color: Color,
    },
}

/// An edit waiting for `y`
pub(super) struct Review {
    pub edit: Edit,
    pub copy: PathBuf,
    pub new: Vec<u8>,
    pub checked: Checked,
    pub diff: Diff,
    /// Hidden for files that may hold a token
    pub hidden: bool,
}

/// Lines of a diff, ready to draw, and how far it is scrolled
pub(super) struct Diff {
    pub lines: Vec<Line<'static>>,
    pub added: usize,
    pub removed: usize,
    pub scroll: u16,
}

impl Diff {
    /// The changes from `old` to `new`, with three lines around each
    pub fn new(old: &[u8], new: &[u8]) -> Self {
        let old = String::from_utf8_lossy(old);
        let new = String::from_utf8_lossy(new);
        let diff = TextDiff::from_lines(old.as_ref(), new.as_ref());
        let (mut added, mut removed) = (0, 0);
        let mut lines = Vec::new();
        for (index, group) in diff.grouped_ops(3).iter().enumerate() {
            if index > 0 {
                lines.push(Line::default());
            }
            if let Some(first) = group.first() {
                lines.push(
                    Line::from(format!("line {}", first.new_range().start + 1))
                        .style(visual::HEADING),
                );
            }
            for op in group {
                for change in diff.iter_changes(op) {
                    let text = change
                        .value()
                        .trim_end_matches(['\n', '\r'])
                        .replace('\t', "    ");
                    lines.push(match change.tag() {
                        ChangeTag::Delete => {
                            removed += 1;
                            Line::from(format!("- {text}")).red()
                        }
                        ChangeTag::Insert => {
                            added += 1;
                            Line::from(format!("+ {text}")).green()
                        }
                        ChangeTag::Equal => Line::from(format!("  {text}")),
                    });
                }
            }
        }
        Self {
            lines,
            added,
            removed,
            scroll: 0,
        }
    }

    pub fn scroll(&mut self, down: bool) {
        let last = u16::try_from(self.lines.len().saturating_sub(1)).unwrap_or(u16::MAX);
        self.scroll = if down {
            (self.scroll + 1).min(last)
        } else {
            self.scroll.saturating_sub(1)
        };
    }

    /// `+3 −1`, colored
    fn counts(&self) -> Vec<Span<'static>> {
        vec![
            Span::raw(format!("+{}", self.added)).green().bold(),
            Span::raw(" "),
            Span::raw(format!("−{}", self.removed)).red().bold(),
        ]
    }
}

/// A label and its value, lined up
fn field(label: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{label:<9}")).bold()];
    spans.extend(value);
    Line::from(spans)
}

/// The review of an edit: what is written, the backup, the check, and the
/// diff below
pub(super) fn draw_review(frame: &mut Frame, area: Rect, review: &Review, writes: String) {
    let mut lines = vec![
        field("Writes", vec![Span::raw(writes)]),
        field(
            "Backup",
            vec![Span::raw("first, in ~/.local/state/neet/backups/dotfiles")],
        ),
    ];
    let check = review.edit.syntax().name();
    lines.push(match &review.checked {
        Checked::Passed => field(
            "Check",
            vec![
                Span::raw(format!("{check}  ")),
                Span::raw("passed").green().bold(),
            ],
        ),
        Checked::Failed(_) => field(
            "Check",
            vec![
                Span::raw(format!("{check}  ")),
                Span::raw("failed").red().bold(),
            ],
        ),
        Checked::NotChecked(why) => field("Check", vec![Span::raw(why.clone())]),
    });
    if let Checked::Failed(message) = &review.checked {
        for line in message.lines().take(6) {
            lines.push(Line::from(format!("         {line}")).red());
        }
        lines.push(
            Line::from("         Press e to fix it. A file that fails its check is never written.")
                .red(),
        );
    }
    lines.push(field("Changes", review.diff.counts()));
    let title = format!(" Change to {} ", review.edit.file_name());
    draw_with_diff(frame, area, &title, lines, &review.diff, review.hidden);
}

/// The question before `r` or `p`, with the diff of what changes
pub(super) fn draw_ask(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    diff: &Diff,
    hidden: bool,
) {
    draw_with_diff(frame, area, title, lines, diff, hidden);
}

fn draw_with_diff(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    diff: &Diff,
    hidden: bool,
) {
    frame.render_widget(Clear, area);
    let width = area.width.saturating_sub(4);
    let height = (visual::wrapped_rows(&lines, width) + 2).min(area.height / 2);
    let [top, below] =
        Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            visual::block()
                .title(Line::from(title.to_string()).bold())
                .padding(Padding::horizontal(1)),
        ),
        top,
    );
    let rows = if hidden { 1 } else { diff.lines.len() };
    let fits = u16::try_from(rows).unwrap_or(u16::MAX).saturating_add(2);
    let [below, _] = Layout::vertical([Constraint::Length(fits), Constraint::Fill(1)]).areas(below);
    let block = visual::block()
        .title(" Diff ")
        .padding(Padding::horizontal(1));
    if hidden {
        let inner = block.inner(below);
        frame.render_widget(block, below);
        frame.render_widget(
            Paragraph::new("Hidden, since it may hold a token.").centered(),
            inner,
        );
        return;
    }
    let shown = u16::try_from(diff.lines.len()).unwrap_or(u16::MAX);
    let block = if shown > below.height.saturating_sub(2) {
        block.title_bottom(
            Line::from(format!(" line {} of {shown} · ↑↓ scroll ", diff.scroll + 1))
                .right_aligned(),
        )
    } else {
        block
    };
    frame.render_widget(
        Paragraph::new(diff.lines.clone())
            .scroll((diff.scroll, 0))
            .block(block),
        below,
    );
}

/// What an edit or fix did, as a title, lines, and a color
pub(super) fn done(result: Result<Done, String>, file: &str, source: Option<&str>) -> Mode {
    let source = source.unwrap_or(file);
    let (title, lines, color) = match result {
        Ok(Done::Written) => (
            "Saved",
            vec![
                Line::from(format!("✓ Saved {file}")).green().bold(),
                Line::from("The old version is in the backups."),
            ],
            Color::Green,
        ),
        Ok(Done::Applied) => (
            "Saved",
            vec![
                Line::from(format!("✓ Saved {source} and applied it"))
                    .green()
                    .bold(),
                Line::from(format!(
                    "chezmoi wrote {file}. Commit it when you are ready."
                )),
            ],
            Color::Green,
        ),
        Ok(Done::Kept) => (
            "Kept",
            vec![
                Line::from(format!("✓ chezmoi now keeps this version of {file}"))
                    .green()
                    .bold(),
                Line::from(format!(
                    "{source} was backed up first. Commit it when you are ready."
                )),
            ],
            Color::Green,
        ),
        Ok(Done::ApplyYourself(command)) => (
            "Saved",
            vec![
                Line::from(format!("✓ Saved {source}")).green().bold(),
                Line::from(format!("Run {command} to write {file}.")).yellow(),
            ],
            Color::Yellow,
        ),
        Ok(Done::ApplyFailed { error, command }) => (
            "chezmoi failed",
            vec![
                Line::from(format!("Saved {source}, but chezmoi failed."))
                    .red()
                    .bold(),
                Line::from(error),
                Line::from(format!("Run {command} once it is fixed.")),
            ],
            Color::Red,
        ),
        Err(error) => ("Not changed", vec![Line::from(error)], Color::Red),
    };
    Mode::Note {
        title: title.to_string(),
        lines,
        color,
    }
}

/// Reads what the editor left in `copy`
pub(super) fn read_copy(copy: &std::path::Path) -> Result<Vec<u8>, String> {
    fs::read(copy).map_err(|error| format!("The edited copy could not be read: {error}."))
}

/// Writes an edit, then removes its copy.
pub(super) fn finish(
    review: &Review,
    backups: &neet_core::rewrite::Backups,
) -> Result<Done, String> {
    let result = review.edit.finish(&review.new, backups, SystemTime::now());
    change::discard(&review.copy);
    result
}

/// A backup's name as a time to read, such as `2026-10-05 16:30:12 UTC`
pub(super) fn saved(backup: &Backup) -> String {
    let stamp = backup.saved.get(..19).unwrap_or(&backup.saved);
    match stamp.split_once('T') {
        Some((day, time)) => format!("{day} {} UTC", time.replace('-', ":")),
        None => backup.saved.clone(),
    }
}

/// The list of backups, in a box sized to fit, with the selected one bold
pub(super) fn draw_backups(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    list: &[Backup],
    selected: usize,
) {
    let rows = usize::from(area.height.saturating_sub(4)).max(1);
    let first = selected.saturating_sub(rows.saturating_sub(1));
    let lines: Vec<Line<'static>> = list
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .map(|(index, backup)| {
            let size = super::super::format::size(backup.size);
            let text = format!("{}   {size:>8}", saved(backup));
            if index == selected {
                Line::from(vec![Span::raw("▸ "), Span::raw(text)]).style(visual::SELECTED)
            } else {
                Line::from(format!("  {text}"))
            }
        })
        .collect();
    let width = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .saturating_add(4)
        .max(44);
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(ratatui::layout::Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(ratatui::layout::Flex::Center)
        .areas(area);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            visual::block()
                .title(Line::from(format!(" Backups of {name} ")).bold())
                .title_bottom(Line::from(format!(" {} ", list.len())).right_aligned())
                .padding(Padding::horizontal(1)),
        ),
        area,
    );
}
