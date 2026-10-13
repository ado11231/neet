//! Chat removal review and progress. File moves run outside the UI thread.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use super::{AiTools, Item, Kind, Review, Viewer, ai, format, visual};
use neet_core::ai::remove::Removal;
use neet_core::ai::summary::Summary;
use ratatui::style::Stylize;
use ratatui::text::Line;

type MoveResult = (PathBuf, Result<(), String>);

pub(super) struct Cleanup {
    receiver: Receiver<MoveResult>,
    pub total: usize,
    pub done: usize,
    removed: usize,
    items: Vec<Item>,
    results: BTreeMap<PathBuf, Result<(), String>>,
}

impl Cleanup {
    pub(super) fn new(items: Vec<Item>, receiver: Receiver<MoveResult>) -> Self {
        Self {
            receiver,
            total: items.len(),
            done: 0,
            removed: 0,
            items,
            results: BTreeMap::new(),
        }
    }
}

impl AiTools {
    fn chat_details(&self, item: &Item, index: usize) -> Vec<Line<'static>> {
        let title = item
            .chat
            .as_ref()
            .map_or("Untitled chat", |chat| chat.title.as_str());
        vec![
            Line::from(format!("{index}. {}", ai::display_text(title))).bold(),
            Line::from(format!(
                "{} · {}",
                item.provider.name(),
                format::size(item.size)
            )),
            Line::from(
                item.chat
                    .as_ref()
                    .map_or_else(|| "Project unknown".into(), Summary::project_label),
            ),
            Line::from(self.shown_path(&item.path)).fg(visual::ACCENT),
            Line::default(),
        ]
    }

    pub(super) fn review_chat_removal(&mut self) {
        let items: Vec<_> = if self.marked.is_empty() {
            self.selected()
                .into_iter()
                .filter(|item| item.kind == Kind::Chat)
                .collect()
        } else {
            self.listing
                .items
                .iter()
                .filter(|item| item.kind == Kind::Chat && self.marked.contains(&item.path))
                .cloned()
                .collect()
        };
        if items.is_empty() {
            self.note = "Select a chat first.".into();
            return;
        }
        let mut removals = Vec::new();
        let mut details = Vec::new();
        let mut skipped = Vec::new();
        let visible: std::collections::BTreeSet<_> = self
            .rows()
            .into_iter()
            .map(|item| item.path.clone())
            .collect();
        let hidden = items
            .iter()
            .filter(|item| !visible.contains(&item.path))
            .count();
        let mut size: u64 = 0;
        for item in items {
            match Removal::prepare(&self.locations, &item) {
                Ok(removal) => {
                    details.extend(self.chat_details(&item, removals.len() + 1));
                    size = size.saturating_add(item.size);
                    removals.push(removal);
                }
                Err(error) => skipped.push((item, error.to_string())),
            }
        }
        let count = removals.len();
        let mut viewer = Viewer::new("Review chat removal".into(), "");
        viewer.summary = vec![
            Line::from(if count == 0 {
                "No chat files ready to move.".into()
            } else {
                format!("Move {count} chat files to Trash?")
            })
            .yellow()
            .bold(),
            Line::from(format!(
                "{} ready · {} · {} skipped · {hidden} hidden by filters",
                count,
                format::size(size),
                skipped.len()
            )),
            Line::from(
                "Local transcripts only. Assistant indexes, related files, and cloud history remain.",
            ),
        ];
        if count > 0 {
            viewer
                .lines
                .push(Line::from("Ready to move").style(visual::HEADING));
            viewer.lines.extend(details);
        }
        if !skipped.is_empty() {
            viewer
                .lines
                .push(Line::from("Skipped before review").yellow().bold());
            for (index, (item, reason)) in skipped.iter().enumerate() {
                viewer.lines.extend(self.chat_details(item, index + 1));
                viewer
                    .lines
                    .push(Line::from(ai::display_text(reason)).yellow());
                viewer.lines.push(Line::default());
            }
        }
        viewer.footer = if count == 0 {
            vec![Line::from(
                "Nothing will be moved. Press Esc, then r to refresh the chat list.",
            )]
        } else {
            vec![
                Line::from("Close assistants first. Open, recent, or changed files are skipped."),
                Line::from("Restore files with Finder's Put Back."),
                Line::from("y move to Trash · n / Esc cancel")
                    .yellow()
                    .bold(),
            ]
        };
        self.viewer = Some(viewer);
        if !removals.is_empty() {
            self.review = Some(Review::Chats(removals));
        }
    }

    pub(super) fn start_chat_removal(&mut self, removals: Vec<Removal>) {
        let items = removals
            .iter()
            .map(|removal| removal.item.clone())
            .collect();
        self.marked
            .extend(removals.iter().map(|removal| removal.item.path.clone()));
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for removal in removals {
                let result = removal.apply().map_err(|error| error.to_string());
                if sender.send((removal.item.path, result)).is_err() {
                    break;
                }
            }
        });
        self.cleanup = Some(Cleanup::new(items, receiver));
        self.viewer = None;
    }

    pub(super) fn poll_cleanup(&mut self) {
        let Some(cleanup) = &mut self.cleanup else {
            return;
        };
        loop {
            match cleanup.receiver.try_recv() {
                Ok((path, result)) => {
                    cleanup.done += 1;
                    if result.is_ok() {
                        cleanup.removed += 1;
                        self.marked.remove(&path);
                    }
                    cleanup.results.insert(path, result);
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        let cleanup = self.cleanup.take().unwrap();
        self.viewer = Some(self.cleanup_viewer(&cleanup));
        self.note = format!(
            "{} of {} chat files moved to Trash.",
            cleanup.removed, cleanup.total
        );
        self.refresh();
    }

    fn cleanup_viewer(&self, cleanup: &Cleanup) -> Viewer {
        let mut viewer = Viewer::new("Chat cleanup result".into(), "");
        let failed = cleanup.done.saturating_sub(cleanup.removed);
        let unchecked = cleanup.total.saturating_sub(cleanup.done);
        viewer.summary = vec![
            Line::from(format!(
                "{} of {} chat files moved to Trash.",
                cleanup.removed, cleanup.total
            ))
            .bold(),
            Line::from(format!(
                "{} moved · {failed} not moved · {unchecked} unconfirmed",
                cleanup.removed
            )),
        ];
        if unchecked > 0 {
            viewer.summary.push(
                Line::from("Cleanup stopped early. Check the Trash before retrying.").yellow(),
            );
        }
        for (group, count) in [
            ("Not moved", failed),
            ("Unconfirmed", unchecked),
            ("Moved to Trash", cleanup.removed),
        ] {
            if count == 0 {
                continue;
            }
            viewer
                .lines
                .push(Line::from(format!("{group} · {count}")).style(visual::HEADING));
            let mut index = 0;
            for item in &cleanup.items {
                let result = cleanup.results.get(&item.path);
                let status = match result {
                    Some(Ok(())) => "Moved to Trash",
                    Some(Err(_)) => "Not moved",
                    None => "Unconfirmed",
                };
                if status != group {
                    continue;
                }
                index += 1;
                viewer.lines.extend(self.chat_details(item, index));
                if let Some(Err(reason)) = result {
                    viewer
                        .lines
                        .push(Line::from(ai::display_text(reason)).yellow());
                    viewer.lines.push(Line::default());
                }
            }
        }
        viewer.footer = vec![
            Line::from("Restore moved files with Finder's Put Back."),
            Line::from("Unmoved chats stay selected. Esc returns to the list."),
        ];
        viewer
    }
}
