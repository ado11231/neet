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

#[test]
fn gemini_jsonl_sessions_support_discovery_summary_preview_and_removal_review() {
    let (_dir, locations) = setup();
    let contents = concat!(
        "{\"sessionId\":\"example\",\"projectHash\":\"project\"}\n",
        "{\"type\":\"user\",\"content\":[\"Review the checkout\"]}\n",
        "{\"type\":\"gemini\",\"content\":[{\"text\":\"Found the problem\"}]}\n",
        "{\"$rewindTo\":\"example-message\"}\n"
    );
    write(
        &locations
            .gemini
            .join("tmp/project/chats/session-example.jsonl"),
        contents,
    );
    let mut listing = locations.list();
    listing.summarize_chats(&locations);
    assert_eq!(listing.items.len(), 1);
    let item = &listing.items[0];
    assert_eq!(item.chat.as_ref().unwrap().title, "Review the checkout");
    let preview = locations.preview(item).unwrap();
    assert!(preview.contains("You\nReview the checkout"));
    assert!(preview.contains("Assistant\nFound the problem"));
    assert!(preview.contains("rewinds are not applied"));
    assert!(remove::Removal::prepare(&locations, item).is_ok());
}

fn skill_source(listing: &Listing, folder: &str) -> Option<Source> {
    listing
        .items
        .iter()
        .find(|item| {
            item.kind == Kind::Skill && item.path.parent().is_some_and(|p| p.ends_with(folder))
        })
        .and_then(|item| item.source.clone())
}

#[test]
fn marks_copies_links_bundled_and_plugin_skills() {
    let (_dir, locations) = setup();
    let home = locations.home.clone();
    write(&home.join(".claude/skills/mine/SKILL.md"), SKILL);
    write(&home.join("code/linked/SKILL.md"), SKILL);
    fs::create_dir_all(home.join(".agents/skills")).unwrap();
    symlink(home.join("code/linked"), home.join(".agents/skills/linked")).unwrap();
    write(&home.join(".codex/skills/.system/helper/SKILL.md"), SKILL);
    write(&home.join(".codex/skills/.system/.marker"), "");
    let install = home.join(".claude/plugins/cache/market/tools/1.2.0");
    write(&install.join("skills/deploy/SKILL.md"), SKILL);
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            r#"{{"version":2,"plugins":{{"tools@market":[{{"scope":"user","installPath":"{}","version":"1.2.0"}}]}}}}"#,
            install.display()
        ),
    );
    write(
        &home.join(".claude/settings.json"),
        r#"{"enabledPlugins":{"tools@market":false},"apiKey":"secret"}"#,
    );
    write(
        &home.join(".codex/plugins/cache/curated/pets/0.1.6/skills/pets/SKILL.md"),
        SKILL,
    );
    write(
        &home.join(".gemini/extensions/notes/skills/notes/SKILL.md"),
        SKILL,
    );
    write(
        &home.join(".gemini/extensions/notes/gemini-extension.json"),
        r#"{"name":"notes","version":"2.0.1"}"#,
    );

    let listing = locations.list();

    assert_eq!(skill_source(&listing, "mine"), Some(Source::Copy));
    assert_eq!(
        skill_source(&listing, "linked"),
        Some(Source::Link(
            home.join(".agents/skills").join(home.join("code/linked"))
        ))
    );
    assert_eq!(skill_source(&listing, "helper"), Some(Source::Bundled));
    assert_eq!(
        skill_source(&listing, "deploy"),
        Some(Source::Plugin(Plugin {
            name: "tools".into(),
            marketplace: "market".into(),
            version: "1.2.0".into(),
            enabled: Some(false),
        }))
    );
    assert_eq!(
        skill_source(&listing, "pets"),
        Some(Source::Plugin(Plugin {
            name: "pets".into(),
            marketplace: "curated".into(),
            version: "0.1.6".into(),
            enabled: None,
        }))
    );
    let Some(Source::Plugin(notes)) = skill_source(&listing, "notes") else {
        panic!("the Gemini extension skill should be listed");
    };
    assert_eq!(notes.version, "2.0.1");
    let skills: Vec<_> = listing
        .items
        .iter()
        .filter(|item| item.kind == Kind::Skill && item.provider == Provider::Claude)
        .collect();
    assert!(
        matches!(
            skills.last().and_then(|item| item.source.as_ref()),
            Some(Source::Plugin(_))
        ),
        "plugin skills come after your own"
    );
    // Plugin and bundled skills are view only.
    for item in listing.items.iter().filter(|item| item.kind == Kind::Skill) {
        let view_only = matches!(item.source, Some(Source::Bundled | Source::Plugin(_)));
        assert_eq!(item.bundled, view_only, "{}", item.path.display());
    }
    assert!(listing.warnings.is_empty(), "{:?}", listing.warnings);
}

