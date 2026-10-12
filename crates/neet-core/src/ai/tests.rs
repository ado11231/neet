use super::*;
use std::os::unix::fs::symlink;

const SKILL: &str =
    "---\nname: sample\ndescription: A sample skill.\n---\nRead the project docs.\n";

pub(super) fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

pub(super) fn setup() -> (tempfile::TempDir, Locations) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    let locations = Locations::new(home, None);
    (dir, locations)
}

#[test]
fn discovers_only_known_files_without_reading_chat_contents() {
    let (_dir, locations) = setup();
    for (path, text) in [
        (".claude/settings.json", "{}"),
        (".codex/AGENTS.md", "Instructions"),
        (".gemini/skills/sample/SKILL.md", SKILL),
        (".agents/skills/shared/SKILL.md", SKILL),
        (".claude/projects/project/chat.jsonl", "not valid json"),
        (".codex/sessions/2026/10/12/rollout-chat.jsonl", ""),
        (".gemini/tmp/project/chats/session-chat.json", "{}"),
        (".claude/.credentials.json", "secret"),
        (".codex/auth.json", "secret"),
        (".codex/state.sqlite", "secret"),
        (".gemini/oauth_creds.json", "secret"),
        (".gemini/tmp/project/other.json", "secret"),
    ] {
        write(&locations.home.join(path), text);
    }
    let listing = locations.list();
    assert_eq!(listing.items.len(), 7);
    assert_eq!(
        listing
            .items
            .iter()
            .filter(|item| item.kind == Kind::Chat)
            .count(),
        3
    );
    assert!(listing.warnings.is_empty(), "{:?}", listing.warnings);
}

#[test]
fn project_discovery_stops_at_git_root_and_honors_custom_roots() {
    let (_dir, mut locations) = setup();
    let project = locations.home.join("work/repo/sub");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(locations.home.join("work/repo/.git")).unwrap();
    write(&locations.home.join("work/AGENTS.md"), "not in this repo");
    write(&locations.home.join("work/repo/AGENTS.md"), "repo");
    write(&project.join(".claude/skills/sample/SKILL.md"), SKILL);
    locations.project = Some(project);
    locations.codex = locations.home.join("custom-codex");
    write(&locations.codex.join("config.toml"), "model = 'sample'");
    assert_eq!(locations.list().items.len(), 3);
}

#[test]
fn rejects_escaped_roots_file_links_and_retargeted_skills() {
    let (_dir, locations) = setup();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("sample/SKILL.md"), SKILL);
    fs::create_dir_all(locations.home.join(".agents/skills")).unwrap();
    symlink(
        outside.path().join("sample"),
        locations.home.join(".agents/skills/sample"),
    )
    .unwrap();
    write(&locations.home.join(".codex/auth.json"), "secret");
    symlink(
        locations.home.join(".codex/auth.json"),
        locations.home.join(".codex/config.toml"),
    )
    .unwrap();
    let listing = locations.list();
    assert!(listing.items.is_empty());
    assert_eq!(listing.warnings.len(), 2);
}

#[test]
fn previews_conversations_and_skips_tools_and_bad_records() {
    for (provider, contents) in [
        (
            Provider::Claude,
            "{\"message\":{\"role\":\"user\",\"content\":\"Hello\"}}\n{\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Hi\"},{\"type\":\"tool_use\",\"text\":\"secret tool\"}]}}\nbad",
        ),
        (
            Provider::Codex,
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"Hello\"}]}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hi\"}]}}",
        ),
        (
            Provider::Gemini,
            "{\"messages\":[{\"type\":\"user\",\"content\":\"Hello\"},{\"type\":\"gemini\",\"content\":\"Hi\"}]}",
        ),
    ] {
        let result = chat::render(provider, contents.as_bytes());
        assert!(result.contains("You\nHello"), "{result}");
        assert!(result.contains("Assistant\nHi"), "{result}");
        assert!(!result.contains("secret tool"));
    }
}

#[test]
fn limits_preview_and_removes_terminal_controls() {
    let (_dir, locations) = setup();
    write(&locations.codex.join("AGENTS.md"), &"x".repeat(600_000));
    let item = locations.list().items.remove(0);
    let preview = locations.preview(&item).unwrap();
    assert!(preview.contains("Preview limited"));
    assert!(preview.len() < 530_000);
    assert_eq!(
        display_text("hi\u{1b}[2J\r\nnext\tline"),
        "hi[2J\nnext    line"
    );
}

#[test]
fn install_links_whole_skill_and_never_overwrites() {
    let (_dir, locations) = setup();
    let source = locations.home.join("my-skills/sample");
    write(&source.join("SKILL.md"), SKILL);
    write(&source.join("references/help.md"), "help");
    let install = skills::Install::prepare(&locations, &source, Provider::Codex).unwrap();
    assert!(!install.destination().exists());
    install.apply().unwrap();
    assert_eq!(
        fs::read_to_string(install.destination().join("references/help.md")).unwrap(),
        "help"
    );
    assert!(install.apply().is_err());
    assert_eq!(locations.list().items.len(), 1);
}

