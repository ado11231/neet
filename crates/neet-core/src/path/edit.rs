//! Reviewed PATH changes: taking a folder out of the shell line that adds
//! it, or moving it to the front or back of PATH from neet's block in
//! `~/.zshrc`. Only plain lines that set PATH, such as
//! `export PATH="$HOME/bin:$PATH"`, are changed. Lines that run a tool,
//! `/etc/paths`, and anything neet cannot fully read are refused. Files go
//! through the same check, backup, and chezmoi steps as a dotfile edit, so
//! Dotfiles can restore them.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use super::{Entry as Folder, Listing, Origin, SHELL_FILES, code_lines, names_folder, spellings};
use crate::dotfiles::change::{self, Checked, Done, Edit};
use crate::dotfiles::configure::zsh;
use crate::rewrite::Backups;

/// The files zsh reads. neet's block is in `.zshrc`, so folders added in
/// another shell's files may not move there.
const ZSH_FILES: [&str; 4] = [".zshenv", ".zprofile", ".zshrc", ".zlogin"];

/// zsh files a folder may not move from either. Scripts and editors read
/// `.zshenv`, and login shells read `.zprofile`, without `.zshrc`, so the
/// folder would quietly go away for them.
const SKIP_ZSHRC: [&str; 2] = [".zshenv", ".zprofile"];

/// Where a moved folder goes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The front of PATH, so its programs run before others
    First,
    /// The back of PATH
    Last,
}

/// One change, by the folder's position in the listing
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Remove(usize),
    Move(usize, Place),
}

impl Change {
    fn index(self) -> usize {
        match self {
            Self::Remove(index) | Self::Move(index, _) => index,
        }
    }
}

/// One shell file's text before and after the changes, for the diff
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// Its path from the home folder, such as `.zshrc`
    pub name: &'static str,
    pub before: String,
    pub after: String,
}

/// The shell files' texts, each read once through `read`, by their place
/// in [`SHELL_FILES`]
struct Files<R> {
    read: R,
    texts: BTreeMap<usize, Result<String, String>>,
}

impl<R: FnMut(&'static str) -> Result<String, String>> Files<R> {
    fn text(&mut self, file: usize) -> Result<&str, String> {
        let read = &mut self.read;
        self.texts
            .entry(file)
            .or_insert_with(|| read(SHELL_FILES[file]))
            .as_deref()
            .map_err(|e| format!("~/{}: {e}", SHELL_FILES[file]))
    }
}

/// A line that only sets PATH, in pieces
struct Line<'a> {
    /// Indentation, `export `, and `PATH=`
    lead: &'a str,
    /// `"`, or nothing
    quote: &'a str,
    /// The folders and `$PATH`, between the `:`s
    parts: Vec<&'a str>,
    /// Spaces, a comment, and the line break after the value
    rest: &'a str,
}

