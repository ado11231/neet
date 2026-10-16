//! Shell PATH: the folders your shell searches for programs, in order,
//! with problems flagged, where each was added, and which copy of a program
//! runs. neet starts from your terminal, so its own `PATH` is your shell's.
//! Looking never changes a file and never runs your shell.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Shell files, from the home folder, searched for where a folder is added
pub const SHELL_FILES: [&str; 8] = [
    ".zshenv",
    ".zprofile",
    ".zshrc",
    ".zlogin",
    ".bash_profile",
    ".bashrc",
    ".profile",
    ".config/fish/config.fish",
];

/// The most of a shell file read, to find where folders are added
const MAX_FILE: u64 = 1024 * 1024;

/// What is wrong with a PATH folder
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// It does not exist
    Missing,
    /// It is a file, not a folder
    NotFolder,
    /// The same folder comes earlier, at this position
    Repeated(usize),
    /// Not a full path, so it depends on the folder you are in
    Relative,
    /// Other users can add programs to it
    WritableByOthers,
}

impl Problem {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Missing => "missing".into(),
            Self::NotFolder => "not a folder".into(),
            Self::Repeated(first) => format!("repeat of #{}", first + 1),
            Self::Relative => "relative".into(),
            Self::WritableByOthers => "writable by others".into(),
        }
    }

    /// Why it matters, in a sentence
    #[must_use]
    pub fn about(&self) -> &'static str {
        match self {
            Self::Missing => {
                "The folder does not exist. The shell skips it, so it only slows lookups a little. A tool may have been removed."
            }
            Self::NotFolder => "This is a file, not a folder, so the shell finds nothing in it.",
            Self::Repeated(_) => "The same folder is searched twice. Only the first one matters.",
            Self::Relative => {
                "This is not a full path, so what it finds depends on the folder you are in. A program in the current folder could run in place of the real one."
            }
            Self::WritableByOthers => {
                "Other users on this Mac can put programs here, which then run in place of yours."
            }
        }
    }
}

/// Where a folder was added to PATH
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A line in a shell file that names it
    Line { file: PathBuf, line: usize },
    /// A line in a shell file that runs a tool, which adds it
    Tool {
        tool: &'static str,
        file: PathBuf,
        line: usize,
    },
    /// `/etc/paths` or a file in `/etc/paths.d`, read by macOS's
    /// `path_helper` for every user
    System(PathBuf),
    /// No shell file names it. Another program added it, such as an
    /// editor's terminal.
    Unknown,
}

/// One folder in PATH
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub folder: String,
    pub problems: Vec<Problem>,
    /// Programs in it that can run
    pub programs: usize,
    pub origin: Origin,
}

/// A program found in more than one PATH folder
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shadowed {
    pub name: String,
    /// Each folder's position in PATH, in order. The first one runs.
    pub entries: Vec<usize>,
}

/// Everything the PATH tab shows
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Programs in more than one folder, by name
    pub shadowed: Vec<Shadowed>,
}

impl Listing {
    /// Every copy of `name` in PATH order. The first is the one that runs.
    #[must_use]
    pub fn which(&self, name: &str) -> Vec<PathBuf> {
        if name.is_empty() || name.contains('/') {
            return Vec::new();
        }
        self.entries
            .iter()
            .filter(|entry| {
                entry.problems.iter().all(|p| {
                    !matches!(
                        p,
                        Problem::Missing | Problem::NotFolder | Problem::Repeated(_)
                    )
                })
            })
            .map(|entry| Path::new(&entry.folder).join(name))
            .filter(|path| is_program(path))
            .collect()
    }
}

