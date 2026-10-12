use super::*;
use ratatui::crossterm::event::KeyModifiers;
use std::fs;
use std::path::Path;

const SKILL: &str = "---\nname: sample\ndescription: A sample.\n---\nRead docs.\n";

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn setup() -> (tempfile::TempDir, AiTools) {
    let dir = tempfile::tempdir().unwrap();
    let locations = Locations::new(fs::canonicalize(dir.path()).unwrap(), None);
    write(&locations.claude.join("skills/sample/SKILL.md"), SKILL);
    write(&locations.codex.join("config.toml"), "model = 'sample'");
    write(
        &locations.claude.join("projects/repo/chat.jsonl"),
        "{\"message\":{\"role\":\"user\",\"content\":\"A private chat\"}}",
    );
    let listing = locations.list();
    let mut screen = AiTools::at(locations);
    screen.listing = listing;
    (dir, screen)
}

fn press(screen: &mut AiTools, code: KeyCode) -> Action {
    let scan = super::super::scan::ScanStatus::Failed("test".into());
    let context = Context {
        scan: &scan,
        disk: None,
        cleanable: None,
        plan: None,
    };
    screen.handle_key(KeyEvent::new(code, KeyModifiers::NONE), &context)
}

fn render(screen: &mut AiTools, width: u16, height: u16) -> String {
    let scan = super::super::scan::ScanStatus::Failed("test".into());
    let context = Context {
        scan: &scan,
        disk: None,
        cleanable: None,
        plan: None,
    };
    let buffer = visual::tests::render("ai", screen, &context, width, height);
    visual::tests::text(&buffer)
}

#[test]
fn tabs_filter_and_chat_preview_work_at_all_sizes() {
    let (_dir, mut screen) = setup();
    for (width, height) in visual::tests::SIZES {
        let text = render(&mut screen, width, height);
        assert!(text.contains("AI Tools"));
        assert!(text.contains("config.toml"));
        assert!(text.contains("▸ config.toml"), "{text}");
        assert!(!text.contains("A private chat"));
    }
    press(&mut screen, KeyCode::Tab);
    finish_refresh(&mut screen);
    assert_eq!(screen.selected().unwrap().kind, Kind::Chat);
    assert!(render(&mut screen, 100, 30).contains("A private chat"));
    press(&mut screen, KeyCode::Enter);
    assert!(render(&mut screen, 100, 30).contains("A private chat"));
    screen.back();
    press(&mut screen, KeyCode::Tab);
    press(&mut screen, KeyCode::Char('/'));
    for c in "missing?q".chars() {
        press(&mut screen, KeyCode::Char(c));
    }
    assert!(screen.takes_text());
    assert!(screen.rows().is_empty());
    screen.back();
    assert_eq!(screen.rows().len(), 1);
    screen.filter = "~/.claude/skills".into();
    assert_eq!(screen.rows().len(), 1);
}

#[test]
fn editing_reviews_before_writing_and_escape_cancels() {
    let (_dir, mut screen) = setup();
    screen.tab = 2;
    let original = screen.selected().unwrap().path;
    let Action::Edit(copy) = press(&mut screen, KeyCode::Char('e')) else {
        panic!("edit expected");
    };
    fs::write(&copy, SKILL.replace("docs", "tests")).unwrap();
    screen.edited(Ok(()));
    assert!(screen.is_dialog());
    assert!(render(&mut screen, 100, 30).contains("+ Read tests."));
    assert_eq!(fs::read_to_string(&original).unwrap(), SKILL);
    screen.back();
    assert!(!copy.exists());
    assert!(!screen.is_dialog());
    assert_eq!(fs::read_to_string(original).unwrap(), SKILL);
}

#[test]
fn loading_requires_review_and_keeps_other_skill_files() {
    let (_dir, mut screen) = setup();
    let source = screen.locations.home.join("local/new-skill");
    write(&source.join("SKILL.md"), SKILL);
    write(&source.join("references/readme.md"), "Reference");
    press(&mut screen, KeyCode::Char('i'));
    press(&mut screen, KeyCode::Tab);
    for c in source.to_str().unwrap().chars() {
        press(&mut screen, KeyCode::Char(c));
    }
    press(&mut screen, KeyCode::Enter);
    let target = screen.locations.home.join(".agents/skills/new-skill");
    assert!(screen.is_dialog());
    assert!(!target.exists());
    press(&mut screen, KeyCode::Char('y'));
    assert!(target.join("references/readme.md").exists());
    assert!(screen.note.contains("Skill linked"));
    finish_refresh(&mut screen);
    assert_eq!(screen.tab, 2);
    assert_eq!(screen.provider, Some(Provider::Codex));
    assert_eq!(screen.selected().unwrap().path, target.join("SKILL.md"));
}