/// Plans `changes` to the folders in `listing`, read against `home`.
/// `read` gives a shell file's text by its path from the home folder.
///
/// # Errors
///
/// Returns why a change cannot be made, in plain words. Nothing is planned
/// then.
pub fn plan(
    listing: &Listing,
    changes: &[Change],
    home: &Path,
    read: impl FnMut(&'static str) -> Result<String, String>,
) -> Result<Vec<Planned>, String> {
    let mut files = Files {
        read,
        texts: BTreeMap::new(),
    };
    // The folders to take out of each line, by file and line number
    let mut out: BTreeMap<(usize, usize), Vec<&str>> = BTreeMap::new();
    for (number, &change) in changes.iter().enumerate() {
        let index = change.index();
        let entry = listing
            .entries
            .get(index)
            .ok_or("A folder to change is no longer in PATH. Open Shell PATH again.")?;
        if changes[..number].iter().any(|c| c.index() == index) {
            return Err(format!("{}: It is changed twice.", entry.folder));
        }
        let source = source(entry, home, change, changes, listing, &mut files)
            .map_err(|reason| format!("{}: {reason}", entry.folder))?;
        // Copies of one folder from the same line are taken out once.
        let folders = out.entry(source).or_default();
        if !folders.contains(&entry.folder.as_str()) {
            folders.push(&entry.folder);
        }
    }

    let mut texts: BTreeMap<usize, (String, Vec<String>)> = BTreeMap::new();
    let mut open = |file: usize| -> Result<(String, Vec<String>), String> {
        let text = files.text(file)?.to_owned();
        let lines = text.split_inclusive('\n').map(str::to_owned).collect();
        Ok((text, lines))
    };
    // How each moved folder was spelled, so `$HOME` stays `$HOME`
    let mut spelled: BTreeMap<&str, String> = BTreeMap::new();
    for (&(file, number), folders) in &out {
        let (_, lines) = match texts.entry(file) {
            Entry::Vacant(vacant) => vacant.insert(open(file)?),
            Entry::Occupied(occupied) => occupied.into_mut(),
        };
        let new = take_out(lines, number - 1, folders, home, &mut spelled)
            .map_err(|reason| format!("~/{}, line {number}, {reason}", SHELL_FILES[file]))?;
        lines[number - 1] = new;
    }

    // Moved folders go in neet's block, at the front or back of PATH. Two
    // copies of one folder go in once.
    let moved = |to: Place| -> Vec<&str> {
        let mut folders = Vec::new();
        for change in changes {
            if let Change::Move(index, place) = *change {
                let folder = spelled[listing.entries[index].folder.as_str()].as_str();
                if place == to && !folders.contains(&folder) {
                    folders.push(folder);
                }
            }
        }
        folders
    };
    let (front, back) = (moved(Place::First), moved(Place::Last));
    let mut add = String::new();
    if !front.is_empty() {
        let _ = writeln!(add, "export PATH=\"{}:$PATH\"", front.join(":"));
    }
    if !back.is_empty() {
        let _ = writeln!(add, "export PATH=\"$PATH:{}\"", back.join(":"));
    }
    let zshrc = SHELL_FILES
        .iter()
        .position(|&name| name == ".zshrc")
        .unwrap_or_default();
    if let (false, Entry::Vacant(vacant)) = (add.is_empty(), texts.entry(zshrc)) {
        vacant.insert(open(zshrc)?);
    }

    let mut planned = Vec::new();
    for (file, (before, lines)) in texts {
        let mut after = lines.concat();
        if file == zshrc && !add.is_empty() {
            after = zsh::append(&after, &add).map_err(|e| format!("~/.zshrc: {e}"))?;
        }
        if after != before {
            planned.push(Planned {
                name: SHELL_FILES[file],
                before,
                after,
            });
        }
    }
    Ok(planned)
}

/// The file, by its place in [`SHELL_FILES`], and line number `change` edits
fn source<R: FnMut(&'static str) -> Result<String, String>>(
    entry: &Folder,
    home: &Path,
    change: Change,
    changes: &[Change],
    listing: &Listing,
    files: &mut Files<R>,
) -> Result<(usize, usize), String> {
    let (file, line) = match &entry.origin {
        Origin::Line { file, line } => (file, *line),
        Origin::Tool { tool, .. } => {
            return Err(format!(
                "{tool} adds it when your shell starts. Change it with {tool}."
            ));
        }
        Origin::System(file) => {
            return Err(format!(
                "It comes from {}, which neet never changes.",
                file.display()
            ));
        }
        Origin::Unknown => {
            return Err("No shell file adds it, so neet cannot tell where to change it.".into());
        }
    };
    let index = SHELL_FILES
        .iter()
        .position(|name| home.join(name) == *file)
        .ok_or("It is added outside the shell files neet may change.")?;
    let name = SHELL_FILES[index];
    if matches!(change, Change::Move(..)) && !ZSH_FILES.contains(&name) {
        return Err(format!(
            "It is added in ~/{name}, and neet's block is in ~/.zshrc, which only zsh reads."
        ));
    }
    if matches!(change, Change::Move(..)) && SKIP_ZSHRC.contains(&name) {
        return Err(format!(
            "It is added in ~/{name}, which shells that skip ~/.zshrc also read, and neet's block is in ~/.zshrc."
        ));
    }
    // Every copy of a folder points at the first line naming it. Taking the
    // folder out of that line must not take away a copy that stays.
    let removed = |i: usize| changes.contains(&Change::Remove(i));
    let kept =
        listing.entries.iter().enumerate().find(|(i, other)| {
            other.folder == entry.folder && !removed(*i) && *i != change.index()
        });
    if let (Change::Remove(_), Some((other, _))) = (change, kept) {
        let names = spellings(&entry.folder, home);
        let mut lines = 0;
        for file in 0..SHELL_FILES.len() {
            // A file that cannot be read adds no copy neet can count on.
            if let Ok(text) = files.text(file) {
                lines += code_lines(text)
                    .iter()
                    .filter(|line| names.iter().any(|name| names_folder(line, name)))
                    .count();
            }
        }
        if lines < 2 {
            return Err(format!(
                "The same line adds the copy at #{}, so taking it out would remove both.",
                other + 1
            ));
        }
    }
    Ok((index, line))
}

/// Line `index` of `lines` without `folders`, empty when only `$PATH` is
/// left. Each folder's spelling is kept in `spelled`.
fn take_out<'a>(
    lines: &[String],
    index: usize,
    folders: &[&'a str],
    home: &Path,
    spelled: &mut BTreeMap<&'a str, String>,
) -> Result<String, String> {
    let leave = "Change it in the editor.";
    let text = lines.get(index).ok_or("is gone. Open Shell PATH again.")?;
    if !at_top(&lines[..index]) {
        return Err(format!("is inside an if, a loop, or a function. {leave}"));
    }
    let line = parse(text, home)
        .ok_or_else(|| format!("does more than set PATH to plain folders. {leave}"))?;
    let bare = line.quote.is_empty();
    let mut parts = line.parts.clone();
    for &folder in folders {
        let names = |part: &str| full_path(part, home, bare).as_deref() == Some(folder);
        let Some(part) = parts.iter().find(|part| names(part)) else {
            return Err(format!("no longer adds {folder}. Open Shell PATH again."));
        };
        // `~` is not filled in inside quotes, which neet's block uses.
        spelled.insert(folder, part.replacen('~', "$HOME", 1));
        parts.retain(|part| !names(part));
    }
    if parts.is_empty() {
        return Err("would leave PATH empty.".to_string());
    }
    if parts.iter().all(|part| is_path(part)) {
        return Ok(String::new());
    }
    Ok(format!(
        "{}{}{}{}{}",
        line.lead,
        line.quote,
        parts.join(":"),
        line.quote,
        line.rest
    ))
}

/// `line` when it only sets PATH to full folders and `$PATH`, such as
/// `export PATH="$HOME/bin:$PATH"` or `PATH=~/bin:$PATH`, with an optional
/// comment. `$(...)`, `&&`, `;`, other variables, and quotes inside are
/// `None`.
fn parse<'a>(line: &'a str, home: &Path) -> Option<Line<'a>> {
    let body = line.trim_end_matches(['\n', '\r']);
    let code = [" #", "\t#"]
        .iter()
        .filter_map(|mark| body.find(mark))
        .min()
        .map_or(body, |end| &body[..end])
        .trim_end();
    let start = &code[code.len() - code.trim_start().len()..];
    let value = start
        .strip_prefix("export ")
        .map_or(start, str::trim_start)
        .strip_prefix("PATH=")?;
    let (quote, inner) = match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => ("\"", inner),
        None => ("", value),
    };
    let parts: Vec<&str> = inner.split(':').collect();
    let plain = parts
        .iter()
        .all(|part| is_path(part) || full_path(part, home, quote.is_empty()).is_some());
    plain.then(|| Line {
        lead: &code[..code.len() - value.len()],
        quote,
        parts,
        rest: &line[code.len()..],
    })
}

