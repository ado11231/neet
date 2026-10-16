//! Chat actions through each tool's own commands: continue a chat, and
//! archive or unarchive a Codex chat. Also what a chat leaves beside its
//! transcript: Claude Code's related folders, and the history and index
//! entries that name it.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::{Item, Kind, Locations, Provider, checked_path};
use crate::rewrite::{self, Backups, Opened};

/// How long `codex archive` may take
const ARCHIVE_TIMEOUT: Duration = Duration::from_secs(60);

fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

/// Whether `text` is a session ID: a UUID, so it can never be read as an
/// option or a name by the tool
fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(index, c)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// The session ID a chat's tool knows it by. Claude Code names the
/// transcript after it; Codex ends its `rollout-` file name with it.
#[must_use]
pub fn session_id(item: &Item) -> Option<String> {
    if item.kind != Kind::Chat {
        return None;
    }
    let stem = item.path.file_stem()?.to_str()?;
    let id = match item.provider {
        Provider::Claude => stem,
        Provider::Codex => stem.get(stem.len().checked_sub(36)?..)?,
        Provider::Gemini | Provider::Shared => return None,
    };
    is_uuid(id).then(|| id.to_owned())
}

/// A tool's command to continue a chat, and the folder to run it in
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resume {
    pub program: &'static str,
    pub args: Vec<String>,
    pub folder: PathBuf,
}

/// How to continue `item` in its own tool.
///
/// # Errors
/// Returns an error for Gemini CLI, which resumes chats by a list number
/// that can change, and when Claude Code's project folder is gone.
pub fn resume(locations: &Locations, item: &Item) -> io::Result<Resume> {
    let project = item
        .chat
        .as_ref()
        .and_then(|chat| chat.project.clone())
        .filter(|project| project.is_dir());
    let id = || session_id(item).ok_or_else(|| invalid("this chat's session ID could not be read"));
    match item.provider {
        Provider::Claude => Ok(Resume {
            program: "claude",
            args: vec!["--resume".into(), id()?],
            // Claude Code finds a chat only from its own project folder.
            folder: project.ok_or_else(|| {
                invalid("Claude Code resumes a chat from its project folder, which is missing")
            })?,
        }),
        Provider::Codex => Ok(Resume {
            program: "codex",
            args: vec!["resume".into(), id()?],
            folder: project.unwrap_or_else(|| locations.home.clone()),
        }),
        Provider::Gemini | Provider::Shared => Err(invalid(
            "Gemini CLI resumes chats by a list number that can change. Open the project and run gemini --resume",
        )),
    }
}

/// Whether a Codex chat is in its archive
#[must_use]
pub fn is_archived(locations: &Locations, item: &Item) -> bool {
    item.provider == Provider::Codex
        && item
            .path
            .starts_with(locations.codex.join("archived_sessions"))
}

/// Archives a Codex chat, or brings an archived one back, with Codex's own
/// command, which moves its file and updates its index. Returns whether the
/// chat is archived now.
///
/// # Errors
/// Returns an error for other tools, or if Codex failed or took too long.
pub fn archive(locations: &Locations, item: &Item) -> io::Result<bool> {
    archive_with(locations, item, "codex")
}

fn archive_with(locations: &Locations, item: &Item, program: &str) -> io::Result<bool> {
    if item.provider != Provider::Codex {
        return Err(invalid("only Codex can archive its chats"));
    }
    let id = session_id(item).ok_or_else(|| invalid("this chat's session ID could not be read"))?;
    let archived = is_archived(locations, item);
    let action = if archived { "unarchive" } else { "archive" };
    let captured = crate::run::capture(
        Command::new(program)
            .arg(action)
            .arg(&id)
            .current_dir(&locations.home),
        ARCHIVE_TIMEOUT,
    )?
    .ok_or_else(|| invalid("Codex took too long. Check its chat list before trying again"))?;
    if !captured.success {
        let message = [captured.stderr.trim(), captured.stdout.trim()]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or("Codex stopped with an error");
        return Err(invalid(message.lines().last().unwrap_or(message)));
    }
    Ok(!archived)
}

