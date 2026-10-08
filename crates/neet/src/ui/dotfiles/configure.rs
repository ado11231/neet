//! Configure: a program's settings, one at a time, changed on the edit copy
//! and then reviewed like an edit.

use std::path::PathBuf;

use neet_core::dotfiles::change::Edit;
use neet_core::dotfiles::configure::{self, Kind, Program};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap};

use super::super::format;
use super::super::visual::{self, Columns};

/// What a setting changes to
#[derive(Clone, PartialEq, Eq)]
pub(in crate::ui) enum NewValue {
    Set(String),
    Unset,
}

impl NewValue {
    fn from(value: Option<String>) -> Self {
        value.map_or(Self::Unset, Self::Set)
    }

    fn as_deref(&self) -> Option<&str> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unset => None,
        }
    }
}

/// A program's settings, open for changes
pub(in crate::ui) struct Configure {
    pub edit: Edit,
    pub copy: PathBuf,
    pub program: &'static Program,
    /// Each setting's value in the file, in the program's order
    pub values: Vec<Option<String>>,
    /// The value each setting will have, where it changes
    pub changes: Vec<Option<NewValue>>,
    pub table: TableState,
    /// The text being typed for the selected setting
    pub typing: Option<String>,
    /// Why the last change was refused
    pub error: Option<String>,
}

impl Configure {
    pub fn new(
        edit: Edit,
        copy: PathBuf,
        program: &'static Program,
        values: Vec<Option<String>>,
    ) -> Self {
        let count = program.settings.len();
        Self {
            edit,
            copy,
            program,
            values,
            changes: vec![None; count],
            table: TableState::default().with_selected(Some(0)),
            typing: None,
            error: None,
        }
    }

    fn selected(&self) -> usize {
        self.table.selected().unwrap_or(0)
    }

    /// The value a setting will have
    fn value(&self, index: usize) -> Option<&str> {
        match &self.changes[index] {
            Some(change) => change.as_deref(),
            None => self.values[index].as_deref(),
        }
    }

    /// Keeps a new value, or forgets the change when it is the value now.
    fn change(&mut self, index: usize, value: Option<String>) {
        self.changes[index] = (value != self.values[index]).then(|| NewValue::from(value));
        self.error = None;
    }