fn is_path(part: &str) -> bool {
    matches!(part, "$PATH" | "${PATH}")
}

/// The full folder `part` names, with the home folder for `$HOME`, when it
/// is a plain full path. `~` counts only when `bare`, outside quotes.
fn full_path(part: &str, home: &Path, bare: bool) -> Option<String> {
    let rest = ["$HOME", "${HOME}"]
        .into_iter()
        .chain(bare.then_some("~"))
        .find_map(|prefix| part.strip_prefix(prefix));
    let (start, rest) = match rest {
        Some(rest) => (home.to_string_lossy().into_owned(), rest),
        None => (String::new(), part),
    };
    let plain = rest.starts_with('/')
        && rest
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._+@-".contains(&b));
    plain.then(|| format!("{start}{rest}"))
}

/// Whether a line after `above` is at the top of the file, not inside an
/// `if`, a loop, a function, or brackets, and not continuing the line
/// before. Words and brackets are counted roughly, so any doubt is a no.
fn at_top(above: &[String]) -> bool {
    let mut depth = 0i32;
    let mut continued = false;
    for line in above {
        let code = line
            .split_once(" #")
            .map_or(line.as_str(), |(code, _)| code);
        let code = if code.trim_start().starts_with('#') {
            ""
        } else {
            code.trim_end()
        };
        continued = code.ends_with('\\');
        for word in code.split(|c: char| c.is_whitespace() || c == ';') {
            match word {
                "if" | "for" | "while" | "until" | "case" | "select" | "repeat" => depth += 1,
                "fi" | "done" | "esac" => depth -= 1,
                _ => {}
            }
        }
        for c in code.chars() {
            match c {
                '{' | '(' => depth += 1,
                '}' | ')' => depth -= 1,
                _ => {}
            }
        }
    }
    depth == 0 && !continued
}