#[test]
fn install_rejects_changes_and_a_destination_link() {
    let (_dir, locations) = setup();
    let source = locations.home.join("sample");
    write(&source.join("SKILL.md"), SKILL);
    let install = skills::Install::prepare(&locations, &source, Provider::Claude).unwrap();
    write(&source.join("SKILL.md"), &SKILL.replace("docs", "tests"));
    assert!(install.apply().is_err());
    assert!(!install.destination().exists());
    fs::create_dir_all(&locations.claude).unwrap();
    symlink(&source, locations.claude.join("skills")).unwrap();
    assert!(skills::Install::prepare(&locations, &source, Provider::Claude).is_err());
}

#[test]
fn edit_backs_up_and_restore_is_also_reversible() {
    let (_dir, locations) = setup();
    let path = locations.claude.join("skills/sample/SKILL.md");
    write(&path, SKILL);
    let item = locations.list().items.remove(0);
    let edit = skills::Edit::begin(&locations, &item).unwrap();
    let updated = SKILL.replace("docs", "tests");
    fs::write(edit.draft(), &updated).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), SKILL);
    let backup = edit.save(&edit.edited().unwrap()).unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), SKILL);
    assert_eq!(fs::read_to_string(&path).unwrap(), updated);
    let restore = skills::Edit::begin(&locations, &item).unwrap();
    restore.save(&restore.latest_backup().unwrap()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), SKILL);
    assert_eq!(
        skills::Edit::begin(&locations, &item)
            .unwrap()
            .latest_backup()
            .unwrap(),
        updated.as_bytes()
    );
}

#[test]
fn edit_refuses_stale_files_links_and_bundled_skills() {
    let (_dir, locations) = setup();
    let source = locations.home.join("sample");
    write(&source.join("SKILL.md"), SKILL);
    let install = skills::Install::prepare(&locations, &source, Provider::Codex).unwrap();
    install.apply().unwrap();
    let item = locations.list().items.remove(0);
    let edit = skills::Edit::begin(&locations, &item).unwrap();
    fs::write(source.join("SKILL.md"), SKILL.replace("docs", "tests")).unwrap();
    assert!(edit.save(SKILL.as_bytes()).is_err());
    let edit = skills::Edit::begin(&locations, &item).unwrap();
    let other = locations.home.join("other");
    write(&other.join("SKILL.md"), SKILL);
    fs::remove_file(install.destination()).unwrap();
    symlink(&other, install.destination()).unwrap();
    assert!(edit.save(SKILL.as_bytes()).is_err());
    write(
        &locations.codex.join("skills/.system/builtin/SKILL.md"),
        SKILL,
    );
    let builtin = locations
        .list()
        .items
        .into_iter()
        .find(|item| item.bundled)
        .unwrap();
    assert!(skills::Edit::begin(&locations, &builtin).is_err());
}

#[test]
fn rejects_invalid_skill_and_backup_directory_escape() {
    let (_dir, locations) = setup();
    assert!(skills::validate(b"no frontmatter").is_err());
    assert!(skills::validate(b"---\nname: sample\ndescription: ''\n---\nBody").is_err());
    write(&locations.claude.join("skills/sample/SKILL.md"), SKILL);
    let item = locations.list().items.remove(0);
    let edit = skills::Edit::begin(&locations, &item).unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), locations.home.join(".local")).unwrap();
    assert!(
        edit.save(SKILL.replace("docs", "tests").as_bytes())
            .is_err()
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn invalid_installed_skills_can_be_repaired_but_not_loaded() {
    let (_dir, locations) = setup();
    let folder = locations.claude.join("skills/broken");
    write(&folder.join("SKILL.md"), "Missing frontmatter\n");
    assert!(skills::Install::prepare(&locations, &folder, Provider::Codex).is_err());
    let item = locations.list().items.remove(0);
    let edit = skills::Edit::begin(&locations, &item).unwrap();
    let backup = edit.save(SKILL.as_bytes()).unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), "Missing frontmatter\n");
    assert_eq!(fs::read_to_string(folder.join("SKILL.md")).unwrap(), SKILL);
}

#[test]
fn discovery_keeps_entries_found_at_the_limit_and_ignores_loose_skill_files() {
    let (_dir, locations) = setup();
    write(
        &locations.claude.join("skills/readme.txt"),
        "not a skill folder",
    );
    let root = locations.codex.join("archived_sessions");
    write(&root.join("rollout-a.jsonl"), "");
    write(&root.join("rollout-b.jsonl"), "");
    let mut scanner = Scanner {
        locations: &locations,
        listing: Listing::default(),
        visited: MAX_ENTRIES - 2,
    };
    scanner.walk_chats(&root, Provider::Codex, 1);
    assert_eq!(scanner.listing.items.len(), 2);
    assert_eq!(locations.list().warnings, Vec::<String>::new());
}