    /// Handles a key. Returns whether `y` asked for the review.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        if let Some(text) = &mut self.typing {
            match key.code {
                KeyCode::Enter => self.enter(),
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(character) => text.push(character),
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Enter => self.enter(),
            KeyCode::Char('y') => return true,
            _ => {}
        }
        false
    }

    fn step(&mut self, down: bool) {
        let last = self.program.settings.len() - 1;
        let index = self.selected();
        self.table.select(Some(if down {
            (index + 1).min(last)
        } else {
            index.saturating_sub(1)
        }));
    }

    /// `Enter`: settings with choices step through each, then not set.
    /// Text and numbers open for typing, or keep what was typed when it is
    /// a value the setting takes.
    fn enter(&mut self) {
        let index = self.selected();
        let setting = &self.program.settings[index];
        if let Some(text) = self.typing.take() {
            let text = text.trim().to_string();
            if text.is_empty() {
                self.change(index, None);
            } else if let Err(error) = configure::check(setting, &text) {
                self.typing = Some(text);
                self.error = Some(error);
            } else {
                self.change(index, Some(text));
            }
            return;
        }
        match setting.kind {
            Kind::Choice(choices) => {
                let next = match self
                    .value(index)
                    .and_then(|now| choices.iter().position(|choice| *choice == now))
                {
                    Some(at) => choices.get(at + 1).map(|next| (*next).to_string()),
                    None => choices.first().map(|first| (*first).to_string()),
                };
                self.change(index, next);
            }
            Kind::Text | Kind::Number | Kind::Decimal => {
                self.typing = Some(self.value(index).unwrap_or_default().to_string());
            }
        }
    }

    /// Applies every change to the copy, in order.
    ///
    /// # Errors
    ///
    /// Returns the first change Git refused.
    pub fn apply(&self) -> Result<(), String> {
        for (setting, change) in self.program.settings.iter().zip(&self.changes) {
            if let Some(value) = change {
                configure::set(self.program, &self.copy, setting, value.as_deref())?;
            }
        }
        Ok(())
    }

    pub fn has_changes(&self) -> bool {
        self.changes.iter().any(Option::is_some)
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, shown: &str) {
        frame.render_widget(Clear, area);
        let pending: Vec<Line<'static>> = self
            .program
            .settings
            .iter()
            .zip(&self.changes)
            .filter_map(|(setting, change)| {
                let change = change.as_ref()?;
                Some(Line::from(vec![
                    Span::raw(format!("{}  ", setting.key)).bold(),
                    Span::raw(change.as_deref().unwrap_or("not set").to_string()).yellow(),
                ]))
            })
            .collect();
        let mut below = pending;
        if below.is_empty() {
            below.push(Line::from(
                "Nothing changed yet. Enter changes the selected setting.",
            ));
        }
        if self.program.format == configure::Format::Zsh {
            below.push(Line::from(
                "neet writes these only between the # >>> neet >>> lines at the end of the file. Every line above them stays as it is.",
            ));
        }
        if let Some(error) = &self.error {
            below.push(Line::from(error.clone()).red());
        }
        let columns = self.columns(area.width);
        let rows: u16 = (0..self.program.settings.len())
            .map(|index| u16::try_from(self.about(index, &columns).len()).unwrap_or(1))
            .sum();
        // The borders and the header
        let table_height = (rows + 3).min(area.height.saturating_sub(4));
        let below_height = visual::wrapped_rows(&below, area.width.saturating_sub(4)) + 2;
        let typing_height = if self.typing.is_some() { 3 } else { 0 };
        let [table, typing, changes, _] = Layout::vertical([
            Constraint::Length(table_height),
            Constraint::Length(typing_height),
            Constraint::Length(below_height),
            Constraint::Fill(1),
        ])
        .areas(area);
        self.draw_table(frame, table, shown);
        if let Some(text) = &self.typing {
            let key = self.program.settings[self.selected()].key;
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(text.clone()),
                    Span::raw("▏").fg(visual::ACCENT),
                ]))
                .block(
                    visual::block()
                        .title(format!(" {key} "))
                        .padding(Padding::horizontal(1)),
                ),
                typing,
            );
        }
        frame.render_widget(
            Paragraph::new(below).wrap(Wrap { trim: false }).block(
                visual::block()
                    .title(" Changes ")
                    .padding(Padding::horizontal(1)),
            ),
            changes,
        );
    }

    /// Settings and values as wide as they need, and About gets the rest
    fn columns(&self, width: u16) -> Columns<3> {
        let keys = self
            .program
            .settings
            .iter()
            .map(|setting| setting.key.len())
            .max()
            .unwrap_or(0);
        let values = (0..self.program.settings.len())
            .map(|index| format::display_width(self.value(index).unwrap_or("not set")))
            .chain([format::display_width("Value")])
            .max()
            .unwrap_or(0)
            .min(32);
        Columns::new(
            width,
            [
                (u16::try_from(keys).unwrap_or(20), 0),
                (u16::try_from(values).unwrap_or(16), 0),
                (24, 1),
            ],
            &[2],
            true,
        )
    }

    /// The setting's About, wrapped to its column, one line at least
    fn about(&self, index: usize, columns: &Columns<3>) -> Vec<String> {
        let about = self.program.settings[index].about;
        let width = columns.width(2);
        if width == 0 {
            return vec![String::new()];
        }
        let mut lines: Vec<String> = Vec::new();
        for word in about.split_whitespace() {
            match lines.last_mut() {
                Some(line)
                    if format::display_width(line) + 1 + format::display_width(word) <= width =>
                {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_string()),
            }
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    fn draw_table(&self, frame: &mut Frame, area: Rect, shown: &str) {
        let values: Vec<String> = (0..self.program.settings.len())
            .map(|index| self.value(index).unwrap_or("not set").to_string())
            .collect();
        let columns = self.columns(area.width);
        let rows: Vec<Row> = self
            .program
            .settings
            .iter()
            .enumerate()
            .map(|(index, setting)| {
                let value = format::shorten_middle(&values[index], columns.width(1));
                let value = if self.changes[index].is_some() {
                    Span::raw(value).yellow().bold()
                } else if self.values[index].is_none() {
                    Span::raw(value)
                } else {
                    Span::raw(value).fg(visual::ACCENT)
                };
                let about = self.about(index, &columns);
                let height = u16::try_from(about.len()).unwrap_or(1);
                columns
                    .row([
                        Cell::from(setting.key),
                        Cell::from(value),
                        Cell::from(Text::raw(about.join("\n"))),
                    ])
                    .height(height)
            })
            .collect();
        let header = columns
            .row([
                Cell::from("Setting"),
                Cell::from("Value"),
                Cell::from("About"),
            ])
            .style(visual::HEADING);
        let table = Table::new(rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(
                visual::block()
                    .title(Line::from(format!(" {} settings · {shown} ", self.program.name)).bold())
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .row_highlight_style(visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table.clone());
    }
}
