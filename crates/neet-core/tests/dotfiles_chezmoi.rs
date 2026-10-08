//! An opt in round trip with the installed chezmoi and an isolated home.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use neet_core::dotfiles::{self, Dotfile, Managed, change, configure, export};
use neet_core::rewrite::Backups;

fn git(folder: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn file(home: &Path, name: &str) -> (Dotfile, dotfiles::Chezmoi) {
    let listing = dotfiles::list(home);
    (
        listing
            .files
            .into_iter()
            .find(|file| file.known.path == name)
            .unwrap(),
        listing.chezmoi.unwrap(),
    )
}

#[test]
#[ignore = "needs chezmoi; run with cargo test -p neet-core --test dotfiles_chezmoi -- --ignored"]
fn real_chezmoi_round_trip() {
    if let Some(home) = std::env::var_os("NEET_DOTFILES_TEST_HOME") {
        round_trip(Path::new(&home));
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "real_chezmoi_round_trip",
            "--ignored",
            "--nocapture",
        ])
        .env("HOME", &home)
        .env("NEET_DOTFILES_TEST_HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn round_trip(home: &Path) {
    let repo = home.join(".local/share/chezmoi");
    let source = repo.join("mac");
    fs::create_dir_all(&source).unwrap();
    fs::write(repo.join(".chezmoiroot"), "mac\n").unwrap();
    fs::write(source.join("dot_zshrc"), "export EDITOR=vi\n").unwrap();
    fs::write(home.join(".zshrc"), "export EDITOR=vi\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &["commit", "-q", "-m", "test(dotfiles): initial files"],
    );
    let backups = Backups::dotfiles(home);

    let (zsh, chezmoi) = file(home, ".zshrc");
    assert_eq!(chezmoi.may_not_run, None);
    let edit = change::Edit::begin(&zsh, Some(&chezmoi)).unwrap();
    assert_eq!(
        edit.finish(b"export EDITOR=nvim\n", &backups, SystemTime::now())
            .unwrap(),
        change::Done::Applied
    );
    assert_eq!(
        fs::read_to_string(home.join(".zshrc")).unwrap(),
        "export EDITOR=nvim\n"
    );
    assert_eq!(backups.list(".zshrc").unwrap().len(), 1);
    assert_eq!(backups.list("chezmoi/dot_zshrc").unwrap().len(), 1);

    fs::write(home.join(".zshrc"), "export EDITOR=hx\n").unwrap();
    let (zsh, chezmoi) = file(home, ".zshrc");
    assert_eq!(
        change::keep_home(&zsh, Some(&chezmoi), &backups, SystemTime::now()).unwrap(),
        change::Done::Kept
    );
    fs::write(home.join(".zshrc"), "export EDITOR=other\n").unwrap();
    let (zsh, chezmoi) = file(home, ".zshrc");
    assert_eq!(
        change::put_back(&zsh, Some(&chezmoi), &backups, SystemTime::now()).unwrap(),
        change::Done::Applied
    );
    let backup = backups.list(".zshrc").unwrap().pop().unwrap();
    change::restore(&zsh, &backup, &backups, SystemTime::now()).unwrap();
    assert_eq!(
        fs::read_to_string(home.join(".zshrc")).unwrap(),
        "export EDITOR=vi\n"
    );

    fs::write(home.join(".gitconfig"), "[user]\nname = Test\n").unwrap();
    let (config, chezmoi) = file(home, ".gitconfig");
    assert!(matches!(
        change::add(&config, Some(&chezmoi)).unwrap(),
        change::Done::Added(_)
    ));
    let (config, chezmoi) = file(home, ".gitconfig");
    let edit = change::Edit::begin(&config, Some(&chezmoi)).unwrap();
    let copy = edit.copy(home).unwrap();
    let program = configure::program_for(".gitconfig").unwrap();
    configure::set(program, &copy, &program.settings[0], Some("Changed")).unwrap();
    edit.finish(&fs::read(&copy).unwrap(), &backups, SystemTime::now())
        .unwrap();
    change::discard(&copy);
    assert!(
        fs::read_to_string(home.join(".gitconfig"))
            .unwrap()
            .contains("Changed")
    );

    fs::write(home.join("shell"), "export LINK=1\n").unwrap();
    symlink(home.join("shell"), home.join(".bashrc")).unwrap();
    let (bash, chezmoi) = file(home, ".bashrc");
    assert!(change::add(&bash, Some(&chezmoi)).is_err());
    let edit = change::Edit::begin(&bash, Some(&chezmoi)).unwrap();
    edit.finish(b"export LINK=2\n", &backups, SystemTime::now())
        .unwrap();
    assert!(
        fs::symlink_metadata(home.join(".bashrc"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(home.join("shell")).unwrap(),
        "export LINK=2\n"
    );

    export_round_trip(home, &repo);
    refuses_new_hooks(home, &backups);
}

fn export_round_trip(home: &Path, repo: &Path) {
    let remote = home.join("remote.git");
    git(home, &["init", "--bare", "-q", remote.to_str().unwrap()]);
    git(repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git(repo, &["push", "-q", "-u", "origin", "main"]);
    let plan = export::plan(&dotfiles::list(home)).unwrap();
    let paths: Vec<&str> = plan
        .pending
        .iter()
        .map(|pending| pending.path.as_str())
        .collect();
    assert_eq!(paths.len(), 2);
    export::commit(&plan, &paths, "test(dotfiles): save reviewed files").unwrap();
    export::push(repo, &export::push_target(repo).unwrap()).unwrap();
    assert_eq!(export::plan(&dotfiles::list(home)).unwrap().pending, []);
    assert!(matches!(
        file(home, ".gitconfig").0.managed,
        Some(Managed::InSync(_))
    ));
}

fn refuses_new_hooks(home: &Path, backups: &Backups) {
    let (config, chezmoi) = file(home, ".gitconfig");
    let edit = change::Edit::begin(&config, Some(&chezmoi)).unwrap();
    let before = fs::read(home.join(".gitconfig")).unwrap();
    let folder = home.join(".config/chezmoi");
    fs::create_dir_all(&folder).unwrap();
    let marker = home.join("hook-ran");
    fs::write(
        folder.join("chezmoi.toml"),
        format!(
            "[hooks.apply.pre]\ncommand = \"touch\"\nargs = [\"{}\"]\n",
            marker.display()
        ),
    )
    .unwrap();
    let result = edit
        .finish(b"[user]\nname = Later\n", backups, SystemTime::now())
        .unwrap();
    assert!(matches!(result, change::Done::ApplyFailed { .. }));
    assert_eq!(fs::read(home.join(".gitconfig")).unwrap(), before);
    assert!(!marker.exists());
}