/// Whether `path` is a file that can run
fn is_program(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The programs in `folder`
fn programs(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| is_program(&entry.path()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect()
}

/// Reads `path_var`, split on `:`, against the shell files in `home` and
/// the system lists in `etc`.
#[must_use]
pub fn read(path_var: &str, home: &Path, etc: &Path) -> Listing {
    let files = shell_lines(home);
    let system = system_lists(etc);
    let mut seen: HashMap<PathBuf, usize> = HashMap::new();
    let mut found: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut entries = Vec::new();
    for (index, folder) in path_var.split(':').enumerate() {
        let mut problems = Vec::new();
        let path = Path::new(folder);
        if !path.is_absolute() {
            problems.push(Problem::Relative);
        }
        let meta = fs::metadata(path);
        match &meta {
            Err(_) => problems.push(Problem::Missing),
            Ok(meta) if !meta.is_dir() => problems.push(Problem::NotFolder),
            Ok(meta) => {
                // Writable by group or others, and not a sticky folder like /tmp
                if meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0 {
                    problems.push(Problem::WritableByOthers);
                }
            }
        }
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut count = 0;
        if let Some(&first) = seen.get(&canonical) {
            problems.push(Problem::Repeated(first));
        } else {
            seen.insert(canonical, index);
            if meta.is_ok_and(|meta| meta.is_dir()) {
                let names = programs(path);
                count = names.len();
                for name in names {
                    found.entry(name).or_default().push(index);
                }
            }
        }
        entries.push(Entry {
            folder: folder.to_string(),
            problems,
            programs: count,
            origin: origin(folder, home, &files, &system),
        });
    }
    let mut shadowed: Vec<Shadowed> = found
        .into_iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(name, entries)| Shadowed { name, entries })
        .collect();
    // Programs whose copy comes from your own folders first, then by name
    shadowed.sort_by(|a, b| {
        a.entries[0]
            .cmp(&b.entries[0])
            .then_with(|| a.name.cmp(&b.name))
    });
    Listing { entries, shadowed }
}

/// Reads `PATH` from neet's own environment, which it got from your shell.
#[must_use]
pub fn from_env(home: &Path) -> Listing {
    read(
        &std::env::var("PATH").unwrap_or_default(),
        home,
        Path::new("/etc"),
    )
}

/// Each shell file's lines, without comments, for the files that exist
fn shell_lines(home: &Path) -> Vec<(PathBuf, Vec<String>)> {
    SHELL_FILES
        .iter()
        .map(|name| home.join(name))
        .filter_map(|path| {
            let meta = fs::metadata(&path).ok()?;
            if !meta.is_file() || meta.len() > MAX_FILE {
                return None;
            }
            let text = fs::read_to_string(&path).ok()?;
            let lines = text
                .lines()
                .map(|line| line.split_once(" #").map_or(line, |(code, _)| code))
                .map(|line| {
                    if line.trim_start().starts_with('#') {
                        String::new()
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            Some((path, lines))
        })
        .collect()
}

/// `/etc/paths` and each file in `/etc/paths.d`, with their folders
fn system_lists(etc: &Path) -> Vec<(PathBuf, Vec<String>)> {
    let mut files = vec![etc.join("paths")];
    if let Ok(entries) = fs::read_dir(etc.join("paths.d")) {
        let mut more: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        more.sort();
        files.extend(more);
    }
    files
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let folders = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_owned)
                .collect();
            Some((path, folders))
        })
        .collect()
}

/// Tools that add folders when a shell file runs them: the folder they
/// add, as a part of its path, and the text that runs them
const TOOLS: [(&str, &str, &str); 8] = [
    ("mise", "/mise/", "mise activate"),
    ("Homebrew", "/opt/homebrew/", "brew shellenv"),
    ("Cargo", "/.cargo/bin", ".cargo/env"),
    ("pyenv", "/.pyenv/", "pyenv init"),
    ("rbenv", "/.rbenv/", "rbenv init"),
    ("nvm", "/.nvm/", "nvm.sh"),
    ("conda", "conda", "conda initialize"),
    ("asdf", "/.asdf/", "asdf.sh"),
];

/// Where `folder` was added: a shell line naming it, a line running a tool
/// that adds it, a system list, or unknown
fn origin(
    folder: &str,
    home: &Path,
    files: &[(PathBuf, Vec<String>)],
    system: &[(PathBuf, Vec<String>)],
) -> Origin {
    let home_text = home.to_string_lossy();
    // The ways a shell file can spell this folder
    let mut spellings = vec![folder.to_string()];
    if let Some(rest) = folder.strip_prefix(home_text.as_ref()) {
        for prefix in ["$HOME", "${HOME}", "~"] {
            spellings.push(format!("{prefix}{rest}"));
        }
    }
    for (file, lines) in files {
        for (number, line) in lines.iter().enumerate() {
            if spellings
                .iter()
                .any(|spelling| names_folder(line, spelling))
            {
                return Origin::Line {
                    file: file.clone(),
                    line: number + 1,
                };
            }
        }
    }
    for (tool, part, runs) in TOOLS {
        if !folder.contains(part) {
            continue;
        }
        for (file, lines) in files {
            if let Some(number) = lines.iter().position(|line| line.contains(runs)) {
                return Origin::Tool {
                    tool,
                    file: file.clone(),
                    line: number + 1,
                };
            }
        }
    }
    for (file, folders) in system {
        if folders.iter().any(|listed| listed == folder) {
            return Origin::System(file.clone());
        }
    }
    Origin::Unknown
}

/// Whether `line` names `folder` as a whole path, not as the start of a
/// longer one
fn names_folder(line: &str, folder: &str) -> bool {
    line.match_indices(folder).any(|(start, _)| {
        let after = line[start + folder.len()..].chars().next();
        let before = line[..start].chars().next_back();
        !matches!(after, Some(c) if c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
            && !matches!(before, Some(c) if c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn program(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let etc = root.join("etc");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(etc.join("paths.d")).unwrap();
        (dir, home, etc)
    }

    #[test]
    fn flags_missing_repeated_relative_files_and_open_folders() {
        let (_dir, home, etc) = setup();
        let bin = home.join("bin");
        program(&bin.join("tool"));
        let open = home.join("open");
        fs::create_dir_all(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
        let file = home.join("file");
        fs::write(&file, "x").unwrap();
        let path = format!(
            "{}:{}:relative/bin:{}:{}:{}",
            bin.display(),
            home.join("gone").display(),
            bin.display(),
            file.display(),
            open.display()
        );

        let listing = read(&path, &home, &etc);

        let problems: Vec<_> = listing.entries.iter().map(|e| e.problems.clone()).collect();
        assert_eq!(problems[0], []);
        assert_eq!(listing.entries[0].programs, 1);
        assert_eq!(problems[1], [Problem::Missing]);
        assert!(problems[2].contains(&Problem::Relative));
        assert_eq!(problems[3], [Problem::Repeated(0)]);
        assert_eq!(problems[4], [Problem::NotFolder]);
        assert_eq!(problems[5], [Problem::WritableByOthers]);
    }

    #[test]
    fn finds_which_copy_runs_and_programs_in_several_folders() {
        let (_dir, home, etc) = setup();
        let mine = home.join("bin");
        let system = home.join("usr/bin");
        program(&mine.join("python3"));
        program(&system.join("python3"));
        program(&system.join("ls"));
        fs::write(system.join("notes.txt"), "not a program").unwrap();
        let path = format!("{}:{}", mine.display(), system.display());

        let listing = read(&path, &home, &etc);

        assert_eq!(
            listing.which("python3"),
            [mine.join("python3"), system.join("python3")]
        );
        assert_eq!(listing.which("ls"), [system.join("ls")]);
        assert_eq!(listing.which("notes.txt"), Vec::<PathBuf>::new());
        assert_eq!(listing.which("../bin/ls"), Vec::<PathBuf>::new());
        assert_eq!(
            listing.shadowed,
            [Shadowed {
                name: "python3".into(),
                entries: vec![0, 1],
            }]
        );
    }

    #[test]
    fn finds_where_each_folder_was_added() {
        let (_dir, home, etc) = setup();
        fs::write(
            home.join(".zshrc"),
            "# PATH\n# path=($HOME/old $path)\ntypeset -U path\npath=(\n  $HOME/bin\n  $path\n)\neval \"$(mise activate zsh)\"\n",
        )
        .unwrap();
        fs::write(
            home.join(".zprofile"),
            "eval \"$(/opt/homebrew/bin/brew shellenv zsh)\"\n",
        )
        .unwrap();
        fs::write(etc.join("paths"), "/usr/local/bin\n/usr/bin\n").unwrap();
        fs::write(etc.join("paths.d/homebrew"), "/opt/homebrew/bin\n").unwrap();
        let listing = read(
            &format!(
                "{0}/bin:{0}/bin-extra:{0}/.local/share/mise/shims:/opt/homebrew/bin:/usr/bin:/Applications/Editor/bin:{0}/old",
                home.display()
            ),
            &home,
            &etc,
        );
        let origins: Vec<_> = listing.entries.iter().map(|e| e.origin.clone()).collect();

        assert_eq!(
            origins[0],
            Origin::Line {
                file: home.join(".zshrc"),
                line: 5
            }
        );
        // A longer folder that starts the same is not a match.
        assert_eq!(origins[1], Origin::Unknown);
        assert_eq!(
            origins[2],
            Origin::Tool {
                tool: "mise",
                file: home.join(".zshrc"),
                line: 8
            }
        );
        // brew shellenv adds Homebrew's folder, ahead of the system list.
        assert_eq!(
            origins[3],
            Origin::Tool {
                tool: "Homebrew",
                file: home.join(".zprofile"),
                line: 1
            }
        );
        assert_eq!(origins[4], Origin::System(etc.join("paths")));
        assert_eq!(origins[5], Origin::Unknown);
        // A commented line is not where it was added.
        assert_eq!(origins[6], Origin::Unknown);
    }

    #[test]
    fn an_empty_path_lists_one_relative_entry() {
        let (_dir, home, etc) = setup();
        let listing = read("", &home, &etc);
        assert_eq!(listing.entries.len(), 1);
        assert!(listing.entries[0].problems.contains(&Problem::Relative));
    }
}