/// Folders and files Claude Code keeps for one chat beside its transcript:
/// its subagent folder, file history, saved environment, and to-do lists.
/// Links are left out. Other tools keep none that neet knows of.
#[must_use]
pub fn related(locations: &Locations, item: &Item) -> Vec<PathBuf> {
    let Some(id) = session_id(item).filter(|_| item.provider == Provider::Claude) else {
        return Vec::new();
    };
    let claude = &locations.claude;
    let mut paths = vec![
        item.path.with_extension(""),
        claude.join("file-history").join(&id),
        claude.join("session-env").join(&id),
    ];
    if let Ok(todos) = fs::read_dir(claude.join("todos")) {
        let mut found: Vec<PathBuf> = todos
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&id)
                            && Path::new(name)
                                .extension()
                                .is_some_and(|ending| ending.eq_ignore_ascii_case("json"))
                    })
            })
            .collect();
        found.sort();
        paths.extend(found);
    }
    paths
        .into_iter()
        .filter(|path| {
            fs::symlink_metadata(path).is_ok_and(|meta| !meta.file_type().is_symlink())
                && checked_path(&locations.home, path).is_ok()
        })
        .collect()
}

/// One history or index file, with the lines for removed chats taken out
pub struct History {
    opened: Opened,
    name: &'static str,
    contents: Vec<u8>,
    /// How many lines go
    pub removed: usize,
}

/// What a history cleanup did
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cleaned {
    pub file: PathBuf,
    pub removed: usize,
    /// The file as it was, to restore it
    pub backup: PathBuf,
}

/// Each history and index file that names a chat, its key, and the name of
/// its backup
fn history_files(
    locations: &Locations,
    provider: Provider,
) -> Vec<(PathBuf, &'static str, &'static str)> {
    match provider {
        Provider::Claude => vec![(
            locations.claude.join("history.jsonl"),
            "sessionId",
            "claude-history.jsonl",
        )],
        Provider::Codex => vec![
            (
                locations.codex.join("history.jsonl"),
                "session_id",
                "codex-history.jsonl",
            ),
            (
                locations.codex.join("session_index.jsonl"),
                "id",
                "codex-session-index.jsonl",
            ),
        ],
        Provider::Gemini | Provider::Shared => Vec::new(),
    }
}

/// `contents` without the lines whose `key` is one of `ids`, and how many
/// went. Lines neet cannot read are kept.
fn without(contents: &[u8], key: &str, ids: &BTreeSet<String>) -> (Vec<u8>, usize) {
    let mut kept = Vec::with_capacity(contents.len());
    let mut removed = 0;
    for line in contents.split_inclusive(|&byte| byte == b'\n') {
        let named = serde_json::from_slice::<Value>(line.trim_ascii())
            .ok()
            .and_then(|value| value.get(key)?.as_str().map(str::to_owned))
            .is_some_and(|id| ids.contains(&id));
        if named {
            removed += 1;
        } else {
            kept.extend_from_slice(line);
        }
    }
    (kept, removed)
}

