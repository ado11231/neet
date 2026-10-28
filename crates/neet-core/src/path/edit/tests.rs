use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use super::Change::{Move, Remove};
use super::Place::{First, Last};
use super::*;
use crate::dotfiles::configure::zsh::{END, START};

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let home = root.join("home");
    let etc = root.join("etc");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(etc.join("paths.d")).unwrap();
    (dir, home, etc)
}

/// The listing for a PATH of `folders`, with `~` for the home folder
fn listing(home: &Path, etc: &Path, folders: &[&str]) -> Listing {
    let folders: Vec<String> = folders
        .iter()
        .map(|folder| folder.replacen('~', &home.to_string_lossy(), 1))
        .collect();
    crate::path::read(&folders.join(":"), home, etc)
}

fn from_disk(home: &Path) -> impl FnMut(&'static str) -> Result<String, String> + '_ {
    move |name| fs::read_to_string(home.join(name)).map_err(|error| error.to_string())
}

/// Each changed file's new text, for shell `files` and a PATH of `folders`
fn planned(
    files: &[(&str, &str)],
    folders: &[&str],
    changes: &[Change],
) -> Result<Vec<(&'static str, String)>, String> {
    let (_dir, home, etc) = setup();
    for (name, text) in files {
        fs::write(home.join(name), text).unwrap();
    }
    let listing = listing(&home, &etc, folders);
    let planned = plan(&listing, changes, &home, from_disk(&home))?;
    for file in &planned {
        assert_eq!(
            file.before,
            fs::read_to_string(home.join(file.name)).unwrap()
        );
    }
    Ok(planned
        .into_iter()
        .map(|file| (file.name, file.after))
        .collect())
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn takes_only_the_removed_folder_out_of_a_line() {
    let zshrc = "# mine\nexport PATH=\"$HOME/a:$HOME/b:$PATH\"  # tools\nalias l=ls\n";
    assert_eq!(
        planned(
            &[(".zshrc", zshrc)],
            &["~/a", "~/b", "/usr/bin"],
            &[Remove(0)]
        )
        .unwrap(),
        [(".zshrc", zshrc.replace("$HOME/a:", ""))]
    );
    // Unquoted, with `~`, a full path, and PATH in the middle
    let zprofile = "PATH=~/c:/opt/d/bin:${PATH}:$HOME/e\n";
    assert_eq!(
        planned(
            &[(".zprofile", zprofile)],
            &["~/c", "/opt/d/bin", "/usr/bin", "~/e"],
            &[Remove(1), Remove(3)]
        )
        .unwrap(),
        [(".zprofile", "PATH=~/c:${PATH}\n".to_string())]
    );
    // A line that sets all of PATH keeps the rest.
    let bashrc = "export PATH=\"/usr/bin:/bin:$HOME/a\"\n";
    assert_eq!(
        planned(
            &[(".bashrc", bashrc)],
            &["/usr/bin", "/bin", "~/a"],
            &[Remove(2)]
        )
        .unwrap(),
        [(".bashrc", "export PATH=\"/usr/bin:/bin\"\n".to_string())]
    );
}

#[test]
fn removes_a_line_left_with_only_path() {
    let zshrc = "export A=1\nexport PATH=\"$HOME/a:$HOME/b:$PATH\"\nPATH=$PATH:/opt/x # x\nB=2";
    assert_eq!(
        planned(
            &[(".zshrc", zshrc)],
            &["~/a", "~/b", "/usr/bin", "/opt/x"],
            &[Remove(1), Remove(3), Remove(0)]
        )
        .unwrap(),
        [(".zshrc", "export A=1\nB=2".to_string())]
    );
    let error = planned(&[(".zshrc", "PATH=$HOME/a\n")], &["~/a"], &[Remove(0)]).unwrap_err();
    assert!(error.contains("would leave PATH empty"), "{error}");
}

#[test]
fn refuses_lines_it_cannot_fully_read() {
    for zshrc in [
        "export PATH=\"$(brew --prefix)/bin:$HOME/a:$PATH\"\n",
        "export PATH=`pwd`/bin:$HOME/a:$PATH\n",
        "[[ -d $HOME/a ]] && export PATH=\"$HOME/a:$PATH\"\n",
        "export PATH=\"$HOME/a:$PATH\"; echo hi\n",
        "export PATH=\"$GOPATH/bin:$HOME/a:$PATH\"\n",
        "export PATH='$HOME/a:$PATH'\n",
        "export PATH=\"~/a:$PATH\"\n",
        "path=($HOME/a $path)\n",
        "if [[ -o interactive ]]; then\nexport PATH=\"$HOME/a:$PATH\"\nfi\n",
        "load() {\n  export PATH=\"$HOME/a:$PATH\"\n}\n",
        "export EDITOR=vim \\\nPATH=$HOME/a:$PATH\n",
    ] {
        let error = planned(&[(".zshrc", zshrc)], &["~/a"], &[Remove(0)]).unwrap_err();
        assert!(error.starts_with("~/.zshrc, line "), "{zshrc}: {error}");
    }
    // A line after a finished `if` is at the top again.
    let zshrc = "if [[ -o interactive ]]; then\n  y=1\nfi\nexport PATH=\"$HOME/a:$PATH\"\n";
    assert_eq!(
        planned(&[(".zshrc", zshrc)], &["~/a"], &[Remove(0)]).unwrap(),
        [(
            ".zshrc",
            "if [[ -o interactive ]]; then\n  y=1\nfi\n".to_string()
        )]
    );
}

#[test]
fn refuses_tools_system_lists_and_unknown_folders() {
    let (_dir, home, etc) = setup();
    fs::write(home.join(".zshrc"), "eval \"$(mise activate zsh)\"\n").unwrap();
    fs::write(etc.join("paths"), "/usr/bin\n").unwrap();
    let listing = listing(
        &home,
        &etc,
        &[
            "~/.local/share/mise/shims",
            "/usr/bin",
            "/Applications/Editor/bin",
        ],
    );
    for (changes, reason) in [
        (&[Remove(0)][..], "mise adds it"),
        (&[Move(0, First)], "mise adds it"),
        (&[Remove(1)], "which neet never changes"),
        (&[Remove(2)], "No shell file adds it"),
        (&[Move(2, Last)], "No shell file adds it"),
        (&[Remove(9)], "no longer in PATH"),
    ] {
        let error = plan(&listing, changes, &home, from_disk(&home)).unwrap_err();
        assert!(error.contains(reason), "{changes:?}: {error}");
    }
}

#[test]
fn moves_folders_into_the_neet_block() {
    let zshrc = "export PATH=\"$HOME/a:$HOME/b:$PATH\"\n";
    let zprofile = "PATH=~/c:$PATH\n";
    let moved = planned(
        &[(".zshrc", zshrc), (".zprofile", zprofile)],
        &["~/a", "~/b", "~/c"],
        &[Move(1, First), Move(2, Last)],
    )
    .unwrap();
    assert_eq!(moved[0], (".zprofile", String::new()));
    assert_eq!(moved[1].0, ".zshrc");
    assert!(
        moved[1].1.starts_with(&format!(
            "export PATH=\"$HOME/a:$PATH\"\n\n{START}\n# Set by neet"
        )),
        "{}",
        moved[1].1
    );
    assert!(moved[1].1.ends_with(&format!(
        "\nexport PATH=\"$HOME/b:$PATH\"\nexport PATH=\"$PATH:$HOME/c\"\n{END}\n"
    )));

    // Into an existing block, in the order asked, after its settings
    let zshrc = format!("PATH=$HOME/d:$HOME/e:$PATH\n\n{START}\nexport EDITOR='nano'\n{END}\n");
    assert_eq!(
        planned(
            &[(".zshrc", &zshrc)],
            &["~/d", "~/e"],
            &[Move(1, First), Move(0, First)]
        )
        .unwrap(),
        [(
            ".zshrc",
            format!(
                "\n{START}\nexport EDITOR='nano'\nexport PATH=\"$HOME/e:$HOME/d:$PATH\"\n{END}\n"
            )
        )]
    );

    // A broken block, a bash file, and no ~/.zshrc are refused.
    let broken = format!("export PATH=\"$HOME/a:$PATH\"\n{START}\n");
    let error = planned(&[(".zshrc", &broken)], &["~/a"], &[Move(0, Last)]).unwrap_err();
    assert!(error.contains("not whole"), "{error}");
    let bashrc = "export PATH=\"$HOME/a:$PATH\"\n";
    let error = planned(&[(".bashrc", bashrc)], &["~/a"], &[Move(0, Last)]).unwrap_err();
    assert!(error.contains("which only zsh reads"), "{error}");
    let error = planned(&[(".zprofile", zprofile)], &["~/c"], &[Move(0, Last)]).unwrap_err();
    assert!(error.starts_with("~/.zshrc: "), "{error}");
}

#[test]
fn keeps_a_copy_another_line_does_not_add() {
    // One line, read twice by a nested shell, adds both copies.
    let zshrc = "export PATH=\"$HOME/a:$PATH\"\n";
    let folders = ["~/a", "/usr/bin", "~/a"];
    let error = planned(&[(".zshrc", zshrc)], &folders, &[Remove(2)]).unwrap_err();
    assert!(error.contains("copy at #1"), "{error}");
    let error = planned(&[(".zshrc", zshrc)], &folders, &[Remove(0)]).unwrap_err();
    assert!(error.contains("copy at #3"), "{error}");
    assert_eq!(
        planned(&[(".zshrc", zshrc)], &folders, &[Remove(0), Remove(2)]).unwrap(),
        [(".zshrc", String::new())]
    );
    // With two lines adding it, one copy stays.
    assert_eq!(
        planned(
            &[(".zprofile", zshrc), (".zshrc", zshrc)],
            &folders,
            &[Remove(2)]
        )
        .unwrap(),
        [(".zprofile", String::new())]
    );
}

#[test]
fn refuses_a_line_that_changed_after_the_listing() {
    let (_dir, home, etc) = setup();
    fs::write(home.join(".zshrc"), "export PATH=\"$HOME/a:$PATH\"\n").unwrap();
    let listing = listing(&home, &etc, &["~/a"]);
    let error = plan(
        &listing,
        &[Remove(0), Move(0, First)],
        &home,
        from_disk(&home),
    )
    .unwrap_err();
    assert!(error.contains("changed twice"), "{error}");
    fs::write(home.join(".zshrc"), "export PATH=\"$HOME/b:$PATH\"\n").unwrap();
    let error = plan(&listing, &[Remove(0)], &home, from_disk(&home)).unwrap_err();
    assert!(error.contains("no longer adds"), "{error}");
    fs::write(home.join(".zshrc"), "").unwrap();
    let error = plan(&listing, &[Remove(0)], &home, from_disk(&home)).unwrap_err();
    assert!(error.contains("is gone"), "{error}");
}

#[test]
fn writes_each_file_after_a_backup_and_restores_it() {
    let (_dir, home, etc) = setup();
    let zshrc = home.join(".zshrc");
    let zprofile = home.join(".zprofile");
    let old = "export PATH=\"$HOME/a:$HOME/b:$PATH\"\n";
    fs::write(&zshrc, old).unwrap();
    fs::write(&zprofile, "PATH=$HOME/c:$PATH\n").unwrap();
    let listing = listing(&home, &etc, &["~/a", "~/b", "~/c"]);
    let backups = Backups::dotfiles(&home);

    let prepared = prepare(&listing, &[Remove(0), Move(2, Last)], &home).unwrap();
    let done = write(&prepared, &home, &backups, at(100)).unwrap();

    assert_eq!(done, [Done::Written, Done::Written]);
    assert_eq!(fs::read_to_string(&zprofile).unwrap(), "");
    let new = fs::read_to_string(&zshrc).unwrap();
    assert!(new.starts_with("export PATH=\"$HOME/b:$PATH\"\n"), "{new}");
    assert!(new.contains("\nexport PATH=\"$PATH:$HOME/c\"\n"), "{new}");
    let saved = backups.list(".zshrc").unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(fs::read_to_string(&saved[0].path).unwrap(), old);
    assert_eq!(backups.list(".zprofile").unwrap().len(), 1);
    let editing = fs::read_dir(home.join(".local/state/neet/editing")).unwrap();
    assert_eq!(editing.count(), 0);

    // Dotfiles restores it like any other edit.
    let dotfiles = crate::dotfiles::list(&home);
    let file = dotfiles.files.iter().find(|f| f.path == zshrc).unwrap();
    change::restore(file, &saved[0], &backups, at(200)).unwrap();
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), old);
}

#[test]
fn writes_nothing_when_a_file_changed_or_fails_its_check() {
    let (_dir, home, etc) = setup();
    let zshrc = home.join(".zshrc");
    fs::write(&zshrc, "export PATH=\"$HOME/a:$HOME/b:$PATH\"\n").unwrap();
    let listing = listing(&home, &etc, &["~/a", "~/b"]);
    let backups = Backups::dotfiles(&home);

    let prepared = prepare(&listing, &[Remove(0)], &home).unwrap();
    fs::write(&zshrc, "changed\n").unwrap();
    assert!(write(&prepared, &home, &backups, at(100)).is_err());
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), "changed\n");

    let broken = "export PATH=\"$HOME/a:$HOME/b:$PATH\"\nif x\n";
    fs::write(&zshrc, broken).unwrap();
    let prepared = prepare(&listing, &[Remove(0)], &home).unwrap();
    let error = write(&prepared, &home, &backups, at(100)).unwrap_err();
    assert!(error.contains("The check of ~/.zshrc failed"), "{error}");
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), broken);
    assert_eq!(backups.list(".zshrc").unwrap(), []);
}
