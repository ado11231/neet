//! Reading and changing zsh settings, only inside a block neet owns.
//!
//! The block starts with [`START`] and ends with [`END`], at the end of the
//! file, so its settings come after, and win over, the lines above it.
//! Lines outside it are never read or changed. Inside it, each setting is one
//! line, in one of three shapes, picked by the setting's command:
//!
//! - `export`: `export EDITOR='nano'`
//! - `set`: `HISTSIZE=50000`
//! - `setopt`: `setopt auto_cd` for on, `unsetopt auto_cd` for off

use std::fmt::Write as _;
use std::ops::Range;

use super::{Program, Setting};

pub const START: &str = "# >>> neet >>>";
pub const END: &str = "# <<< neet <<<";
const ABOUT: &str = "# Set by neet's Configure. neet changes only the lines between these markers.";

/// Why a file's block cannot be used
const BROKEN: &str =
    "The neet block in this file is not whole. Press e to edit the file, and fix its marker lines.";

/// The lines of `text`, each with its line break
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn is(line: &str, marker: &str) -> bool {
    line.trim() == marker
}

/// Which lines are the block, markers included, or `None` without one
fn block(lines: &[&str]) -> Result<Option<Range<usize>>, String> {
    let starts: Vec<usize> = (0..lines.len()).filter(|&i| is(lines[i], START)).collect();
    let ends: Vec<usize> = (0..lines.len()).filter(|&i| is(lines[i], END)).collect();
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([start], [end]) if start < end => Ok(Some(*start..end + 1)),
        _ => Err(BROKEN.to_string()),
    }
}

/// `setting`'s value on `line`, when the line sets it
fn read(line: &str, setting: &Setting) -> Option<String> {
    let line = line.trim();
    if setting.command == "setopt" {
        let (command, name) = line.split_once(char::is_whitespace)?;
        if name.trim() != setting.key {
            return None;
        }
        return match command {
            "setopt" => Some("on".to_string()),
            "unsetopt" => Some("off".to_string()),
            _ => None,
        };
    }
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let raw = line.strip_prefix(setting.key)?.strip_prefix('=')?;
    let value = raw
        .strip_prefix('\'')
        .and_then(|inner| inner.strip_suffix('\''))
        .unwrap_or(raw);
    Some(value.to_string())
}

/// The line that sets `setting` to `value`
fn line_for(setting: &Setting, value: &str) -> Result<String, String> {
    match setting.command {
        "setopt" if value == "off" => Ok(format!("unsetopt {}\n", setting.key)),
        "setopt" => Ok(format!("setopt {}\n", setting.key)),
        "export" if value.contains('\'') => Err(format!(
            "{} cannot have a ' in it here. Press e to edit the file.",
            setting.key
        )),
        "export" => Ok(format!("export {}='{value}'\n", setting.key)),
        _ => Ok(format!("{}={value}\n", setting.key)),
    }
}

/// Each of `program`'s settings in the block of `text`
///
/// # Errors
///
/// Returns why the block cannot be read.
pub(super) fn values(program: &Program, text: &str) -> Result<Vec<Option<String>>, String> {
    let lines = lines(text);
    let inside = match block(&lines)? {
        Some(range) => &lines[range],
        None => &[][..],
    };
    Ok(program
        .settings
        .iter()
        .map(|setting| inside.iter().rev().find_map(|line| read(line, setting)))
        .collect())
}