impl History {
    /// The history and index files of `provider` that name any chat in
    /// `ids`, ready to write. Files that name none are left out. A file
    /// that cannot be read, is a link, or is larger than 1 MiB gives an
    /// error in its place.
    #[must_use]
    pub fn prepare(
        locations: &Locations,
        provider: Provider,
        ids: &BTreeSet<String>,
    ) -> Vec<io::Result<Self>> {
        if ids.is_empty() {
            return Vec::new();
        }
        let mut found = Vec::new();
        for (path, key, name) in history_files(locations, provider) {
            if fs::symlink_metadata(&path).is_err() {
                continue;
            }
            let prepared = (|| {
                if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                    return Err(invalid("history files are not followed through links"));
                }
                checked_path(&locations.home, &path)?;
                let opened = Opened::open(&path).map_err(|error| {
                    invalid(&format!("{}: {error}; its entries stay", path.display()))
                })?;
                let (contents, removed) = without(opened.contents(), key, ids);
                Ok((removed > 0).then_some(Self {
                    opened,
                    name,
                    contents,
                    removed,
                }))
            })();
            match prepared {
                Ok(Some(history)) => found.push(Ok(history)),
                Ok(None) => {}
                Err(error) => found.push(Err(error)),
            }
        }
        found
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.opened.path()
    }

    /// Writes the file without the removed lines, after saving a backup.
    ///
    /// # Errors
    /// Returns an error if the file changed since it was read, or the backup
    /// or write failed. The file is then as it was.
    pub fn apply(&self, locations: &Locations) -> io::Result<Cleaned> {
        let backups = Backups::at(locations.home.join(".local/state/neet/backups/ai/history"));
        let backup = rewrite::write(
            &self.opened,
            &self.contents,
            &backups,
            self.name,
            SystemTime::now(),
        )
        .map_err(|error| invalid(&error.to_string()))?;
        Ok(Cleaned {
            file: self.opened.path().to_path_buf(),
            removed: self.removed,
            backup,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{setup, write};
    use super::*;

    const ID: &str = "0df8fdc8-b9e2-48ed-bc04-d2dc4b6e8fd0";
    const OTHER: &str = "03439f05-b175-476b-a588-cd32ef866c54";

    fn chat(locations: &Locations, provider: Provider, path: &str) -> Item {
        let path = locations.home.join(path);
        write(&path, "{}\n");
        Item {
            provider,
            kind: Kind::Chat,
            path,
            scope: "History".into(),
            size: 3,
            modified: None,
            bundled: false,
            chat: Some(super::super::summary::Summary {
                project: Some(locations.home.clone()),
                ..Default::default()
            }),
            source: None,
        }
    }

    #[test]
    fn reads_session_ids_from_file_names() {
        let (_dir, locations) = setup();
        let claude = chat(
            &locations,
            Provider::Claude,
            &format!(".claude/projects/p/{ID}.jsonl"),
        );
        let codex = chat(
            &locations,
            Provider::Codex,
            &format!(".codex/sessions/2026/10/01/rollout-2026-10-01T16-30-26-{ID}.jsonl"),
        );
        let gemini = chat(
            &locations,
            Provider::Gemini,
            ".gemini/tmp/h/chats/session-1.json",
        );
        let odd = chat(
            &locations,
            Provider::Claude,
            ".claude/projects/p/--help.jsonl",
        );

        assert_eq!(session_id(&claude).as_deref(), Some(ID));
        assert_eq!(session_id(&codex).as_deref(), Some(ID));
        assert_eq!(session_id(&gemini), None);
        assert_eq!(session_id(&odd), None);
    }

    #[test]
    fn resume_uses_each_tools_own_command() {
        let (_dir, locations) = setup();
        let claude = chat(
            &locations,
            Provider::Claude,
            &format!(".claude/projects/p/{ID}.jsonl"),
        );
        let codex = chat(
            &locations,
            Provider::Codex,
            &format!(".codex/sessions/2026/10/01/rollout-x-{ID}.jsonl"),
        );
        let gemini = chat(
            &locations,
            Provider::Gemini,
            ".gemini/tmp/h/chats/session-1.json",
        );

        let resume_claude = resume(&locations, &claude).unwrap();
        assert_eq!(resume_claude.program, "claude");
        assert_eq!(resume_claude.args, ["--resume", ID]);
        assert_eq!(resume_claude.folder, locations.home);
        assert_eq!(resume(&locations, &codex).unwrap().args, ["resume", ID]);
        assert!(resume(&locations, &gemini).is_err());

        let mut gone = claude.clone();
        gone.chat.as_mut().unwrap().project = Some(locations.home.join("deleted"));
        assert!(resume(&locations, &gone).is_err());
    }

    #[test]
    fn archive_runs_codex_and_reports_the_new_state() {
        let (_dir, locations) = setup();
        let active = chat(
            &locations,
            Provider::Codex,
            &format!(".codex/sessions/2026/10/01/rollout-x-{ID}.jsonl"),
        );
        let archived = chat(
            &locations,
            Provider::Codex,
            &format!(".codex/archived_sessions/rollout-x-{OTHER}.jsonl"),
        );
        // A stand in for codex that records what it was asked
        let log = locations.home.join("calls");
        let fake = locations.home.join("fake-codex");
        write(
            &fake,
            &format!("#!/bin/sh\necho \"$@\" >> {}\n", log.display()),
        );
        fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let program = fake.to_str().unwrap();

        assert!(archive_with(&locations, &active, program).unwrap());
        assert!(!archive_with(&locations, &archived, program).unwrap());
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!("archive {ID}\nunarchive {OTHER}\n")
        );

        let failing = locations.home.join("failing-codex");
        write(&failing, "#!/bin/sh\necho 'no such session' >&2\nexit 1\n");
        fs::set_permissions(
            &failing,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let error = archive_with(&locations, &active, failing.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("no such session"));

        let claude = chat(
            &locations,
            Provider::Claude,
            &format!(".claude/projects/p/{ID}.jsonl"),
        );
        assert!(archive_with(&locations, &claude, program).is_err());
    }

    #[test]
    fn finds_claude_folders_for_one_chat_only() {
        let (_dir, locations) = setup();
        let item = chat(
            &locations,
            Provider::Claude,
            &format!(".claude/projects/p/{ID}.jsonl"),
        );
        let claude = &locations.claude;
        for path in [
            format!("projects/p/{ID}/subagents/a.jsonl"),
            format!("file-history/{ID}/v1"),
            format!("session-env/{ID}/env"),
            format!("todos/{ID}-agent-{ID}.json"),
            format!("file-history/{OTHER}/v1"),
            format!("todos/{OTHER}-agent.json"),
        ] {
            write(&claude.join(path), "x");
        }

        let found = related(&locations, &item);

        assert_eq!(
            found,
            vec![
                claude.join(format!("projects/p/{ID}")),
                claude.join(format!("file-history/{ID}")),
                claude.join(format!("session-env/{ID}")),
                claude.join(format!("todos/{ID}-agent-{ID}.json")),
            ]
        );
    }

    #[test]
    fn history_cleanup_keeps_other_chats_and_can_be_restored() {
        let (_dir, locations) = setup();
        let history = locations.claude.join("history.jsonl");
        let before = format!(
            "{{\"display\":\"one\",\"sessionId\":\"{ID}\"}}\n{{\"display\":\"two\",\"sessionId\":\"{OTHER}\"}}\nnot json\n{{\"display\":\"three\",\"sessionId\":\"{ID}\"}}\n"
        );
        write(&history, &before);
        write(
            &locations.codex.join("session_index.jsonl"),
            &format!("{{\"id\":\"{ID}\",\"thread_name\":\"a\"}}\n{{\"id\":\"{OTHER}\"}}\n"),
        );
        write(
            &locations.codex.join("history.jsonl"),
            &format!("{{\"session_id\":\"{OTHER}\",\"text\":\"keep\"}}\n"),
        );
        let ids = BTreeSet::from([ID.to_string()]);

        let claude = History::prepare(&locations, Provider::Claude, &ids);
        assert_eq!(claude.len(), 1);
        let edit = claude.into_iter().next().unwrap().unwrap();
        assert_eq!(edit.removed, 2);
        let cleaned = edit.apply(&locations).unwrap();
        assert_eq!(
            fs::read_to_string(&history).unwrap(),
            format!("{{\"display\":\"two\",\"sessionId\":\"{OTHER}\"}}\nnot json\n")
        );
        assert_eq!(fs::read_to_string(&cleaned.backup).unwrap(), before);

        // Only the index names the chat; Codex's history does not.
        let codex: Vec<_> = History::prepare(&locations, Provider::Codex, &ids)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(codex.len(), 1);
        assert!(codex[0].path().ends_with("session_index.jsonl"));

        // A file changed after it was read is left alone.
        let stale = History::prepare(&locations, Provider::Codex, &ids)
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        write(&locations.codex.join("session_index.jsonl"), "{}\n");
        assert!(stale.apply(&locations).is_err());
        assert_eq!(
            fs::read_to_string(locations.codex.join("session_index.jsonl")).unwrap(),
            "{}\n"
        );

        // The backup puts the history back.
        fs::write(&history, fs::read(&cleaned.backup).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(&history).unwrap(), before);
    }
}
