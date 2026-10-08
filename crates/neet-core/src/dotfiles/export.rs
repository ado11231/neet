//! Export: a review for secrets, then a commit and a push of chezmoi's
//! source files. Without chezmoi's folder, it starts one as a new Git
//! repository, in chezmoi's layout. See Export in `docs/SAFETY.md`.
//!
//! The review is not a promise that nothing secret remains.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::{Chezmoi, Listing, Managed, NewSource, ViewOnly};
use crate::rewrite::Opened;

/// How long a Git command may take. Pushing can be slow.
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

/// How long `gh repo create` may take, with its push
const GH_TIMEOUT: Duration = Duration::from_secs(120);

/// The name of the GitHub repository Export offers to create
pub const GITHUB_NAME: &str = "dotfiles";

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
#[derive(Debug, Clone)]
pub struct Plan {
    /// The repository's top folder
    pub top: PathBuf,
    pub pending: Vec<Pending>,
    /// For a new repository, the dotfiles themselves; otherwise their
    /// source files
    reviewed: Vec<(String, Opened)>,
    /// For a new repository, where each pending file goes
    new: Option<Vec<NewSource>>,
}

impl Plan {
    /// Whether the repository is still to be started
    #[must_use]
    pub fn is_new(&self) -> bool {
        self.new.is_some()
    }
}

fn git(folder: &Path, args: &[&str]) -> Result<crate::run::Captured, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(folder)
        .args(["-c", "core.fsmonitor=false"]);
    // A new repository has no name for its commits of its own, and the
    // machine running the tests may have none either.
    #[cfg(test)]
    command.args(["-c", "user.name=t", "-c", "user.email=t@t"]);
    command.args(args).env("GIT_TERMINAL_PROMPT", "0");
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
    let mut changed = Vec::new();
    let mut entries = status.split('\0');
    while let Some(entry) = entries.next() {
        if let Some(path) = entry.get(3..) {
            changed.push(path.to_string());
            // Renames and copies have a second, unprefixed path.
            if entry[..2].contains(['R', 'C']) {
                entries.next();
            }
        }
    }
    let mut pending = Vec::new();
    let mut reviewed = Vec::new();
    for file in &listing.files {
        let Some(source) = file.managed.as_ref().and_then(Managed::source) else {
            continue;
        };
        let real_source = std::fs::canonicalize(source)
            .map_err(|_| "A source file could not be read for the secret review.")?;
        if real_source != source {
            return Err("A source path contains a link. Review it outside neet.".to_string());
        }
        let Ok(relative) = source.strip_prefix(&top) else {
            continue;
        };
        let path = relative.display().to_string();
        if !changed.contains(&path) {
            continue;
        }
        let opened = Opened::open(source)
            .map_err(|_| format!("{path} could not be read for the secret review."))?;
        let text = String::from_utf8_lossy(opened.contents());
        let findings = scan(&text);
        reviewed.push((path.clone(), opened));
        pending.push(Pending {
            file: file.known.path,
            path,
            findings,
            may_hold_secrets: file.may_hold_secrets,
        });
    }
    Ok(Plan {
        top,
        pending,
        reviewed,
        new: None,
    })
}

/// Whether `folder` is missing, or an empty folder that is not a link
fn is_free(folder: &Path) -> Result<(), String> {
    let shown = folder.display();
    match std::fs::symlink_metadata(folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{shown} could not be read: {error}.")),
        Ok(metadata) if !metadata.is_dir() => {
            Err(format!("{shown} is there, and is not a folder."))
        }
        Ok(_) => match std::fs::read_dir(folder).map(|mut entries| entries.next().is_none()) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!("{shown} already has files in it.")),
            Err(error) => Err(format!("{shown} could not be read: {error}.")),
        },
    }
}