/// `text` with `setting` changed to `value` in the block, or removed for
/// `None`. Without a block, one is added at the end. A block left with no
/// settings is taken out.
///
/// # Errors
///
/// Returns why the value or the block cannot be used.
pub(super) fn set(text: &str, setting: &Setting, value: Option<&str>) -> Result<String, String> {
    let new = value.map(|value| line_for(setting, value)).transpose()?;
    let lines = lines(text);
    let Some(range) = block(&lines)? else {
        let Some(new) = new else {
            return Ok(text.to_string());
        };
        let mut out = text.to_string();
        if !out.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        let _ = write!(out, "{START}\n{ABOUT}\n{new}{END}\n");
        return Ok(out);
    };

    let inner = range.start + 1..range.end - 1;
    let last = inner
        .clone()
        .rev()
        .find(|&i| read(lines[i], setting).is_some());
    let mut body: Vec<String> = Vec::new();
    for i in inner.clone() {
        if read(lines[i], setting).is_none() {
            body.push(lines[i].to_string());
        } else if Some(i) == last {
            body.extend(new.clone());
        }
    }
    if last.is_none() {
        body.extend(new);
    }

    let holds_settings = body.iter().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#')
    });
    let mut out: String = lines[..range.start].concat();
    if holds_settings {
        out.push_str(lines[range.start]);
        for line in &body {
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(lines[inner.end]);
    } else if out.ends_with("\n\n") {
        // The empty line neet put before the block goes with it.
        out.pop();
    }
    out.push_str(&lines[range.end..].concat());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::PROGRAMS;
    use super::*;

    fn zsh() -> &'static Program {
        PROGRAMS.iter().find(|p| p.name == "zsh").unwrap()
    }

    fn setting(key: &str) -> &'static Setting {
        zsh().settings.iter().find(|s| s.key == key).unwrap()
    }

    fn value(text: &str, key: &str) -> Option<String> {
        let index = zsh().settings.iter().position(|s| s.key == key).unwrap();
        values(zsh(), text).unwrap()[index].clone()
    }

    const MINE: &str = "# my zshrc\nexport EDITOR=vim\nsetopt auto_cd\n";

    #[test]
    fn reads_only_inside_the_block() {
        assert_eq!(value(MINE, "EDITOR"), None);
        assert_eq!(value(MINE, "auto_cd"), None);
        let text = format!(
            "{MINE}\n{START}\nexport EDITOR='nano'\nHISTSIZE=5000\nunsetopt auto_cd\n{END}\n# after\n"
        );
        assert_eq!(value(&text, "EDITOR").as_deref(), Some("nano"));
        assert_eq!(value(&text, "HISTSIZE").as_deref(), Some("5000"));
        assert_eq!(value(&text, "auto_cd").as_deref(), Some("off"));
        assert_eq!(value(&text, "SAVEHIST"), None);
    }

    #[test]
    fn adds_a_block_at_the_end_and_leaves_every_other_line() {
        let out = set(MINE, setting("EDITOR"), Some("nano")).unwrap();
        assert_eq!(
            out,
            format!("{MINE}\n{START}\n{ABOUT}\nexport EDITOR='nano'\n{END}\n")
        );
        let out = set(&out, setting("auto_cd"), Some("off")).unwrap();
        assert!(out.starts_with(MINE));
        assert!(out.ends_with("export EDITOR='nano'\nunsetopt auto_cd\n# <<< neet <<<\n"));
        assert_eq!(
            set("x=1", setting("HISTSIZE"), Some("10")).unwrap(),
            format!("x=1\n\n{START}\n{ABOUT}\nHISTSIZE=10\n{END}\n")
        );
    }

    #[test]
    fn changes_a_line_in_place_and_drops_an_empty_block() {
        let start = set(MINE, setting("EDITOR"), Some("nano")).unwrap();
        let two = set(&start, setting("HISTSIZE"), Some("10")).unwrap();
        let changed = set(&two, setting("EDITOR"), Some("code --wait")).unwrap();
        assert_eq!(
            changed,
            two.replace("export EDITOR='nano'", "export EDITOR='code --wait'")
        );
        let one = set(&changed, setting("EDITOR"), None).unwrap();
        assert_eq!(one, start.replace("export EDITOR='nano'", "HISTSIZE=10"));
        assert_eq!(set(&one, setting("HISTSIZE"), None).unwrap(), MINE);
        assert_eq!(set(MINE, setting("HISTSIZE"), None).unwrap(), MINE);
    }

    #[test]
    fn refuses_a_broken_block_and_a_quote_in_an_export() {
        for broken in [
            format!("{START}\n"),
            format!("{END}\n{START}\n"),
            format!("{START}\n{END}\n{START}\n{END}\n"),
        ] {
            assert!(values(zsh(), &broken).is_err(), "{broken}");
            assert!(set(&broken, setting("EDITOR"), Some("nano")).is_err());
        }
        assert!(set(MINE, setting("EDITOR"), Some("it's")).is_err());
    }
}
