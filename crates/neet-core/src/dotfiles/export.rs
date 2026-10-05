//! Export: a review for secrets, then a commit and a push of chezmoi's
//! source files. See Export in `docs/SAFETY.md`.
//!
//! The review is not a promise that nothing secret remains.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::{Chezmoi, Listing, Managed};

/// How long a Git command may take. Pushing can be slow.
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

/// Something in a file that looks secret
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// From 1
    pub line: usize,
    /// What it looks like, such as `private key`
    pub kind: &'static str,
    /// Its first four characters, then `****`
    pub start: String,
}

/// Starts of tokens that services hand out, and what they are
const TOKENS: &[(&str, &str)] = &[
    ("ghp_", "GitHub token"),
    ("gho_", "GitHub token"),
    ("ghu_", "GitHub token"),
    ("ghs_", "GitHub token"),
    ("ghr_", "GitHub token"),
    ("github_pat_", "GitHub token"),
    ("glpat-", "GitLab token"),
    ("sk-", "API key"),
    ("sk_live_", "Stripe key"),
    ("xoxb-", "Slack token"),
    ("xoxp-", "Slack token"),
    ("xoxa-", "Slack token"),
    ("AKIA", "AWS key"),
    ("AIza", "Google key"),
    ("npm_", "npm token"),
];

/// Setting names that hold a secret when they have a value
const NAMES: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "_authtoken",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
];

/// Looks through `text` for anything that looks secret.
#[must_use]
pub fn scan(text: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let finding = |kind, value: &str| Finding {
            line: index + 1,
            kind,
            start: format!("{}****", value.chars().take(4).collect::<String>()),
        };
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            found.push(finding("private key", "----"));
            continue;
        }
        if let Some((kind, value)) = token(line) {
            found.push(finding(kind, value));
            continue;
        }
        if let Some(value) = named_secret(line) {
            found.push(finding("secret setting", value));
        }
    }
    found
}

/// A word in `line` that starts like a token and is long enough to be one
fn token(line: &str) -> Option<(&'static str, &str)> {
    line.split(|character: char| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
    })
    .find_map(|word| {
        TOKENS.iter().find_map(|(start, kind)| {
            (word.starts_with(start) && word.len() >= start.len() + 12).then_some((*kind, word))
        })
    })
}

/// The value of a setting whose name says it is secret, such as
/// `password = hunter2` or `//registry/:_authToken=...`
fn named_secret(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with("//#") {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    NAMES.iter().find_map(|name| {
        let at = lower.find(name)?;
        let rest = &line[at + name.len()..];
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
        let rest = rest.trim_start_matches([' ', '"', '\'']);
        let value = rest
            .strip_prefix('=')
            .or_else(|| rest.strip_prefix(':'))?
            .trim()
            .trim_matches(['"', '\'']);
        (!value.is_empty()).then_some(value)
    })
}

/// A source file that waits to be committed, with what the review found
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The dotfile's path from the home folder
    pub file: &'static str,
    /// The source file, from the repository's top folder
    pub path: String,
    pub findings: Vec<Finding>,
    /// Starts left out, such as `.npmrc`
    pub may_hold_secrets: bool,
}

/// The repository chezmoi's source folder is in, and the files that wait
/// to be committed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The repository's top folder
    pub top: PathBuf,
    pub pending: Vec<Pending>,
}

fn git(folder: &Path, args: &[&str]) -> Result<crate::run::Captured, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(folder)
        .args(["-c", "core.fsmonitor=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    match crate::run::capture(&mut command, GIT_TIMEOUT) {
        Ok(Some(captured)) => Ok(captured),
        Ok(None) => Err(format!(
            "git did not finish within {} seconds.",
            GIT_TIMEOUT.as_secs()
        )),
        Err(error) => Err(format!("git could not run: {error}.")),
    }
}

fn git_ok(folder: &Path, args: &[&str]) -> Result<String, String> {
    let captured = git(folder, args)?;
    if captured.success {
        Ok(captured.stdout)
    } else {
        Err(format!("git {}: {}", args[0], captured.stderr.trim()))
    }
}