/// Every listed dotfile that exists, reviewed for secrets, to start
/// chezmoi's folder at `folder` as a new repository. Nothing is written.
///
/// # Errors
///
/// Returns why a repository cannot be started there.
pub fn start_plan(home: &Path, listing: &Listing, folder: &Path) -> Result<Plan, String> {
    if listing.chezmoi.is_some() {
        return Err("chezmoi's folder is already there.".to_string());
    }
    is_free(folder)?;
    let mut pending = Vec::new();
    let mut reviewed = Vec::new();
    let mut new = Vec::new();
    for file in &listing.files {
        if file.found.is_none()
            || matches!(
                file.view_only,
                Some(
                    ViewOnly::BrokenLink
                        | ViewOnly::LinkLeadsOut
                        | ViewOnly::Protected
                        | ViewOnly::NotAFile
                )
            )
        {
            continue;
        }
        let Some(source) = super::new_source(folder, home, file.known.path) else {
            continue;
        };
        let path = source
            .path
            .strip_prefix(folder)
            .map_err(|_| "A new source path is outside the folder.".to_string())?
            .display()
            .to_string();
        let opened = Opened::open(&file.path).map_err(|_| {
            format!(
                "~/{} could not be read for the secret review.",
                file.known.path
            )
        })?;
        let findings = scan(&String::from_utf8_lossy(opened.contents()));
        reviewed.push((path.clone(), opened));
        new.push(source);
        pending.push(Pending {
            file: file.known.path,
            path,
            findings,
            may_hold_secrets: file.may_hold_secrets,
        });
    }
    if pending.is_empty() {
        return Err("None of the listed dotfiles are in your home folder yet.".to_string());
    }
    Ok(Plan {
        top: folder.to_path_buf(),
        pending,
        reviewed,
        new: Some(new),
    })
}

/// The README a new repository starts with
fn readme(files: &[&str]) -> String {
    let mut text = String::from(
        "# Dotfiles\n\nSettings files, kept in [chezmoi](https://www.chezmoi.io)'s layout. \
         Set up another Mac from this repository with:\n\n```sh\nchezmoi init --apply <this repository's address>\n```\n\n## Files\n\n",
    );
    for file in files {
        let _ = writeln!(text, "- `~/{file}`");
    }
    text
}

/// Makes `path` with `mode`, failing if it is already there
fn create(path: &Path, contents: &[u8], mode: u32) -> Result<(), String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .map_err(|error| format!("{} could not be made: {error}.", path.display()))?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("{} could not be written: {error}.", path.display()))
}

fn make_folder(path: &Path, private: bool) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt as _;
    if path.is_dir() {
        return Ok(());
    }
    std::fs::DirBuilder::new()
        .mode(if private { 0o700 } else { 0o755 })
        .create(path)
        .map_err(|error| format!("{} could not be made: {error}.", path.display()))
}

/// Starts the new repository: writes the chosen files as they were
/// reviewed, a README, and a `.chezmoiignore` that leaves the README out,
/// then runs `git init` and commits them.
fn start(plan: &Plan, new: &[NewSource], paths: &[&str], message: &str) -> Result<(), String> {
    let who = git(Path::new("/"), &["var", "GIT_AUTHOR_IDENT"])?;
    if !who.success {
        return Err(
            "Git does not know your name and email yet. Set user.name and user.email with c on .gitconfig, then export again."
                .to_string(),
        );
    }
    let top = &plan.top;
    is_free(top)?;
    let mut chosen = Vec::new();
    for path in paths {
        let index = plan
            .reviewed
            .iter()
            .position(|(name, _)| name == path)
            .ok_or("A selected file was not reviewed. Open Export again.")?;
        let opened = &plan.reviewed[index].1;
        if !opened.is_unchanged().unwrap_or(false) {
            return Err(format!(
                "~/{} changed since the secret review. Open Export again.",
                plan.pending[index].file
            ));
        }
        chosen.push(index);
    }
    if let Some(parent) = top.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} could not be made: {error}.", parent.display()))?;
    }
    make_folder(top, true)?;
    for &index in &chosen {
        let source = &new[index];
        for (folder, private) in &source.folders {
            make_folder(folder, *private)?;
        }
        let opened = &plan.reviewed[index].1;
        create(&source.path, opened.contents(), opened.mode() & 0o777)?;
    }
    let files: Vec<&str> = chosen.iter().map(|&i| plan.pending[i].file).collect();
    create(&top.join("README.md"), readme(&files).as_bytes(), 0o644)?;
    create(&top.join(".chezmoiignore"), b"README.md\n", 0o644)?;
    git_ok(top, &["init", "--quiet", "--initial-branch=main"])?;
    let mut add = vec!["add", "--", "README.md", ".chezmoiignore"];
    add.extend(paths);
    git_ok(top, &add)?;
    git_ok(top, &["commit", "--quiet", "-m", message])?;
    Ok(())
}

