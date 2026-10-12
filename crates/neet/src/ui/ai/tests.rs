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
        assert!(text.contains("▸ Codex"), "{text}");
        assert!(!text.contains("A private chat"));
    }
    press(&mut screen, KeyCode::Tab);
    assert_eq!(screen.selected().unwrap().kind, Kind::Chat);
    assert!(!render(&mut screen, 100, 30).contains("A private chat"));
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
    assert!(screen.note.contains("Skill loaded"));
}
