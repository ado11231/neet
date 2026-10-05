//! Changing a listed dotfile: editing it, keeping the version in the home
//! folder in chezmoi, or putting chezmoi's version back. Every change is
//! backed up first. See how a change runs, and chezmoi, in
//! `docs/SAFETY.md`.

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Chezmoi, Dotfile, Managed, Syntax};
use crate::rewrite::{self, Backup, Backups, Opened, WriteError};

/// How long a syntax check may take
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// How long chezmoi may take to change one file
const CHEZMOI_TIMEOUT: Duration = Duration::from_secs(60);

/// What a syntax check found
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    Passed,
    /// The check's own message
    Failed(String),
    /// Why nothing was checked
    NotChecked(String),
}

/// Checks the file at `path` with `syntax`. A check only reads the file. It
/// never runs it, and no shell reads its startup files. The copy's path is
/// replaced by `name` in messages.
#[must_use]
pub fn check(syntax: Syntax, path: &Path, name: &str) -> Checked {
    let (program, args): (&str, &[&str]) = match syntax {
        Syntax::Zsh => ("zsh", &["-f", "-n"]),
        Syntax::Bash => ("bash", &["--norc", "--noprofile", "-n"]),
        Syntax::Fish => ("fish", &["--no-config", "--no-execute"]),
        Syntax::Git => ("git", &["config", "--list", "--file"]),
        Syntax::Toml => {
            return match fs::read_to_string(path).map(|text| text.parse::<toml::Table>()) {
                Ok(Ok(_)) => Checked::Passed,
                Ok(Err(error)) => Checked::Failed(error.to_string().trim().to_string()),
                Err(error) => Checked::Failed(error.to_string()),
            };
        }
        Syntax::None => {
            return Checked::NotChecked("There is no check for this file.".to_string());
        }
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .arg(path)
        .env_remove("BASH_ENV")
        .env_remove("ENV");
    match crate::run::with_timeout(&mut command, CHECK_TIMEOUT) {
        Ok(Some((true, _))) => Checked::Passed,
        Ok(Some((false, message))) => Checked::Failed(
            message
                .replace(&path.display().to_string(), name)
                .trim()
                .to_string(),
        ),
        Ok(None) => Checked::Failed(format!(
            "The check did not finish within {} seconds.",
            CHECK_TIMEOUT.as_secs()
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Checked::NotChecked(format!(
            "{program} is not installed, so it was not checked."
        )),
        Err(error) => Checked::NotChecked(format!("The check could not run: {error}.")),
    }
}

/// What a change did
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// The file in the home folder was written.
    Written,
    /// The source file was written, and chezmoi wrote the home folder's file
    /// from it.
    Applied,
    /// chezmoi's source file now holds the home folder's version.
    Kept,
    /// The source file was written. Run this command to write the home
    /// folder's file.
    ApplyYourself(String),
    /// The source file was written, but chezmoi failed. Its message, and the
    /// command to run once fixed.
    ApplyFailed { error: String, command: String },
}

/// The source file chezmoi keeps for a listed file, opened
struct Source {
    opened: Opened,
    /// Its backup name, such as `chezmoi/dot_zshrc`
    name: String,
    /// Whether neet may run chezmoi
    may_run: bool,
}

/// A listed file opened to edit, with chezmoi's source file when chezmoi
/// manages it
pub struct Edit {
    name: String,
    syntax: Syntax,
    home: Opened,
    source: Option<Source>,
    /// The file's path from the home folder, for the command to run yourself
    shown: String,
}

impl Edit {
    /// Opens `file` to edit, or says why it cannot be.
    ///
    /// # Errors
    ///
    /// Returns why the file cannot be edited, in plain words.
    pub fn begin(file: &Dotfile, chezmoi: Option<&Chezmoi>) -> Result<Self, String> {
        let home = open_home(file)?;
        let source = match &file.managed {
            Some(Managed::Differs(_)) => return Err(differs()),
            Some(Managed::InSync(source)) => {
                let source = open_source(source, chezmoi)?;
                if source.opened.contents() != home.contents() {
                    return Err(
                        "It no longer matches its source file in chezmoi. Open Dotfiles again."
                            .to_string(),
                    );
                }
                Some(source)
            }
            _ => None,
        };
        Ok(Self {
            name: file.known.path.to_string(),
            syntax: file.known.syntax,
            home,
            source,
            shown: format!("~/{}", file.known.path),
        })
    }

    /// What the edit starts from
    #[must_use]
    pub fn contents(&self) -> &[u8] {
        self.home.contents()
    }

    #[must_use]
    pub fn syntax(&self) -> Syntax {
        self.syntax
    }

    /// The file's name, such as `kitty.conf`
    #[must_use]
    pub fn file_name(&self) -> &str {
        self.name.rsplit('/').next().unwrap_or(&self.name)
    }

    /// The file that is written: chezmoi's source file, or the file itself
    #[must_use]
    pub fn writes(&self) -> &Path {
        self.source
            .as_ref()
            .map_or(self.home.path(), |source| source.opened.path())
    }

    /// Whether it writes chezmoi's source file instead of the file itself
    #[must_use]
    pub fn edits_source(&self) -> bool {
        self.source.is_some()
    }

    /// Whether chezmoi then writes the file in the home folder
    #[must_use]
    pub fn applies(&self) -> bool {
        self.source.as_ref().is_some_and(|source| source.may_run)
    }

    /// Writes a copy to edit, in a folder only you can open inside
    /// `~/.local/state/neet/editing`, with the file's own name so editors
    /// know its kind.
    ///
    /// # Errors
    ///
    /// Returns an error if the copy could not be written.
    pub fn copy(&self, home: &Path) -> io::Result<PathBuf> {
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let folder = home.join(".local/state/neet/editing").join(format!(
            "{}-{}",
            std::process::id(),
            since.as_nanos()
        ));
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&folder)?;
        let path = folder.join(self.file_name());
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(self.contents())?;
        file.sync_all()?;
        Ok(path)
    }

    /// Writes `contents`, after a backup. With chezmoi, the source file is
    /// written, then chezmoi writes the file in the home folder when neet
    /// may run it.
    ///
    /// # Errors
    ///
    /// Returns why nothing was written, in plain words.
    pub fn finish(
        &self,
        contents: &[u8],
        backups: &Backups,
        now: SystemTime,
    ) -> Result<Done, String> {
        let Some(source) = &self.source else {
            rewrite::write(&self.home, contents, backups, &self.name, now)
                .map_err(|error| plain(&error))?;
            return Ok(Done::Written);
        };
        // chezmoi overwrites the home folder's file, so it is backed up too,
        // and must still be as it was.
        if !self
            .home
            .is_unchanged()
            .map_err(|error| plain(&error.into()))?
        {
            return Err(plain(&WriteError::Changed));
        }
        backups
            .save(&self.name, self.home.contents(), self.home.mode(), now)
            .map_err(|error| plain(&error.into()))?;
        rewrite::write(&source.opened, contents, backups, &source.name, now)
            .map_err(|error| plain(&error))?;
        let command = format!("chezmoi apply {}", self.shown);
        if !source.may_run {
            return Ok(Done::ApplyYourself(command));
        }
        Ok(
            match run_chezmoi(
                &["apply", "--force", "--exclude=scripts,externals"],
                self.home.path(),
            ) {
                Ok(()) if fs::read(self.home.path()).is_ok_and(|now| now == contents) => {
                    Done::Applied
                }
                Ok(()) => Done::ApplyFailed {
                    error: "chezmoi finished, but the file does not match yet".to_string(),
                    command,
                },
                Err(error) => Done::ApplyFailed { error, command },
            },
        )
    }
}

/// Removes a copy made by [`Edit::copy`], and its folder.
pub fn discard(copy: &Path) {
    let _ = fs::remove_file(copy);
    if let Some(folder) = copy.parent() {
        let _ = fs::remove_dir(folder);
    }
}

/// Makes chezmoi's source file hold the version in the home folder, for a
/// file that differs from it. The source file is backed up first.
///
/// # Errors
///
/// Returns why nothing was changed, in plain words.
pub fn keep_home(
    file: &Dotfile,
    chezmoi: Option<&Chezmoi>,
    backups: &Backups,
    now: SystemTime,
) -> Result<Done, String> {
    let (home, source) = open_differing(file, chezmoi)?;
    if !source.may_run {
        // For a plain source file, this is what `chezmoi re-add` does.
        rewrite::write(&source.opened, home.contents(), backups, &source.name, now)
            .map_err(|error| plain(&error))?;
        return Ok(Done::Kept);
    }
    if !source
        .opened
        .is_unchanged()
        .map_err(|error| plain(&error.into()))?
    {
        return Err(plain(&WriteError::Changed));
    }
    backups
        .save(
            &source.name,
            source.opened.contents(),
            source.opened.mode(),
            now,
        )
        .map_err(|error| plain(&error.into()))?;
    run_chezmoi(&["re-add"], home.path())?;
    if fs::read(source.opened.path()).is_ok_and(|now| now == home.contents()) {
        Ok(Done::Kept)
    } else {
        Err("chezmoi finished, but the source file does not match yet.".to_string())
    }
}

/// Puts chezmoi's version back in the home folder, for a file that differs
/// from its source file. The file is backed up first.
///
/// # Errors
///
/// Returns why nothing was changed, in plain words, including when neet may
/// not run chezmoi.
pub fn put_back(
    file: &Dotfile,
    chezmoi: Option<&Chezmoi>,
    backups: &Backups,
    now: SystemTime,
) -> Result<Done, String> {
    let (home, source) = open_differing(file, chezmoi)?;
    if !source.may_run {
        return Err(format!(
            "neet will not run chezmoi here. Run chezmoi apply ~/{} yourself.",
            file.known.path
        ));
    }
    if !home.is_unchanged().map_err(|error| plain(&error.into()))? {
        return Err(plain(&WriteError::Changed));
    }
    backups
        .save(file.known.path, home.contents(), home.mode(), now)
        .map_err(|error| plain(&error.into()))?;
    run_chezmoi(
        &["apply", "--force", "--exclude=scripts,externals"],
        home.path(),
    )?;
    if fs::read(home.path()).is_ok_and(|now| now == source.opened.contents()) {
        Ok(Done::Applied)
    } else {
        Err("chezmoi finished, but the file does not match its source file yet.".to_string())
    }
}

/// Puts `backup` back over the file in the home folder. The file now is
/// backed up first, so the restore can be undone too.
///
/// # Errors
///
/// Returns why nothing was changed, in plain words.
pub fn restore(
    file: &Dotfile,
    backup: &Backup,
    backups: &Backups,
    now: SystemTime,
) -> Result<Done, String> {
    open_home(file)?;
    rewrite::restore(&file.path, backup, backups, file.known.path, now)
        .map_err(|error| plain(&error))?;
    Ok(Done::Written)
}

fn differs() -> String {
    "It differs from its source file in chezmoi. Press r to keep this version, or p to put the source back, first."
        .to_string()
}

fn open_home(file: &Dotfile) -> Result<Opened, String> {
    if file.found.is_none() {
        return Err("It is not on this Mac.".to_string());
    }
    if let Some(view_only) = &file.view_only {
        return Err(format!("{}. View only.", view_only.describe()));
    }
    Opened::open(&file.path).map_err(|error| format!("It could not be opened: {error}."))
}

fn open_source(path: &Path, chezmoi: Option<&Chezmoi>) -> Result<Source, String> {
    let chezmoi = chezmoi.ok_or("chezmoi's folder was not found.")?;
    let opened = Opened::open(path)
        .map_err(|error| format!("Its source file could not be opened: {error}."))?;
    let relative = path.strip_prefix(&chezmoi.source).unwrap_or(path);
    Ok(Source {
        opened,
        name: format!("chezmoi/{}", relative.display()),
        may_run: chezmoi.may_not_run.is_none(),
    })
}

fn open_differing(file: &Dotfile, chezmoi: Option<&Chezmoi>) -> Result<(Opened, Source), String> {
    let Some(Managed::Differs(source)) = &file.managed else {
        return Err("It does not differ from a source file in chezmoi.".to_string());
    };
    let home = open_home(file)?;
    let source = open_source(source, chezmoi)?;
    Ok((home, source))
}

/// Runs chezmoi for one file, with no terminal, so it can never ask.
fn run_chezmoi(args: &[&str], target: &Path) -> Result<(), String> {
    let mut command = Command::new("chezmoi");
    command.args(args).arg("--no-tty").arg("--").arg(target);
    match crate::run::with_timeout(&mut command, CHEZMOI_TIMEOUT) {
        Ok(Some((true, _))) => Ok(()),
        Ok(Some((false, message))) => Err(format!("chezmoi failed: {}", message.trim())),
        Ok(None) => Err(format!(
            "chezmoi did not finish within {} seconds.",
            CHEZMOI_TIMEOUT.as_secs()
        )),
        Err(error) => Err(format!("chezmoi could not run: {error}.")),
    }
}

/// A write error as a sentence for the screen
fn plain(error: &WriteError) -> String {
    let text = error.to_string();
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        format!("{}{}.", first.to_uppercase(), characters.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Found, Group, Known};
    use super::*;

    static ZSHRC: Known = Known {
        path: ".zshrc",
        group: Group::Shell,
        syntax: Syntax::Zsh,
    };

    fn found() -> Found {
        Found {
            size: 0,
            modified: None,
            mode: 0o644,
            link: None,
        }
    }

    fn dotfile(home: &Path, managed: Option<Managed>) -> Dotfile {
        Dotfile {
            known: &ZSHRC,
            path: home.join(".zshrc"),
            found: Some(found()),
            view_only: None,
            managed,
            may_hold_secrets: false,
        }
    }

    fn setup() -> (tempfile::TempDir, PathBuf, Backups) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        fs::write(home.join(".zshrc"), "old\n").unwrap();
        let backups = Backups::dotfiles(&home);
        (dir, home, backups)
    }

    /// chezmoi with its source folder in the home folder. neet may not run
    /// it, so tests never start the real chezmoi.
    fn chezmoi(home: &Path) -> Chezmoi {
        let source = home.join(".local/share/chezmoi");
        fs::create_dir_all(&source).unwrap();
        Chezmoi {
            source,
            may_not_run: Some("tests".to_string()),
        }
    }

    #[test]
    fn checks_only_read_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran");
        let good = dir.path().join("good");
        fs::write(&good, format!("touch {}\n", marker.display())).unwrap();
        assert_eq!(check(Syntax::Zsh, &good, ".zshrc"), Checked::Passed);
        assert_eq!(check(Syntax::Bash, &good, ".bashrc"), Checked::Passed);
        assert!(!marker.exists());

        let bad = dir.path().join("bad");
        fs::write(&bad, "if x\n").unwrap();
        let Checked::Failed(message) = check(Syntax::Zsh, &bad, ".zshrc") else {
            panic!("a broken file must fail");
        };
        assert!(message.contains(".zshrc"), "{message}");
        assert!(
            !message.contains(&dir.path().display().to_string()),
            "{message}"
        );
    }

    #[test]
    fn checks_git_and_toml_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x");
        fs::write(&path, "[user]\n\tname = You\n").unwrap();
        assert_eq!(check(Syntax::Git, &path, ".gitconfig"), Checked::Passed);
        fs::write(&path, "[user\n").unwrap();
        assert!(matches!(
            check(Syntax::Git, &path, ".gitconfig"),
            Checked::Failed(_)
        ));
        fs::write(&path, "format = \"$all\"\n").unwrap();
        assert_eq!(check(Syntax::Toml, &path, "starship.toml"), Checked::Passed);
        fs::write(&path, "format = \n").unwrap();
        assert!(matches!(
            check(Syntax::Toml, &path, "starship.toml"),
            Checked::Failed(_)
        ));
        assert!(matches!(
            check(Syntax::None, &path, "x"),
            Checked::NotChecked(_)
        ));
    }

    #[test]
    fn edits_a_file_chezmoi_does_not_manage() {
        let (_dir, home, backups) = setup();
        let edit = Edit::begin(&dotfile(&home, None), None).unwrap();
        assert_eq!(edit.contents(), b"old\n");
        assert_eq!(edit.writes(), home.join(".zshrc"));
        let copy = edit.copy(&home).unwrap();
        assert_eq!(copy.file_name().unwrap(), ".zshrc");
        assert_eq!(fs::read(&copy).unwrap(), b"old\n");

        assert_eq!(
            edit.finish(b"new\n", &backups, SystemTime::now()),
            Ok(Done::Written)
        );

        assert_eq!(fs::read_to_string(home.join(".zshrc")).unwrap(), "new\n");
        assert_eq!(backups.list(".zshrc").unwrap().len(), 1);
        discard(&copy);
        assert!(!copy.parent().unwrap().exists());
    }

    #[test]
    fn edits_the_source_file_when_chezmoi_manages_it() {
        let (_dir, home, backups) = setup();
        let chezmoi = chezmoi(&home);
        let source = chezmoi.source.join("dot_zshrc");
        fs::write(&source, "old\n").unwrap();
        let file = dotfile(&home, Some(Managed::InSync(source.clone())));

        let edit = Edit::begin(&file, Some(&chezmoi)).unwrap();
        assert_eq!(edit.writes(), source);
        assert!(!edit.applies());
        let done = edit.finish(b"new\n", &backups, SystemTime::now()).unwrap();

        assert_eq!(
            done,
            Done::ApplyYourself("chezmoi apply ~/.zshrc".to_string())
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), "new\n");
        assert_eq!(fs::read_to_string(home.join(".zshrc")).unwrap(), "old\n");
        assert_eq!(backups.list(".zshrc").unwrap().len(), 1);
        assert_eq!(backups.list("chezmoi/dot_zshrc").unwrap().len(), 1);
    }

    #[test]
    fn refuses_files_that_cannot_be_edited_yet() {
        let (_dir, home, _backups) = setup();
        let chezmoi = chezmoi(&home);
        let source = chezmoi.source.join("dot_zshrc");
        fs::write(&source, "other\n").unwrap();

        let differs = dotfile(&home, Some(Managed::Differs(source.clone())));
        assert!(
            Edit::begin(&differs, Some(&chezmoi))
                .err()
                .unwrap()
                .contains("Press r")
        );
        let stale = dotfile(&home, Some(Managed::InSync(source)));
        assert!(
            Edit::begin(&stale, Some(&chezmoi))
                .err()
                .unwrap()
                .contains("no longer matches")
        );
        let mut missing = dotfile(&home, None);
        missing.found = None;
        assert!(Edit::begin(&missing, None).is_err());
        let mut view_only = dotfile(&home, None);
        view_only.view_only = Some(super::super::ViewOnly::SshConfig);
        assert!(
            Edit::begin(&view_only, None)
                .err()
                .unwrap()
                .contains("View only")
        );
    }

    #[test]
    fn refuses_to_write_when_the_home_file_changed_meanwhile() {
        let (_dir, home, backups) = setup();
        let chezmoi = chezmoi(&home);
        let source = chezmoi.source.join("dot_zshrc");
        fs::write(&source, "old\n").unwrap();
        let file = dotfile(&home, Some(Managed::InSync(source.clone())));
        let edit = Edit::begin(&file, Some(&chezmoi)).unwrap();
        fs::write(home.join(".zshrc"), "changed\n").unwrap();

        let error = edit
            .finish(b"new\n", &backups, SystemTime::now())
            .unwrap_err();

        assert!(
            error.starts_with("It changed since you opened it"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), "old\n");
    }

    #[test]
    fn keeps_the_home_version_in_chezmoi_without_running_it() {
        let (_dir, home, backups) = setup();
        let chezmoi = chezmoi(&home);
        let source = chezmoi.source.join("dot_zshrc");
        fs::write(&source, "source\n").unwrap();
        let file = dotfile(&home, Some(Managed::Differs(source.clone())));

        assert_eq!(
            keep_home(&file, Some(&chezmoi), &backups, SystemTime::now()),
            Ok(Done::Kept)
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), "old\n");
        assert_eq!(backups.list("chezmoi/dot_zshrc").unwrap().len(), 1);

        let error = put_back(&file, Some(&chezmoi), &backups, SystemTime::now()).unwrap_err();
        assert!(error.contains("chezmoi apply ~/.zshrc"), "{error}");
    }

    #[test]
    fn restores_a_backup_of_the_home_file() {
        let (_dir, home, backups) = setup();
        let file = dotfile(&home, None);
        let edit = Edit::begin(&file, None).unwrap();
        edit.finish(b"new\n", &backups, SystemTime::now()).unwrap();
        let saved = backups.list(".zshrc").unwrap();

        assert_eq!(
            restore(&file, &saved[0], &backups, SystemTime::now()),
            Ok(Done::Written)
        );
        assert_eq!(fs::read_to_string(home.join(".zshrc")).unwrap(), "old\n");
        assert_eq!(backups.list(".zshrc").unwrap().len(), 2);

        let mut view_only = dotfile(&home, None);
        view_only.view_only = Some(super::super::ViewOnly::SshConfig);
        assert!(restore(&view_only, &saved[0], &backups, SystemTime::now()).is_err());
    }
}