/// Which listed dotfiles' source files have changes not yet committed, each
/// reviewed for secrets. Only the source files of listed dotfiles are ever
/// offered.
///
/// # Errors
///
/// Returns why, when chezmoi's folder is not in a Git repository or Git
/// cannot read it.
pub fn plan(listing: &Listing) -> Result<Plan, String> {
    let chezmoi: &Chezmoi = listing
        .chezmoi
        .as_ref()
        .ok_or("chezmoi's folder was not found.")?;
    let top = PathBuf::from(
        git_ok(&chezmoi.source, &["rev-parse", "--show-toplevel"])
            .map_err(|_| "chezmoi's folder is not in a Git repository.".to_string())?
            .trim(),
    );
    let top = std::fs::canonicalize(&top).unwrap_or(top);
    let status = git_ok(
        &top,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let changed: Vec<String> = status
        .split('\0')
        .filter(|entry| entry.len() > 3)
        .map(|entry| entry[3..].to_string())
        .collect();
    let mut pending = Vec::new();
    for file in &listing.files {
        let Some(source) = file.managed.as_ref().and_then(Managed::source) else {
            continue;
        };
        let source = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        let Ok(relative) = source.strip_prefix(&top) else {
            continue;
        };
        let path = relative.display().to_string();
        if !changed.contains(&path) {
            continue;
        }
        let text = std::fs::read(&source)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        pending.push(Pending {
            file: file.known.path,
            path,
            findings: scan(&text),
            may_hold_secrets: file.may_hold_secrets,
        });
    }
    Ok(Plan { top, pending })
}

/// Commits `paths`, from the repository's top folder, with `message`, and
/// nothing else, even if other changes are staged.
///
/// # Errors
///
/// Returns Git's message when it refuses.
pub fn commit(top: &Path, paths: &[&str], message: &str) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Nothing is selected to commit.".to_string());
    }
    if message.trim().is_empty() || message.contains('\0') {
        return Err("The commit needs a message.".to_string());
    }
    let mut add = vec!["add", "--"];
    add.extend(paths);
    git_ok(top, &add)?;
    let mut commit = vec!["commit", "--quiet", "-m", message, "--"];
    commit.extend(paths);
    git_ok(top, &commit)?;
    Ok(())
}

