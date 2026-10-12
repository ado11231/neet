//! Bounded chat titles and project paths. Unknown paths are never called missing.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde_json::Value;

use super::{Kind, Listing, Locations, Provider, display_text, read_bounded};

const SUMMARY_BYTES: u64 = 512 * 1024;
const SCAN_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub title: String,
    pub project: Option<PathBuf>,
    pub missing_project: bool,
    pub note: String,
}

impl Summary {
    #[must_use]
    pub fn project_label(&self) -> String {
        self.project.as_ref().map_or_else(
            || "Project unknown".into(),
            |path| {
                format!(
                    "{}: {}",
                    if self.missing_project {
                        "Missing project"
                    } else {
                        "Project"
                    },
                    display_text(&path.display().to_string())
                )
            },
        )
    }
}

impl Listing {
    /// Reads bounded summaries when the Chats tab is opened. Settings stay closed.
    pub fn summarize_chats(&mut self, locations: &Locations) {
        let mut budget = SCAN_BYTES;
        for item in self.items.iter_mut().filter(|item| item.kind == Kind::Chat) {
            let mut summary = if budget == 0 {
                Summary {
                    note: "Summary scan limit reached. Enter previews this chat.".into(),
                    ..Summary::default()
                }
            } else {
                let limit = SUMMARY_BYTES.min(budget);
                budget = budget.saturating_sub(item.size.min(limit).saturating_add(1));
                match read_bounded(&locations.home, &item.path, limit) {
                    Ok((bytes, truncated)) => {
                        let mut summary = parse(item.provider, &bytes);
                        if truncated {
                            summary.note = "Summary uses the start of this file.".into();
                        }
                        summary
                    }
                    Err(error) => Summary {
                        note: error.to_string(),
                        ..Summary::default()
                    },
                }
            };
            if let Some(project) = &summary.project {
                match fs::metadata(project) {
                    Ok(meta) => summary.missing_project = !meta.is_dir(),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        summary.missing_project = true
                    }
                    Err(_) => summary.note = "Could not check the project directory.".into(),
                }
            }
            item.chat = Some(summary);
        }
    }
}

fn parse(provider: Provider, bytes: &[u8]) -> Summary {
    let mut summary = Summary::default();
    let records: Vec<Value> = if provider == Provider::Gemini {
        serde_json::from_slice(bytes).map_or_else(|_| lines(bytes), |value| vec![value])
    } else {
        lines(bytes)
    };
    for value in &records {
        let project = match provider {
            Provider::Codex if value["type"] == "session_meta" => value["payload"]["cwd"].as_str(),
            Provider::Claude => value["cwd"].as_str(),
            Provider::Gemini => value["cwd"]
                .as_str()
                .or_else(|| value["projectRoot"].as_str()),
            _ => None,
        };
        if summary.project.is_none() {
            summary.project = project.map(PathBuf::from).filter(|path| path.is_absolute());
        }
        let title = value["customTitle"]
            .as_str()
            .or_else(|| value["summary"].as_str());
        if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
            summary.title = title.into();
        }
        if summary.title.is_empty() {
            let message = match provider {
                Provider::Claude => &value["message"],
                Provider::Codex if value["type"] == "response_item" => &value["payload"],
                _ => value,
            };
            let messages = value["messages"]
                .as_array()
                .map_or_else(|| vec![message], |rows| rows.iter().collect());
            for message in messages {
                if message["role"] == "user" || message["type"] == "user" {
                    let text = super::chat::content(&message["content"]);
                    // Injected context is not a useful conversation title.
                    if ![
                        "<environment_context>",
                        "<system-reminder>",
                        "# AGENTS.md instructions",
                    ]
                    .iter()
                    .any(|prefix| text.trim().starts_with(prefix))
                        && !text.trim().is_empty()
                    {
                        summary.title = text;
                        break;
                    }
                }
            }
        }
    }
    summary.title = display_text(&summary.title)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect();
    if records.is_empty() {
        summary.note = "No readable chat records. Enter shows details.".into();
    }
    summary
}

fn lines(bytes: &[u8]) -> Vec<Value> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::{setup, write};

    #[test]
    fn titles_use_user_text_and_recorded_project_paths() {
        let (_dir, locations) = setup();
        let gone = locations.home.join("gone-project");
        write(&locations.claude.join("projects/repo/chat.jsonl"), &serde_json::json!({"cwd": gone, "message": {"role": "user", "content": "Fix the login\nform"}}).to_string());
        write(
            &locations.codex.join("sessions/rollout-chat.jsonl"),
            &format!(
                "{}\n{}",
                serde_json::json!({"type":"session_meta", "payload":{"cwd":locations.home}}),
                serde_json::json!({"type":"response_item", "payload":{"role":"user","content":[{"type":"input_text","text":"Explain this app"}]}})
            ),
        );
        write(
            &locations.gemini.join("tmp/hash/chats/session.json"),
            r#"{"messages":[{"type":"user","content":"Review tests"}]}"#,
        );
        let mut listing = locations.list();
        assert!(listing.items.iter().all(|item| item.chat.is_none()));
        listing.summarize_chats(&locations);
        let summary = |provider| {
            listing
                .items
                .iter()
                .find(|item| item.provider == provider)
                .unwrap()
                .chat
                .as_ref()
                .unwrap()
        };
        assert_eq!(summary(Provider::Claude).title, "Fix the login form");
        assert!(summary(Provider::Claude).missing_project);
        assert!(!summary(Provider::Codex).missing_project);
        assert_eq!(summary(Provider::Codex).title, "Explain this app");
        assert_eq!(summary(Provider::Gemini).title, "Review tests");
        assert_eq!(summary(Provider::Gemini).project_label(), "Project unknown");
    }

    #[test]
    fn malformed_records_and_relative_paths_do_not_imply_missing_projects() {
        let summary = parse(
            Provider::Claude,
            b"bad\n{\"cwd\":\"relative\",\"customTitle\":\"Useful\\u001b title\"}\n{",
        );
        assert_eq!(summary.title, "Useful title");
        assert!(summary.project.is_none());
        assert!(!summary.missing_project);
    }
}
