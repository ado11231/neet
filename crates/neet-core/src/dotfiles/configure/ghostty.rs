//! Reading and changing settings in Ghostty's config, one line at a time.
//!
//! Ghostty reads each line as a key, `=`, and a value, with spaces around
//! `=` ignored. Lines that start with `#` are comments, and there are no
//! comments after a value. A value in double quotes has them taken off, and
//! an empty value puts the setting back to Ghostty's default. The last line
//! for a key wins.

use std::ops::Range;

use super::{Program, Setting};

/// A line that sets `key`, and where its value is in the line. The value may
/// be empty, and keeps its quotes.
fn found(line: &str, key: &str) -> Option<Range<usize>> {
    let bare = line.trim_end_matches(['\n', '\r']);
    let start = bare.len() - bare.trim_start().len();
    let rest = &bare[start..];
    if rest.starts_with('#') {
        return None;
    }
    let (name, after) = rest.split_once('=')?;
    if name.trim() != key {
        return None;
    }
    let value_start = start + name.len() + 1 + (after.len() - after.trim_start().len());
    let value_end = (start + rest.trim_end().len()).max(value_start);
    Some(value_start..value_end)
}

/// `raw` as Ghostty reads it: without its quotes, or `None` when empty
fn unquote(raw: &str) -> Option<String> {
    let value = match raw.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => inner,
        None => raw,
    };
    (!value.is_empty()).then(|| value.to_string())
}

fn quoted(raw: &str) -> bool {
    raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"')
}

/// Each of `program`'s settings in `text`
pub(super) fn values(program: &Program, text: &str) -> Vec<Option<String>> {
    program
        .settings
        .iter()
        .map(|setting| {
            text.lines()
                .rev()
                .find_map(|line| found(line, setting.key).map(|span| unquote(&line[span])))
                .flatten()
        })
        .collect()
}

/// `value` as it is written, in quotes when `quote` is set or Ghostty would
/// otherwise read it differently
fn written(value: &str, quote: bool) -> Result<String, String> {
    let needs = quote || value.is_empty() || value.trim() != value || quoted(value);
    if !needs {
        return Ok(value.to_string());
    }
    if value.contains('"') {
        return Err(
            "Ghostty cannot read this value in quotes. Press e to edit the file.".to_string(),
        );
    }
    Ok(format!("\"{value}\""))
}

/// `text` with `setting` changed to `value`, or removed for `None`.
///
/// The value of the last line for the key is replaced, so the line keeps
/// its spacing, and its quotes. With no line, one is added at the end.
/// Removing takes out every line for the key.
///
/// # Errors
///
/// Returns why the value cannot be written.
pub(super) fn set(text: &str, setting: &Setting, value: Option<&str>) -> Result<String, String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some(value) = value else {
        return Ok(lines
            .into_iter()
            .filter(|line| found(line, setting.key).is_none())
            .collect());
    };
    let last = lines
        .iter()
        .rposition(|line| found(line, setting.key).is_some());
    let mut out = String::with_capacity(text.len() + 32);
    for (index, line) in lines.iter().enumerate() {
        match found(line, setting.key) {
            Some(span) if Some(index) == last => {
                let new = written(value, quoted(&line[span.clone()]))?;
                out.push_str(&line[..span.start]);
                // `key =` with nothing after it gets a space before the value.
                if span.is_empty() && line[..span.start].ends_with('=') {
                    out.push(' ');
                }
                out.push_str(&new);
                out.push_str(&line[span.end..]);
            }
            _ => out.push_str(line),
        }
    }
    if last.is_none() {
        let new = written(value, false)?;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(setting.key);
        out.push_str(" = ");
        out.push_str(&new);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::PROGRAMS;
    use super::*;

    fn ghostty() -> &'static Program {
        PROGRAMS.iter().find(|p| p.name == "Ghostty").unwrap()
    }

    fn setting(key: &str) -> &'static Setting {
        ghostty().settings.iter().find(|s| s.key == key).unwrap()
    }

    fn value(text: &str, key: &str) -> Option<String> {
        let index = ghostty()
            .settings
            .iter()
            .position(|s| s.key == key)
            .unwrap();
        values(ghostty(), text)[index].clone()
    }

    const CONFIG: &str = "# ~/.config/ghostty/config
# font-size = 99

theme = \"Catppuccin Mocha\"
font-size=13
  window-padding-x = 4,8
cursor-style =
";

    #[test]
    fn reads_values_without_quotes_and_skips_comments_and_empty_ones() {
        assert_eq!(value(CONFIG, "theme").as_deref(), Some("Catppuccin Mocha"));
        assert_eq!(value(CONFIG, "font-size").as_deref(), Some("13"));
        assert_eq!(value(CONFIG, "window-padding-x").as_deref(), Some("4,8"));
        assert_eq!(value(CONFIG, "cursor-style"), None);
        assert_eq!(value(CONFIG, "background-opacity"), None);
        assert_eq!(
            value("font-size = 12\nfont-size = 14\n", "font-size").as_deref(),
            Some("14")
        );
        assert_eq!(value("font-size-x = 12\n", "font-size"), None);
        assert_eq!(value("font-size = 12\nfont-size =\n", "font-size"), None);
    }

    #[test]
    fn changes_only_the_value_and_keeps_spacing_and_quotes() {
        assert_eq!(
            set(CONFIG, setting("font-size"), Some("14.5")).unwrap(),
            CONFIG.replace("font-size=13", "font-size=14.5")
        );
        assert_eq!(
            set(CONFIG, setting("theme"), Some("nord")).unwrap(),
            CONFIG.replace("\"Catppuccin Mocha\"", "\"nord\"")
        );
        assert_eq!(
            set(CONFIG, setting("cursor-style"), Some("bar")).unwrap(),
            CONFIG.replace("cursor-style =\n", "cursor-style = bar\n")
        );
        assert_eq!(
            set("theme=\n", setting("theme"), Some("nord")).unwrap(),
            "theme= nord\n"
        );
    }

    #[test]
    fn adds_a_missing_key_at_the_end_and_removes_every_line() {
        assert_eq!(
            set(CONFIG, setting("background-opacity"), Some("0.9")).unwrap(),
            format!("{CONFIG}background-opacity = 0.9\n")
        );
        assert_eq!(
            set("font-size = 12", setting("theme"), Some(" spaced ")).unwrap(),
            "font-size = 12\ntheme = \" spaced \"\n"
        );
        assert_eq!(
            set(
                "font-size = 12\n# keep\nfont-size = 14\n",
                setting("font-size"),
                None
            )
            .unwrap(),
            "# keep\n"
        );
    }

    #[test]
    fn refuses_a_quote_inside_a_quoted_value() {
        assert!(set(CONFIG, setting("theme"), Some("a\"b")).is_err());
        assert_eq!(
            set("", setting("theme"), Some("a\"b")).unwrap(),
            "theme = a\"b\n"
        );
    }
}
