//! The menu `Enter` opens on a file: only what can be done with it, each
//! with the key that does it straight from the list.

use neet_core::dotfiles::change as core_change;
use neet_core::dotfiles::configure as settings;
use neet_core::dotfiles::{Chezmoi, Dotfile, Managed};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Padding};

use super::super::visual;

/// One thing the menu offers
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Act {
    Edit,
    /// `r`: chezmoi's source file takes the home folder's version.
    SaveMine,
    /// `p`: the home folder's file takes chezmoi's version.
    UseSaved,
    Add,
    Changes,
    Configure,
    Backups,
}

impl Act {
    /// The key that does it from the list
    pub(super) fn key(self) -> char {
        match self {
            Self::Edit => 'e',
            Self::SaveMine => 'r',
            Self::UseSaved => 'p',
            Self::Add => 'a',
            Self::Changes => 'd',
            Self::Configure => 'c',
            Self::Backups => 'b',
        }
    }

    fn label(self, file: &Dotfile) -> String {
        match self {
            Self::Edit => "Edit".to_string(),
            Self::SaveMine => "Save my version".to_string(),
            Self::UseSaved => "Use the saved version".to_string(),
            Self::Add => "Add to your dotfiles".to_string(),
            Self::Changes => "Show the changes".to_string(),
            Self::Configure => settings::program_for(file.known.path).map_or_else(
                || "Configure".to_string(),
                |program| format!("Configure {} settings", program.name),
            ),
            Self::Backups => "Backups".to_string(),
        }
    }
}

/// What can be done with `file`, in the order the menu shows it
pub(super) fn for_file(file: &Dotfile, chezmoi: Option<&Chezmoi>, backups: usize) -> Vec<Act> {
    let mut acts = Vec::new();
    if file.found.is_none() {
        return acts;
    }
    let changeable = file.view_only.is_none();
    let differs = matches!(file.managed, Some(Managed::Differs(_)));
    if changeable && differs {
        acts.extend([Act::SaveMine, Act::UseSaved]);
    }
    if changeable && !differs {
        acts.push(Act::Edit);
    }
    if core_change::add_name(file, chezmoi).is_ok() {
        acts.push(Act::Add);
    }
    if differs || backups > 0 {
        acts.push(Act::Changes);
    }
    if changeable && !differs && settings::program_for(file.known.path).is_some() {
        acts.push(Act::Configure);
    }
    if backups > 0 {
        acts.push(Act::Backups);
    }
    acts
}

/// The open menu
pub(super) struct Menu {
    pub(super) index: usize,
    pub(super) acts: Vec<Act>,
    pub(super) state: ListState,
}

impl Menu {
    pub(super) fn new(index: usize, acts: Vec<Act>) -> Self {
        Self {
            index,
            acts,
            state: ListState::default().with_selected(Some(0)),
        }
    }

    pub(super) fn step(&mut self, down: bool) {
        let last = self.acts.len().saturating_sub(1);
        let now = self.state.selected().unwrap_or(0);
        self.state.select(Some(if down {
            (now + 1).min(last)
        } else {
            now.saturating_sub(1)
        }));
    }

    pub(super) fn chosen(&self) -> Option<Act> {
        self.acts.get(self.state.selected()?).copied()
    }

    /// A small box in the middle, sized to its items
    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, file: &Dotfile) {
        let labels: Vec<String> = self.acts.iter().map(|act| act.label(file)).collect();
        let longest = labels.iter().map(String::len).max().unwrap_or(0);
        let name = file
            .known
            .path
            .rsplit('/')
            .next()
            .unwrap_or(file.known.path);
        let width = u16::try_from(longest.max(name.len()) + 12)
            .unwrap_or(u16::MAX)
            .min(area.width);
        let height = u16::try_from(self.acts.len() + 2)
            .unwrap_or(u16::MAX)
            .min(area.height);
        let [middle] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        let [middle] = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .areas(middle);
        let items: Vec<ListItem> = self
            .acts
            .iter()
            .zip(labels)
            .map(|(act, label)| {
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{label:<longest$}  ")),
                    Span::styled(act.key().to_string(), visual::HEADING),
                ]))
            })
            .collect();
        frame.render_widget(Clear, middle);
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    visual::block()
                        .title(format!(" {name} "))
                        .padding(Padding::horizontal(1)),
                )
                .highlight_symbol("▸ ")
                .highlight_style(visual::SELECTED),
            middle,
            &mut self.state.clone(),
        );
    }
}
