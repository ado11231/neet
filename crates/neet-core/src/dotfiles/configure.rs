//! Changing a program's settings one at a time, from a list neet knows.
//! Each change is made on a copy of the file, which then goes through the
//! same check, diff, and backup as an edit. See Configure in
//! `docs/SAFETY.md`.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// How long Git may take to read or change one file
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// What kind of value a setting takes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// `true` or `false`
    OnOff,
}

/// One setting neet can change
#[derive(Debug, PartialEq, Eq)]
pub struct Setting {
    /// Such as `user.name`
    pub key: &'static str,
    /// What it does, in a few words
    pub about: &'static str,
    pub kind: Kind,
}

/// A program, the files its settings live in, and the settings neet knows
#[derive(Debug, PartialEq, Eq)]
pub struct Program {
    pub name: &'static str,
    /// Paths from the home folder
    pub files: &'static [&'static str],
    pub settings: &'static [Setting],
}

const fn setting(key: &'static str, about: &'static str, kind: Kind) -> Setting {
    Setting { key, about, kind }
}

/// Every program Configure knows. Keep in step with the table in SAFETY.md.
pub const PROGRAMS: &[Program] = &[Program {
    name: "Git",
    files: &[".gitconfig", ".config/git/config"],
    settings: &[
        setting("user.name", "Name on your commits.", Kind::Text),
        setting("user.email", "Email on your commits.", Kind::Text),
        setting(
            "init.defaultBranch",
            "Branch name for new repositories.",
            Kind::Text,
        ),
        setting("core.editor", "Editor for commit messages.", Kind::Text),
        setting(
            "pull.rebase",
            "Rebase instead of merge when pulling.",
            Kind::OnOff,
        ),
        setting(
            "push.autoSetupRemote",
            "Push a new branch without naming the remote.",
            Kind::OnOff,
        ),
    ],
}];

/// The program whose settings live in `path`, a path from the home folder
#[must_use]
pub fn program_for(path: &str) -> Option<&'static Program> {
    PROGRAMS
        .iter()
        .find(|program| program.files.contains(&path))
}

fn git(file: &Path, args: &[&str]) -> Result<crate::run::Captured, String> {
    let mut command = Command::new("git");
    command
        .args(["config", "--file"])
        .arg(file)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1");
    match crate::run::capture(&mut command, GIT_TIMEOUT) {
        Ok(Some(captured)) => Ok(captured),
        Ok(None) => Err(format!(
            "git did not finish within {} seconds.",
            GIT_TIMEOUT.as_secs()
        )),
        Err(error) => Err(format!("git could not run: {error}.")),
    }
}

/// The value of each of `program`'s settings in `file`, in the same order,
/// or `None` when it is not set. Only that file is read, not the files it
/// includes.
///
/// # Errors
///
/// Returns Git's message when the file cannot be read.
pub fn values(program: &Program, file: &Path) -> Result<Vec<Option<String>>, String> {
    let captured = git(file, &["--list", "--null"])?;
    if !captured.success {
        return Err(format!("git could not read it: {}", captured.stderr.trim()));
    }
    // Each entry is the key, then a new line and the value. A key on its own
    // is an on setting. Git writes section and setting names in lowercase.
    let mut found: Vec<(String, String)> = Vec::new();
    for entry in captured
        .stdout
        .split('\0')
        .filter(|entry| !entry.is_empty())
    {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, "true"));
        found.push((key.to_ascii_lowercase(), value.to_string()));
    }
    Ok(program
        .settings
        .iter()
        .map(|setting| {
            let key = setting.key.to_ascii_lowercase();
            // The last one wins, as in Git.
            found
                .iter()
                .rev()
                .find(|(found, _)| *found == key)
                .map(|(_, value)| value.clone())
        })
        .collect())
}

/// Sets `setting` in `file` to `value`, or removes it for `None`. Every
/// other line, comment, and their order stay as they are.
///
/// # Errors
///
/// Returns why the value was refused, or Git's message.
pub fn set(file: &Path, setting: &Setting, value: Option<&str>) -> Result<(), String> {
    let captured = if let Some(value) = value {
        if value.contains(['\n', '\r', '\0']) {
            return Err("A value cannot have more than one line.".to_string());
        }
        if setting.kind == Kind::OnOff && value != "true" && value != "false" {
            return Err(format!("{} is on or off.", setting.key));
        }
        git(file, &["--", setting.key, value])?
    } else {
        let captured = git(file, &["--unset-all", "--", setting.key])?;
        // Git says 5, with no message, when there was nothing to remove.
        if !captured.success && captured.stderr.trim().is_empty() {
            return Ok(());
        }
        captured
    };
    if captured.success {
        Ok(())
    } else {
        Err(format!(
            "git could not change {}: {}",
            setting.key,
            captured.stderr.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const GIT: &Program = &PROGRAMS[0];

    fn setting(key: &str) -> &'static Setting {
        GIT.settings
            .iter()
            .find(|setting| setting.key == key)
            .unwrap()
    }

    #[test]
    fn finds_the_program_for_a_file() {
        assert_eq!(program_for(".gitconfig").map(|p| p.name), Some("Git"));
        assert_eq!(
            program_for(".config/git/config").map(|p| p.name),
            Some("Git")
        );
        assert_eq!(program_for(".zshrc"), None);
    }

    #[test]
    fn reads_values_from_only_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        fs::write(&other, "[core]\n\teditor = vim\n").unwrap();
        let file = dir.path().join(".gitconfig");
        fs::write(
            &file,
            format!(
                "# mine\n[user]\n\tname = You\n\tname = Later You\n[Init]\n\tdefaultBranch = main\n[pull]\n\trebase\n[include]\n\tpath = {}\n",
                other.display()
            ),
        )
        .unwrap();

        let values = values(GIT, &file).unwrap();

        assert_eq!(
            values,
            [
                Some("Later You".to_string()),
                None,
                Some("main".to_string()),
                None,
                Some("true".to_string()),
                None,
            ]
        );
    }

    #[test]
    fn sets_and_removes_values_and_keeps_comments() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".gitconfig");
        fs::write(&file, "# mine\n[user]\n\tname = You # me\n").unwrap();

        set(&file, setting("user.email"), Some("you@example.com")).unwrap();
        set(&file, setting("pull.rebase"), Some("false")).unwrap();
        set(&file, setting("user.name"), None).unwrap();
        set(&file, setting("core.editor"), None).unwrap();

        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# mine\n[user]\n"), "{text}");
        assert!(text.contains("email = you@example.com"), "{text}");
        assert!(!text.contains("name = You"), "{text}");
        let values = values(GIT, &file).unwrap();
        assert_eq!(values[1].as_deref(), Some("you@example.com"));
        assert_eq!(values[4].as_deref(), Some("false"));
    }

    #[test]
    fn refuses_values_git_would_misread() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".gitconfig");
        fs::write(&file, "").unwrap();
        assert!(set(&file, setting("user.name"), Some("a\nb")).is_err());
        assert!(set(&file, setting("pull.rebase"), Some("yes")).is_err());
        set(&file, setting("user.name"), Some("--global")).unwrap();
        assert_eq!(values(GIT, &file).unwrap()[0].as_deref(), Some("--global"));
    }
}