fn finish_refresh(screen: &mut AiTools) {
    let listing = screen
        .pending
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    sender.send(listing).unwrap();
    screen.pending = Some(receiver);
    screen.poll();
}

#[test]
fn shared_skills_appear_for_codex_and_gemini() {
    let (_dir, mut screen) = setup();
    let shared = screen.locations.home.join(".agents/skills/shared/SKILL.md");
    write(&shared, SKILL);
    screen.listing = screen.locations.list();
    screen.tab = 2;
    for provider in [Provider::Codex, Provider::Gemini, Provider::Shared] {
        screen.provider = Some(provider);
        assert_eq!(screen.rows().len(), 1);
        assert_eq!(screen.selected().unwrap().path, shared);
    }
    screen.provider = Some(Provider::Claude);
    assert_eq!(screen.rows().len(), 1);
    assert_ne!(screen.selected().unwrap().path, shared);
}

#[test]
fn invalid_load_keeps_path_and_error_visible_and_cancel_keeps_search() {
    let (_dir, mut screen) = setup();
    screen.filter = "config".into();
    press(&mut screen, KeyCode::Char('i'));
    for c in "~/missing/skill".chars() {
        press(&mut screen, KeyCode::Char(c));
    }
    press(&mut screen, KeyCode::Enter);
    assert!(screen.takes_text());
    assert_ne!(screen.note, "");
    let text = render(&mut screen, 60, 20);
    assert!(text.contains("~/missing/skill"), "{text}");
    assert!(text.contains("No such file"), "{text}");
    screen.back();
    assert_eq!(screen.filter, "config");
    assert!(matches!(screen.back(), Action::None));
    assert_eq!(screen.filter, "");
    assert!(matches!(screen.back(), Action::Back));
}

#[test]
fn long_paths_keep_the_cursor_and_destination_visible() {
    let (_dir, mut screen) = setup();
    screen.input = Some(Input::Install {
        path: format!("~/{}final-folder", "directory/".repeat(20)),
        target: 1,
    });
    let text = render(&mut screen, 60, 20);
    assert!(text.contains("final-folder▏"), "{text}");
    assert!(text.contains("~/.agents/skills"), "{text}");
    assert!(!text.contains("e edit"), "{text}");
}

#[test]
fn invalid_edits_keep_the_draft_for_repair_then_save_and_restore() {
    let (_dir, mut screen) = setup();
    screen.tab = 2;
    let original = screen.selected().unwrap().path;
    let Action::Edit(copy) = press(&mut screen, KeyCode::Char('e')) else {
        panic!("edit expected");
    };
    fs::write(&copy, "invalid skill").unwrap();
    screen.edited(Ok(()));
    assert!(screen.is_dialog());
    assert!(copy.exists());
    assert!(render(&mut screen, 80, 24).contains("Skill draft kept"));
    assert!(matches!(press(&mut screen, KeyCode::Char('e')), Action::Edit(path) if path == copy));
    fs::write(&copy, SKILL.replace("docs", "tests")).unwrap();
    screen.edited(Ok(()));
    press(&mut screen, KeyCode::Char('y'));
    assert!(!copy.exists());
    finish_refresh(&mut screen);
    assert_eq!(screen.selected().unwrap().path, original);
    assert_eq!(
        fs::read_to_string(&original).unwrap(),
        SKILL.replace("docs", "tests")
    );
    press(&mut screen, KeyCode::Char('b'));
    assert!(screen.is_dialog());
    assert!(render(&mut screen, 80, 24).contains("+ Read docs."));
    press(&mut screen, KeyCode::Char('y'));
    finish_refresh(&mut screen);
    assert_eq!(fs::read_to_string(original).unwrap(), SKILL);
}