#[test]
fn plugin_install_paths_outside_home_are_skipped() {
    let (_dir, locations) = setup();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("skills/x/SKILL.md"), SKILL);
    write(
        &locations
            .home
            .join(".claude/plugins/installed_plugins.json"),
        &format!(
            r#"{{"plugins":{{"x@m":[{{"installPath":"{}","version":"1"}}]}}}}"#,
            outside.path().display()
        ),
    );

    let listing = locations.list();

    assert!(listing.items.iter().all(|item| item.kind != Kind::Skill));
    assert!(
        listing
            .warnings
            .iter()
            .any(|warning| warning.contains("outside"))
    );
}

#[test]
fn removing_a_link_keeps_its_source_and_can_be_undone_for_each_tool() {
    let (_dir, locations) = setup();
    let home = locations.home.clone();
    for root in [
        ".claude/skills",
        ".agents/skills",
        ".codex/skills",
        ".gemini/skills",
    ] {
        let name = format!(
            "linked-{}",
            root.split('/').next().unwrap().trim_start_matches('.')
        );
        let source = home.join("code").join(&name);
        write(&source.join("SKILL.md"), SKILL);
        fs::create_dir_all(home.join(root)).unwrap();
        let link = home.join(root).join(&name);
        symlink(&source, &link).unwrap();
        let listed = |locations: &Locations| {
            locations
                .list()
                .items
                .into_iter()
                .find(|item| item.path == link.join("SKILL.md"))
        };
        let item = listed(&locations).expect("the linked skill should be listed");

        let unlink = skills::Unlink::prepare(&locations, &item).unwrap();
        assert_eq!(unlink.source(), source);
        let relink = unlink.apply().unwrap();

        assert!(
            fs::symlink_metadata(&link).is_err(),
            "{root}: the link goes"
        );
        assert!(
            source.join("SKILL.md").is_file(),
            "{root}: the source stays"
        );
        assert!(
            listed(&locations).is_none(),
            "{root}: the tool no longer sees it"
        );

        relink.apply().unwrap();
        assert!(listed(&locations).is_some(), "{root}: undo links it again");
        assert!(relink.apply().is_err(), "{root}: undo never replaces");
    }
}

#[test]
fn removing_a_link_refuses_changes_copies_plugins_and_project_links() {
    let (_dir, mut locations) = setup();
    let home = locations.home.clone();
    write(&home.join("code/first/SKILL.md"), SKILL);
    write(&home.join("code/second/SKILL.md"), SKILL);
    fs::create_dir_all(home.join(".claude/skills")).unwrap();
    let link = home.join(".claude/skills/first");
    symlink(home.join("code/first"), &link).unwrap();
    write(&home.join(".claude/skills/copy/SKILL.md"), SKILL);
    let project = home.join("work/repo");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::create_dir_all(project.join(".claude/skills")).unwrap();
    symlink(
        home.join("code/second"),
        project.join(".claude/skills/second"),
    )
    .unwrap();
    locations.project = Some(project.clone());
    let listing = locations.list();
    let find = |path: &Path| {
        listing
            .items
            .iter()
            .find(|item| item.path == path.join("SKILL.md"))
            .unwrap()
            .clone()
    };

    let unlink = skills::Unlink::prepare(&locations, &find(&link)).unwrap();
    // The source changes after review: the link now points somewhere else.
    fs::remove_file(&link).unwrap();
    symlink(home.join("code/second"), &link).unwrap();
    assert!(unlink.apply().is_err());
    assert_eq!(fs::read_link(&link).unwrap(), home.join("code/second"));

    assert!(skills::Unlink::prepare(&locations, &find(&home.join(".claude/skills/copy"))).is_err());
    assert!(
        skills::Unlink::prepare(&locations, &find(&project.join(".claude/skills/second"))).is_err()
    );
    let mut plugin = find(&link);
    plugin.source = Some(Source::Plugin(Plugin {
        name: "p".into(),
        marketplace: String::new(),
        version: String::new(),
        enabled: None,
    }));
    assert!(skills::Unlink::prepare(&locations, &plugin).is_err());
}