/// Creates a private GitHub repository named [`GITHUB_NAME`] with `gh`,
/// as the remote `origin`, and pushes to it. Returns its address.
///
/// # Errors
///
/// Returns why, such as `gh` not being logged in or the name being taken.
pub fn create_github(top: &Path) -> Result<String, String> {
    if !git_ok(top, &["remote"])?.trim().is_empty() {
        return Err("The repository already has a remote. Push to it instead.".to_string());
    }
    let mut command = Command::new("gh");
    command
        .args(["repo", "create", GITHUB_NAME, "--private", "--source"])
        .arg(top)
        .args(["--remote", "origin", "--push"])
        .current_dir(top)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    let captured = match crate::run::capture(&mut command, GH_TIMEOUT) {
        Ok(Some(captured)) => captured,
        Ok(None) => {
            return Err(format!(
                "gh did not finish within {} seconds.",
                GH_TIMEOUT.as_secs()
            ));
        }
        Err(error) => return Err(format!("gh could not run: {error}.")),
    };
    if !captured.success {
        return Err(format!("gh repo create failed: {}", captured.stderr.trim()));
    }
    Ok(captured
        .stdout
        .lines()
        .find(|line| line.starts_with("https://"))
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// Commits `paths`, from the repository's top folder, with `message`, and
/// nothing else, even if other changes are staged.
///
/// # Errors
///
/// Returns Git's message when it refuses.
pub fn commit(plan: &Plan, paths: &[&str], message: &str) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Nothing is selected to commit.".to_string());
    }
    if message.trim().is_empty() || message.contains('\0') {
        return Err("The commit needs a message.".to_string());
    }
    if let Some(new) = &plan.new {
        return start(plan, new, paths, message);
    }
    for path in paths {
        let opened = plan
            .reviewed
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, opened)| opened)
            .ok_or("A selected file was not reviewed. Open Export again.")?;
        if !opened.is_unchanged().unwrap_or(false) {
            return Err(format!(
                "{path} changed since the secret review. Open Export again."
            ));
        }
    }
    let top = &plan.top;
    let mut add = vec!["add", "--"];
    add.extend(paths);
    git_ok(top, &add)?;
    let mut commit = vec!["commit", "--quiet", "-m", message, "--"];
    commit.extend(paths);
    git_ok(top, &commit)?;
    Ok(())
}

/// The exact destination and commit shown before a push
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    pub label: String,
    remote: String,
    branch: String,
    url: String,
    head: String,
}

/// The remote branch tracked by the current branch, with its push address.
///
/// # Errors
///
/// Returns why there is no single remote branch to push to.
pub fn push_target(top: &Path) -> Result<PushTarget, String> {
    let branch = git_ok(top, &["symbolic-ref", "--quiet", "HEAD"])?;
    let upstream = git_ok(
        top,
        &[
            "for-each-ref",
            "--format=%(upstream:short)%00%(upstream:remotename)%00%(upstream:remoteref)",
            branch.trim(),
        ],
    )?;
    let parts: Vec<&str> = upstream.trim().split('\0').collect();
    let [label, remote, branch] = parts.as_slice() else {
        return Err("The branch does not track a remote branch.".to_string());
    };
    if remote.is_empty() || *remote == "." || !branch.starts_with("refs/heads/") {
        return Err(
            "The branch does not track a remote branch, so there is nowhere to push.".to_string(),
        );
    }
    let urls = git_ok(top, &["remote", "get-url", "--push", "--all", remote])?;
    let urls: Vec<&str> = urls.lines().collect();
    let [url] = urls.as_slice() else {
        return Err("The remote has several push addresses. Push it yourself.".to_string());
    };
    Ok(PushTarget {
        label: (*label).to_string(),
        remote: (*remote).to_string(),
        branch: (*branch).to_string(),
        url: (*url).to_string(),
        head: git_ok(top, &["rev-parse", "HEAD"])?.trim().to_string(),
    })
}

