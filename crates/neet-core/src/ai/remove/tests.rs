use super::*;
use crate::ai::tests::{setup, write};
use std::cell::Cell;
use std::os::unix::fs::symlink;

fn chat(locations: &Locations) -> Item {
    write(
        &locations.claude.join("projects/repo/chat.jsonl"),
        "original",
    );
    locations.list().items.remove(0)
}

#[test]
fn moves_only_the_reviewed_file_and_keeps_siblings() {
    let (_dir, locations) = setup();
    let item = chat(&locations);
    let sibling = item.path.with_file_name("other.jsonl");
    write(&sibling, "keep");
    let trash = locations.home.join("Trash");
    fs::create_dir(&trash).unwrap();
    let removal = Removal::prepare(&locations, &item).unwrap();
    removal
        .apply_with(&|_| Ok(()), &|path| {
            fs::rename(path, trash.join("chat.jsonl"))
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(trash.join("chat.jsonl")).unwrap(),
        "original"
    );
    assert!(!item.path.exists());
    assert_eq!(fs::read_to_string(sibling).unwrap(), "keep");
}

#[test]
fn refuses_changes_before_and_after_review() {
    let (_dir, locations) = setup();
    let item = chat(&locations);
    let removal = Removal::prepare(&locations, &item).unwrap();
    fs::write(&item.path, "changed after review").unwrap();
    assert!(Removal::prepare(&locations, &item).is_err());
    let moved = Cell::new(false);
    assert!(
        removal
            .apply_with(&|_| Ok(()), &|_| {
                moved.set(true);
                Ok(())
            })
            .is_err()
    );
    assert!(!moved.get());
}

#[test]
fn refuses_replaced_files_and_linked_parents() {
    let (_dir, locations) = setup();
    let item = chat(&locations);
    let removal = Removal::prepare(&locations, &item).unwrap();
    let original = item.path.with_extension("old");
    fs::rename(&item.path, &original).unwrap();
    symlink(&original, &item.path).unwrap();
    assert!(
        removal
            .apply_with(&|_| Ok(()), &|_| panic!("must not move"))
            .is_err()
    );
    let folder = item.path.parent().unwrap();
    let moved = locations.home.join("moved");
    fs::rename(folder, &moved).unwrap();
    symlink(&moved, folder).unwrap();
    assert!(Removal::prepare(&locations, &item).is_err());
}

#[test]
fn refuses_settings_and_forged_history_paths() {
    let (_dir, locations) = setup();
    let mut item = chat(&locations);
    item.path = locations.codex.join("auth.jsonl");
    write(&item.path, "secret");
    item.provider = Provider::Codex;
    assert!(Removal::prepare(&locations, &item).is_err());
    item.path = locations.codex.join("sessions/other.jsonl");
    write(&item.path, "not a rollout");
    assert!(Removal::prepare(&locations, &item).is_err());
    item.kind = Kind::Settings;
    assert!(Removal::prepare(&locations, &item).is_err());
}

#[test]
fn keeps_busy_chats_and_reports_move_errors() {
    let (_dir, locations) = setup();
    let item = chat(&locations);
    let removal = Removal::prepare(&locations, &item).unwrap();
    let busy = |_: &Path| Err(io::Error::other("open"));
    assert!(
        removal
            .apply_with(&busy, &|_| panic!("must not move"))
            .is_err()
    );
    assert!(
        removal
            .apply_with(&|_| Ok(()), &|_| Err(io::Error::other("Finder refused")))
            .is_err()
    );
    assert_eq!(fs::read_to_string(item.path).unwrap(), "original");
}

#[test]
fn recently_written_files_are_skipped_without_running_finder() {
    let (_dir, locations) = setup();
    let item = chat(&locations);
    let error = Removal::prepare(&locations, &item)
        .unwrap()
        .apply()
        .unwrap_err();
    assert!(error.to_string().contains("Recently updated"));
    assert!(item.path.exists());
}