/// A planned change to one shell file, opened as Dotfiles opens it, so it
/// is written only if the file has not changed since
pub struct Prepared {
    pub planned: Planned,
    edit: Edit,
}

/// Opens the shell files `changes` need, as Dotfiles does, and plans them.
///
/// # Errors
///
/// Returns why a change cannot be made, in plain words.
pub fn prepare(
    listing: &Listing,
    changes: &[Change],
    home: &Path,
) -> Result<Vec<Prepared>, String> {
    let dotfiles = crate::dotfiles::list(home);
    let mut edits: Vec<(&str, Edit)> = Vec::new();
    let planned = plan(listing, changes, home, |name| {
        let file = dotfiles
            .files
            .iter()
            .find(|file| file.known.path == name)
            .ok_or("It is not a file neet may change.")?;
        let edit = Edit::begin(file, dotfiles.chezmoi.as_ref())?;
        let text = String::from_utf8(edit.contents().to_vec())
            .map_err(|_| "It is not plain text.".to_string())?;
        edits.push((name, edit));
        Ok(text)
    })?;
    Ok(planned
        .into_iter()
        .filter_map(|planned| {
            let at = edits.iter().position(|(name, _)| *name == planned.name)?;
            Some(Prepared {
                edit: edits.swap_remove(at).1,
                planned,
            })
        })
        .collect())
}

/// Checks each file's new text, then writes each one after a backup in
/// `backups`, as a dotfile edit does. A failed check writes nothing. If a
/// later file cannot be written, the ones before it stay written, each
/// with its backup.
///
/// # Errors
///
/// Returns why a file was not written, in plain words.
pub fn write(
    prepared: &[Prepared],
    home: &Path,
    backups: &Backups,
    now: SystemTime,
) -> Result<Vec<Done>, String> {
    for Prepared { planned, edit } in prepared {
        let name = planned.name;
        let copy = edit
            .copy(home)
            .map_err(|e| format!("~/{name}: A copy could not be written: {e}."))?;
        let checked =
            fs::write(&copy, &planned.after).map(|()| change::check(edit.syntax(), &copy, name));
        change::discard(&copy);
        match checked {
            Ok(Checked::Failed(error)) => {
                return Err(format!(
                    "The check of ~/{name} failed. Nothing was written. {error}"
                ));
            }
            Err(e) => return Err(format!("~/{name}: A copy could not be written: {e}.")),
            Ok(Checked::Passed | Checked::NotChecked(_)) => {}
        }
    }
    let mut done = Vec::new();
    for Prepared { planned, edit } in prepared {
        let result = edit
            .finish(planned.after.as_bytes(), backups, now)
            .map_err(|error| format!("~/{}: {error}", planned.name))?;
        done.push(result);
    }
    Ok(done)
}

#[cfg(test)]
mod tests;