/// Pushes the branch to the remote branch it tracks. Never forces. If the
/// remote has commits this does not, Git refuses, and so does this.
///
/// # Errors
///
/// Returns why nothing was pushed, in plain words.
pub fn push(top: &Path, target: &PushTarget) -> Result<(), String> {
    if push_target(top)? != *target {
        return Err("The branch or push destination changed. Open Export again.".to_string());
    }
    // An explicit address and ref ignore pushDefault, mirror, and extra refspecs.
    let reference = format!("{}:{}", target.head, target.branch);
    let captured = git(
        top,
        &[
            "push",
            "--quiet",
            "--no-follow-tags",
            "--",
            &target.url,
            &reference,
        ],
    )?;
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
    fn starts_a_repository_in_chezmoi_layout_with_only_the_chosen_files() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap().join("home");
        fs::create_dir_all(home.join(".ssh")).unwrap();
        fs::set_permissions(home.join(".ssh"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(home.join(".zshrc"), "export A=1\n").unwrap();
        fs::write(home.join(".ssh/config"), "Host x\n").unwrap();
        fs::set_permissions(home.join(".ssh/config"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(
            home.join(".npmrc"),
            "//r/:_authToken=npm_abcdefabcdefabcdef\n",
        )
        .unwrap();
        fs::write(dir.path().join("outside"), "x\n").unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside"), home.join(".vimrc")).unwrap();
        let folder = home.join(".local/share/chezmoi");
        let listing = super::super::list(&home);
        assert!(listing.chezmoi.is_none());

        let plan = start_plan(&home, &listing, &folder).unwrap();
        assert!(plan.is_new());
        assert!(!folder.exists(), "the plan writes nothing");
        let files: Vec<(&str, &str, usize)> = plan
            .pending
            .iter()
            .map(|p| (p.file, p.path.as_str(), p.findings.len()))
            .collect();
        assert_eq!(
            files,
            [
                (".zshrc", "dot_zshrc", 0),
                (".ssh/config", "private_dot_ssh/private_config", 0),
                (".npmrc", "dot_npmrc", 1),
            ]
        );

        commit(
            &plan,
            &["dot_zshrc", "private_dot_ssh/private_config"],
            "Start dotfiles",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(folder.join("dot_zshrc")).unwrap(),
            "export A=1\n"
        );
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&folder.join("private_dot_ssh")), 0o700);
        assert_eq!(mode(&folder.join("private_dot_ssh/private_config")), 0o600);
        assert!(!folder.join("dot_npmrc").exists());
        assert_eq!(
            fs::read_to_string(folder.join(".chezmoiignore")).unwrap(),
            "README.md\n"
        );
        assert!(
            fs::read_to_string(folder.join("README.md"))
                .unwrap()
                .contains("- `~/.ssh/config`")
        );
        let shown = Command::new("git")
            .arg("-C")
            .arg(&folder)
            .args(["show", "--name-only", "--format=%s"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&shown.stdout),
            "Start dotfiles\n\n.chezmoiignore\nREADME.md\ndot_zshrc\nprivate_dot_ssh/private_config\n"
        );

        // The folder now holds files, so starting again is refused, and so
        // is a plan made before the folder was filled.
        assert!(start_plan(&home, &listing, &folder).is_err());
        assert!(commit(&plan, &["dot_zshrc"], "Again").is_err());
        // chezmoi's folder is found now, so the screen treats it as chezmoi's.
        assert!(super::super::list(&home).chezmoi.is_some());
    }

    #[test]
    fn a_new_repository_refuses_a_file_changed_after_the_review() {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        fs::write(home.join(".zshrc"), "export A=1\n").unwrap();
        let folder = home.join("dots");
        let plan = start_plan(&home, &super::super::list(&home), &folder).unwrap();
        fs::write(home.join(".zshrc"), "export A=2\n").unwrap();
        let error = commit(&plan, &["dot_zshrc"], "Start").unwrap_err();
        assert!(error.contains("changed since the secret review"), "{error}");
        assert!(!folder.exists());
    }

    #[test]
    fn commits_only_the_chosen_files_and_pushes_without_force() {
        let (dir, home, repo) = setup();
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
        commit(
            &plan(&super::super::list(&home)).unwrap(),
            &["mac/dot_zshrc"],
            "Update zshrc",
        )
        .unwrap();

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

        assert_eq!(push_target(&repo).unwrap().label, "origin/main");
        git(&repo, &["config", "remote.pushDefault", "elsewhere"]);
        git(&repo, &["config", "branch.main.pushRemote", "elsewhere"]);
        git(
            &repo,
            &["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"],
        );
        git(&repo, &["config", "remote.origin.mirror", "true"]);
        git(&repo, &["branch", "private-work"]);
        push(&repo, &push_target(&repo).unwrap()).unwrap();
        let refs = git_ok(&remote, &["for-each-ref", "--format=%(refname)"]).unwrap();
        assert_eq!(refs.trim(), "refs/heads/main");

        let reviewed = push_target(&repo).unwrap();
        git(
            &repo,
            &["config", "remote.origin.pushurl", "/changed-destination"],
        );
        assert!(
            push(&repo, &reviewed)
                .unwrap_err()
                .contains("destination changed")
        );
        git(&repo, &["config", "--unset", "remote.origin.pushurl"]);

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
        commit(
            &plan(&super::super::list(&home)).unwrap(),
            &["mac/dot_zshrc"],
            "Again",
        )
        .unwrap();
        let error = push(&repo, &push_target(&repo).unwrap()).unwrap_err();
        assert!(error.contains("Pull first"), "{error}");
    }

    #[test]
    fn refuses_changes_after_the_secret_review_and_unreviewed_paths() {
        let (_dir, home, repo) = setup();
        let source = repo.join("mac/dot_zshrc");
        fs::write(&source, "export A=2\n").unwrap();
        let reviewed = plan(&super::super::list(&home)).unwrap();
        fs::write(&source, "export TOKEN=secret\n").unwrap();
        let error = commit(&reviewed, &["mac/dot_zshrc"], "Update").unwrap_err();
        assert!(error.contains("changed since the secret review"), "{error}");
        assert!(commit(&reviewed, &["."], "Update").is_err());
        assert_eq!(
            git_ok(&repo, &["diff", "--cached", "--name-only"]).unwrap(),
            ""
        );
    }

    #[test]
    fn renamed_unicode_paths_do_not_break_the_review() {
        let (_dir, home, repo) = setup();
        fs::write(repo.join("古い"), "old\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "Add a file"]);
        git(&repo, &["mv", "古い", "new"]);
        fs::write(repo.join("mac/dot_zshrc"), "export A=2\n").unwrap();
        let reviewed = plan(&super::super::list(&home)).unwrap();
        assert_eq!(reviewed.pending.len(), 1);
        assert_eq!(reviewed.pending[0].file, ".zshrc");
    }

    #[test]
    fn refuses_empty_commits_and_messages() {
        let (_dir, home, repo) = setup();
        assert!(commit(&plan(&super::super::list(&home)).unwrap(), &[], "x").is_err());
        assert!(
            commit(
                &plan(&super::super::list(&home)).unwrap(),
                &["mac/dot_zshrc"],
                "  "
            )
            .is_err()
        );
        assert!(push_target(&repo).is_err());
    }
}