#[test]
fn a_failed_editor_keeps_its_draft_until_discarded() {
    let (_dir, mut screen) = setup();
    screen.tab = 2;
    let Action::Edit(copy) = press(&mut screen, KeyCode::Char('e')) else {
        panic!("edit expected");
    };
    screen.edited(Err("editor failed".into()));
    assert!(copy.exists());
    assert!(screen.is_dialog());
    assert!(render(&mut screen, 60, 20).contains("editor failed"));
    screen.back();
    assert!(!copy.exists());
    assert!(!screen.is_dialog());
}

#[test]
fn failed_restore_keeps_the_reviewed_backup_in_its_draft() {
    let (_dir, mut screen) = setup();
    screen.tab = 2;
    let item = screen.selected().unwrap();
    let edit = Edit::begin(&screen.locations, &item).unwrap();
    edit.save(SKILL.replace("docs", "tests").as_bytes())
        .unwrap();
    press(&mut screen, KeyCode::Char('b'));
    fs::write(&item.path, SKILL.replace("docs", "other")).unwrap();
    press(&mut screen, KeyCode::Char('y'));
    let draft = screen.editing.as_ref().unwrap().draft();
    assert_eq!(fs::read_to_string(&draft).unwrap(), SKILL);
    assert_eq!(
        fs::read_to_string(item.path).unwrap(),
        SKILL.replace("docs", "other")
    );
}

#[test]
fn refresh_preserves_selection_and_preview_keeps_full_metadata() {
    let (_dir, mut screen) = setup();
    screen.tab = 2;
    let selected = screen.selected().unwrap().path;
    write(
        &screen.locations.claude.join("skills/first/SKILL.md"),
        SKILL,
    );
    screen.refresh();
    finish_refresh(&mut screen);
    assert_eq!(screen.list.selected(), Some(1));
    assert_eq!(screen.selected().unwrap().path, selected);
    press(&mut screen, KeyCode::Enter);
    let viewer = screen.viewer.as_ref().unwrap();
    assert_eq!(viewer.source.as_ref(), Some(&selected));
    assert!(screen.hints().contains("o Finder"));
    let text = render(&mut screen, 60, 20);
    assert!(text.contains("~/.claude/skills/sample/SKILL.md"));
}

#[test]
fn chats_show_titles_and_missing_projects_and_search_both() {
    let (_dir, mut screen) = setup();
    let path = screen.locations.claude.join("projects/repo/chat.jsonl");
    write(
        &path,
        &format!(
            r#"{{"cwd":{:?},"message":{{"role":"user","content":"Fix the checkout button"}}}}"#,
            screen.locations.home.join("deleted-repo")
        ),
    );
    press(&mut screen, KeyCode::Tab);
    finish_refresh(&mut screen);
    for (width, height) in visual::tests::SIZES {
        let text = render(&mut screen, width, height);
        assert!(text.contains("Fix the checkout"), "{text}");
        assert!(text.contains("Missing project"), "{text}");
    }
    for query in ["checkout", "deleted-repo", "missing project"] {
        screen.filter = query.into();
        assert_eq!(screen.rows().len(), 1);
    }
    press(&mut screen, KeyCode::Enter);
    assert!(render(&mut screen, 100, 30).contains("Fix the checkout button"));
}

#[test]
fn chat_selection_follows_paths_and_select_all_respects_filters() {
    let (_dir, mut screen) = setup();
    write(
        &screen.locations.codex.join("sessions/rollout-other.jsonl"),
        "{}",
    );
    press(&mut screen, KeyCode::Tab);
    finish_refresh(&mut screen);
    assert!(screen.marked.is_empty());
    press(&mut screen, KeyCode::Char('a'));
    assert_eq!(screen.marked.len(), 2);
    assert!(render(&mut screen, 100, 30).contains("[x] A private chat"));
    screen.provider = Some(Provider::Claude);
    assert!(render(&mut screen, 80, 24).contains("2 selected · 1 hidden"));
    press(&mut screen, KeyCode::Char('a'));
    assert_eq!(screen.marked.len(), 1);
    let selected = screen.marked.clone();
    screen.refresh();
    finish_refresh(&mut screen);
    assert_eq!(screen.marked, selected);
    press(&mut screen, KeyCode::Char(' '));
    assert_eq!(screen.marked.len(), 2);
    press(&mut screen, KeyCode::Char('n'));
    assert!(screen.marked.is_empty());
    press(&mut screen, KeyCode::Tab);
    press(&mut screen, KeyCode::Char('a'));
    assert!(screen.marked.is_empty());
}
