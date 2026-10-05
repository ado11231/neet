//! Export: the review for secrets, the files to commit, the message, and
//! then the push question.

use neet_core::dotfiles::export::{self, Pending, Plan};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph, Wrap};

use super::super::visual;

/// What a key asked Export to do
pub(in crate::ui) enum Step {
    Stay,
    Commit,
}

/// The files waiting to be committed, which go in, and the message
pub(in crate::ui) struct Export {
    pub plan: Plan,
    pub chosen: Vec<bool>,
    pub selected: usize,
    pub message: String,
    /// The message being typed
    pub typing: Option<String>,
    /// Why the last commit was refused
    pub error: Option<String>,
    /// The remote branch a push goes to, such as `origin/main`
    pub target: String,
}

impl Export {
    /// Files with findings, and files that may hold a token, start left out.
    pub fn new(plan: Plan, target: String) -> Self {
        let chosen: Vec<bool> = plan
            .pending
            .iter()
            .map(|pending| pending.findings.is_empty() && !pending.may_hold_secrets)
            .collect();
        let message = message(&plan.pending, &chosen);
        Self {
            plan,
            chosen,
            selected: 0,
            message,
            typing: None,
            error: None,
            target,
        }
    }

    /// The paths that go in
    pub fn paths(&self) -> Vec<&str> {
        self.plan
            .pending
            .iter()
            .zip(&self.chosen)
            .filter(|(_, chosen)| **chosen)
            .map(|(pending, _)| pending.path.as_str())
            .collect()
    }

    pub fn key(&mut self, key: KeyEvent) -> Step {
        if let Some(text) = &mut self.typing {
            match key.code {
                KeyCode::Enter => {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        self.message = text;
                    }
                    self.typing = None;
                }
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(character) => text.push(character),
                _ => {}
            }
            return Step::Stay;
        }
        let last = self.plan.pending.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::Char(' ') => {
                let was_default = self.message == message(&self.plan.pending, &self.chosen);
                self.chosen[self.selected] = !self.chosen[self.selected];
                if was_default {
                    self.message = message(&self.plan.pending, &self.chosen);
                }
                self.error = None;
            }
            KeyCode::Char('m') => self.typing = Some(self.message.clone()),
            KeyCode::Char('y') => return Step::Commit,
            _ => {}
        }
        Step::Stay
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let remote = &self.target;
        frame.render_widget(Clear, area);
        let mut lines = vec![Line::from("Review for secrets, then pick what goes in.").bold()];
        lines.push(Line::default());
        for (index, (pending, chosen)) in self.plan.pending.iter().zip(&self.chosen).enumerate() {
            let mark = if *chosen {
                Span::raw("[x] ").green().bold()
            } else {
                Span::raw("[ ] ")
            };
            let status = if !pending.findings.is_empty() {
                let count = pending.findings.len();
                Span::raw(format!(
                    "{count} {} that may be secret",
                    if count == 1 { "line" } else { "lines" }
                ))
                .red()
            } else if pending.may_hold_secrets {
                Span::raw("may hold secrets").red()
            } else {
                Span::raw("nothing found").green()
            };
            let mut line = Line::from(vec![
                Span::raw(if index == self.selected { "▸ " } else { "  " }),
                mark,
                Span::raw(format!("{:<28}", pending.path)),
                status,
            ]);
            if index == self.selected {
                line = line.style(visual::SELECTED);
            }
            lines.push(line);
            if index == self.selected {
                for finding in &pending.findings {
                    lines.push(
                        Line::from(format!(
                            "        line {}  {}  {}",
                            finding.line, finding.kind, finding.start
                        ))
                        .red(),
                    );
                }
            }
        }
        lines.push(Line::default());
        let count = self.paths().len();
        lines.push(Line::from(vec![
            Span::raw(format!("{:<9}", "Commit")).bold(),
            Span::raw(format!(
                "{count} {} · \"{}\"",
                if count == 1 { "file" } else { "files" },
                self.message
            )),
        ]));
        lines.push(Line::from(vec![
            Span::raw(format!("{:<9}", "Push")).bold(),
            Span::raw(format!("to {remote}, after a question")),
        ]));
        lines.push(Line::default());
        lines.push(Line::from(
            "This is a review, not a promise that nothing secret remains.",
        ));
        if let Some(error) = &self.error {
            lines.push(Line::from(error.clone()).red());
        }
        let width = area.width.saturating_sub(4);
        let height = (visual::wrapped_rows(&lines, width) + 2).min(area.height);
        let typing = if self.typing.is_some() { 3 } else { 0 };
        let [body, box_area, _] = Layout::vertical([
            Constraint::Length(height),
            Constraint::Length(typing),
            Constraint::Fill(1),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                visual::block()
                    .title(Line::from(" Export ").bold())
                    .padding(Padding::horizontal(1)),
            ),
            body,
        );
        if let Some(text) = &self.typing {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(text.clone()),
                    Span::raw("▏").fg(visual::ACCENT),
                ]))
                .block(
                    visual::block()
                        .title(" Commit message ")
                        .padding(Padding::horizontal(1)),
                ),
                box_area,
            );
        }
    }
}

/// A message naming the chosen files, such as `Update zshrc and gitconfig`
fn message(pending: &[Pending], chosen: &[bool]) -> String {
    let names: Vec<&str> = pending
        .iter()
        .zip(chosen)
        .filter(|(_, chosen)| **chosen)
        .map(|(pending, _)| {
            let name = pending.file.rsplit('/').next().unwrap_or(pending.file);
            name.trim_start_matches('.')
        })
        .collect();
    match names.as_slice() {
        [] => "Update dotfiles".to_string(),
        [one] => format!("Update {one}"),
        [first, second] => format!("Update {first} and {second}"),
        [first, rest @ ..] => format!("Update {first} and {} more", rest.len()),
    }
}

/// The lines of the push question
pub(in crate::ui) fn push_lines(target: &str, ahead: Option<usize>) -> Vec<Line<'static>> {
    let commits = match ahead {
        Some(1) => "1 commit".to_string(),
        Some(count) => format!("{count} commits"),
        None => "your commits".to_string(),
    };
    vec![
        Line::from(format!("Push {commits} to {target}?")).bold(),
        Line::default(),
        Line::from(
            "neet never forces a push. If the remote has commits you do not, nothing is pushed.",
        ),
        Line::default(),
        Line::from(vec![
            Span::raw("y").fg(visual::ACCENT).bold(),
            Span::raw(" push · "),
            Span::raw("esc").fg(visual::ACCENT).bold(),
            Span::raw(" not now"),
        ]),
    ]
}

/// Commits the chosen files.
///
/// # Errors
///
/// Returns Git's message, or why nothing was chosen.
pub(in crate::ui) fn commit(export_view: &Export) -> Result<(), String> {
    export::commit(
        &export_view.plan.top,
        &export_view.paths(),
        &export_view.message,
    )
}