/// The remote and branch a push goes to, such as `origin/main`
///
/// # Errors
///
/// Returns why there is nowhere to push.
pub fn push_target(top: &Path) -> Result<String, String> {
    git_ok(
        top,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .map(|target| target.trim().to_string())
    .map_err(|_| {
        "The branch does not track a remote branch, so there is nowhere to push.".to_string()
    })
}

/// Pushes the branch to the remote branch it tracks. Never forces. If the
/// remote has commits this does not, Git refuses, and so does this.
///
/// # Errors
///
/// Returns why nothing was pushed, in plain words.
pub fn push(top: &Path) -> Result<(), String> {
    push_target(top)?;
    let captured = git(top, &["push", "--quiet"])?;
    if captured.success {
        return Ok(());
    }
    let message = captured.stderr.trim();
    if message.contains("rejected") || message.contains("fetch first") {
        return Err(
            "The remote has commits you do not have, so nothing was pushed. Pull first."
                .to_string(),
        );
    }
    Err(format!("git push failed: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_keys_tokens_and_secret_settings() {
        let text = "\
-----BEGIN OPENSSH PRIVATE KEY-----
export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789
//registry.npmjs.org/:_authToken=npm_secretsecretsecretsecret
password = hunter2
# password = in a comment
token =
export PATH=$HOME/bin:$PATH
alias sk-=ls
";
        let found = scan(text);
        let kinds: Vec<(usize, &str)> =
            found.iter().map(|found| (found.line, found.kind)).collect();
        assert_eq!(
            kinds,
            [
                (1, "private key"),
                (2, "GitHub token"),
                (3, "npm token"),
                (4, "secret setting"),
            ]
        );
        assert_eq!(found[1].start, "ghp_****");
        assert_eq!(found[3].start, "hunt****");
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// A home folder whose chezmoi source folder is a subfolder of a
    /// repository, as with `.chezmoiroot`
    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap().join("home");
        let repo = home.join(".local/share/chezmoi");
        let source = repo.join("mac");
        fs::create_dir_all(&source).unwrap();
        fs::write(repo.join(".chezmoiroot"), "mac\n").unwrap();
        fs::write(source.join("dot_zshrc"), "export A=1\n").unwrap();
        fs::write(source.join("dot_npmrc"), "x=1\n").unwrap();
        fs::write(home.join(".zshrc"), "export A=1\n").unwrap();
        fs::write(home.join(".npmrc"), "x=1\n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "first"]);
        (dir, home, repo)
    }

    #[test]
    fn plans_only_listed_source_files_with_changes() {
        let (_dir, home, repo) = setup();
        fs::write(repo.join("mac/dot_zshrc"), "export A=2\npassword=abc\n").unwrap();
        fs::write(repo.join("mac/dot_npmrc"), "x=2\n").unwrap();
        fs::write(repo.join("README.md"), "not a dotfile\n").unwrap();

        let plan = plan(&super::super::list(&home)).unwrap();

        assert_eq!(plan.top, repo);
        let files: Vec<(&str, &str, bool)> = plan
            .pending
            .iter()
            .map(|pending| {
                (
                    pending.file,
                    pending.path.as_str(),
                    pending.may_hold_secrets,
                )
            })
            .collect();
        assert_eq!(
            files,
            [
                (".zshrc", "mac/dot_zshrc", false),
                (".npmrc", "mac/dot_npmrc", true)
            ]
        );
        assert_eq!(plan.pending[0].findings[0].line, 2);
    }

    #[test]
    fn commits_only_the_chosen_files_and_pushes_without_force() {
        let (dir, _home, repo) = setup();
        let remote = dir.path().join("remote.git");
        git(
            dir.path(),
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                remote.to_str().unwrap(),
            ],
        );
        git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&repo, &["push", "-q", "-u", "origin", "main"]);
        fs::write(repo.join("mac/dot_zshrc"), "export A=2\n").unwrap();
        fs::write(repo.join("other"), "staged elsewhere\n").unwrap();
        git(&repo, &["add", "other"]);

        // Commits need a name; the test repository gets one.
        git(&repo, &["config", "user.name", "t"]);
        git(&repo, &["config", "user.email", "t@t"]);
        commit(&repo, &["mac/dot_zshrc"], "Update zshrc").unwrap();

        let shown = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["show", "--name-only", "--format=%s"])
            .output()
            .unwrap();
        let shown = String::from_utf8_lossy(&shown.stdout).into_owned();
        assert!(shown.starts_with("Update zshrc\n"), "{shown}");
        assert!(shown.contains("mac/dot_zshrc"));
        assert!(!shown.contains("other"));

        assert_eq!(push_target(&repo).unwrap(), "origin/main");
        push(&repo).unwrap();

        // Someone else pushes; this one must not force over it.
        let other = dir.path().join("other-clone");
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        git(&other, &["config", "user.name", "t"]);
        git(&other, &["config", "user.email", "t@t"]);
        fs::write(other.join("x"), "x\n").unwrap();
        git(&other, &["add", "x"]);
        git(&other, &["commit", "-q", "-m", "elsewhere"]);
        git(&other, &["push", "-q"]);
        fs::write(repo.join("mac/dot_zshrc"), "export A=3\n").unwrap();
        commit(&repo, &["mac/dot_zshrc"], "Again").unwrap();
        let error = push(&repo).unwrap_err();
        assert!(error.contains("Pull first"), "{error}");
    }

    #[test]
    fn refuses_empty_commits_and_messages() {
        let (_dir, _home, repo) = setup();
        assert!(commit(&repo, &[], "x").is_err());
        assert!(commit(&repo, &["mac/dot_zshrc"], "  ").is_err());
        assert!(push_target(&repo).is_err());
    }
}
