//! Chat removal review and progress. File moves run outside the UI thread.

use std::fmt::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use super::{AiTools, Kind, Review, Viewer, format};
use neet_core::ai::remove::Removal;
use neet_core::ai::summary::Summary;

type MoveResult = (PathBuf, Result<(), String>);

pub(super) struct Cleanup {
    receiver: Receiver<MoveResult>,
    pub total: usize,
    pub done: usize,
    removed: usize,
    failures: Vec<String>,
}

impl Cleanup {
    pub(super) fn new(total: usize, receiver: Receiver<MoveResult>) -> Self {
        Self {
            receiver,
            total,
            done: 0,
            removed: 0,
            failures: Vec::new(),
        }
    }
}

impl AiTools {
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
        let mut size: u64 = 0;
        for item in items {
            match Removal::prepare(&self.locations, &item) {
                Ok(removal) => {
                    let title = item
                        .chat
                        .as_ref()
                        .map_or("Untitled chat", |chat| chat.title.as_str());
                    details.push(format!(
                        "{} · {}\n{}\n{}",
                        title,
                        item.provider.name(),
                        self.shown_path(&item.path),
                        item.chat
                            .as_ref()
                            .map_or_else(String::new, Summary::project_label)
                    ));
                    size = size.saturating_add(item.size);
                    removals.push(removal);
                }
                Err(error) => skipped.push(format!("{}: {error}", self.shown_path(&item.path))),
            }
        }
        let count = removals.len();
        let mut text = format!(
            "Move {count} chat files ({}) to Trash?\n\nClose the assistants first. Recently updated, open, or changed files are skipped.\nRestore files with Finder's Put Back.\n\nThis removes local transcript files only. Assistant indexes, related files, and cloud history remain.\n\ny moves the listed files. Esc cancels.\n\n{}",
            format::size(size),
            details.join("\n\n")
        );
        if !skipped.is_empty() {
            let _ = write!(text, "\n\nSkipped before review:\n{}", skipped.join("\n"));
        }
        self.viewer = Some(Viewer::new("Review chat removal".into(), &text));
        if !removals.is_empty() {
            self.review = Some(Review::Chats(removals));
        }
    }

    pub(super) fn start_chat_removal(&mut self, removals: Vec<Removal>) {
        let total = removals.len();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for removal in removals {
                let result = removal.apply().map_err(|error| error.to_string());
                if sender.send((removal.item.path, result)).is_err() {
                    break;
                }
            }
        });
        self.cleanup = Some(Cleanup::new(total, receiver));
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
                    match result {
                        Ok(()) => {
                            cleanup.removed += 1;
                            self.marked.remove(&path);
                        }
                        Err(error) => cleanup
                            .failures
                            .push(format!("{}: {error}", path.display())),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        let cleanup = self.cleanup.take().unwrap();
        let mut text = format!(
            "{} of {} chat files moved to Trash.\nRestore them with Finder's Put Back.",
            cleanup.removed, cleanup.total
        );
        if cleanup.done != cleanup.total {
            text.push_str("\nCleanup stopped unexpectedly. Check the Trash before retrying.");
        }
        if !cleanup.failures.is_empty() {
            let _ = write!(text, "\n\nNot moved:\n{}", cleanup.failures.join("\n\n"));
        }
        self.viewer = Some(Viewer::new("Chat cleanup result".into(), &text));
        self.note = format!(
            "{} of {} chat files moved to Trash.",
            cleanup.removed, cleanup.total
        );
        self.refresh();
    }
}
