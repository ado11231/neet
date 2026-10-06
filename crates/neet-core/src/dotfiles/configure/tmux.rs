//! Reading and changing settings in a tmux config, one line at a time.
//!
//! Only plain lines count: `set`, `set-option`, `setw`, or
//! `set-window-option`, with `-g` or `-s`, then the option and one value.
//! Lines inside `%if` blocks, lines continued with `\`, lines with more than
//! one command, and lines that append (`-a`) or name a target (`-t`) are
//! left alone, as is every other line.

use std::ops::Range;

use super::{Program, Setting};

/// One plain `set` line for an option
struct Found {
    /// The option, such as `mouse`
    key: String,
    /// The value, without quotes, or `None` for a line that unsets it
    value: Option<String>,
    /// Where the value is in the line, quotes included
    span: Option<Range<usize>>,
}

/// A word on a line and where it is, quotes included
struct Word {
    text: String,
    span: Range<usize>,
    quoted: bool,
}

/// Splits a line into words, as tmux does, up to a `#` that starts a word.
/// Returns `None` for a line neet does not read: one with an unclosed quote.
fn words(line: &str) -> Option<Vec<Word>> {
    let mut words = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '#' {
            break;
        }
        let mut text = String::new();
        let mut quoted = false;
        let mut end = start;
        while let Some(&(at, c)) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            chars.next();
            end = at + c.len_utf8();
            match c {
                '\'' => {
                    quoted = true;
                    let mut closed = false;
                    for (at, c) in chars.by_ref() {
                        end = at + c.len_utf8();
                        if c == '\'' {
                            closed = true;
                            break;
                        }
                        text.push(c);
                    }
                    if !closed {
                        return None;
                    }
                }
                '"' => {
                    quoted = true;
                    let mut closed = false;
                    while let Some((at, c)) = chars.next() {
                        end = at + c.len_utf8();
                        match c {
                            '"' => {
                                closed = true;
                                break;
                            }
                            '\\' => {
                                if let Some((at, escaped)) = chars.next() {
                                    end = at + escaped.len_utf8();
                                    text.push(escaped);
                                }
                            }
                            _ => text.push(c),
                        }
                    }
                    if !closed {
                        return None;
                    }
                }
                _ => text.push(c),
            }
        }
        words.push(Word {
            text,
            span: start..end,
            quoted,
        });
    }
    Some(words)
}

/// The option a plain `set` line sets, or `None` for any other line
fn found(line: &str) -> Option<Found> {
    let words = words(line)?;
    let (command, rest) = words.split_first()?;
    if command.quoted
        || !matches!(
            command.text.as_str(),
            "set" | "set-option" | "setw" | "set-window-option"
        )
    {
        return None;
    }
    // A `;` runs another command on the same line.
    if words
        .iter()
        .any(|word| !word.quoted && word.text.ends_with(';'))
    {
        return None;
    }
    let flags: String = rest
        .iter()
        .take_while(|word| !word.quoted && word.text.starts_with('-'))
        .flat_map(|word| word.text.chars().skip(1))
        .collect();
    if !flags
        .chars()
        .all(|flag| matches!(flag, 'g' | 's' | 'w' | 'q' | 'u'))
        || !flags.contains(['g', 's'])
    {
        return None;
    }
    let flag_words = rest
        .iter()
        .take_while(|word| !word.quoted && word.text.starts_with('-'))
        .count();
    let mut rest = rest[flag_words..].iter();
    let key = rest.next()?;
    let value = rest.next();
    if rest.next().is_some() {
        return None;
    }
    if flags.contains('u') {
        return value.is_none().then(|| Found {
            key: key.text.clone(),
            value: None,
            span: None,
        });
    }
    let value = value?;
    Some(Found {
        key: key.text.clone(),
        value: Some(value.text.clone()),
        span: Some(value.span.clone()),
    })
}

/// Each line, and the option it sets when it is a plain `set` line neet
/// reads
fn lines(text: &str) -> Vec<(&str, Option<Found>)> {
    let mut depth = 0_usize;
    let mut continued = false;
    text.split_inclusive('\n')
        .map(|line| {
            let bare = line.trim_end_matches(['\n', '\r']);
            let was_continued = continued;
            let trailing = bare.len() - bare.trim_end_matches('\\').len();
            continued = trailing % 2 == 1;
            let directive = bare.trim_start();
            if directive.starts_with("%if") {
                depth += 1;
            } else if directive.starts_with("%endif") {
                depth = depth.saturating_sub(1);
            }
            let read = depth == 0 && !was_continued && !continued && !directive.starts_with('%');
            (line, if read { found(bare) } else { None })
        })
        .collect()
}

/// Each of `program`'s settings in `text`. The last line for an option wins,
/// as in tmux.
pub(super) fn values(program: &Program, text: &str) -> Vec<Option<String>> {
    let found: Vec<Found> = lines(text)
        .into_iter()
        .filter_map(|(_, found)| found)
        .collect();
    program
        .settings
        .iter()
        .map(|setting| {
            found
                .iter()
                .rev()
                .find(|found| found.key == setting.key)
                .and_then(|found| found.value.clone())
        })
        .collect()
}

/// `value` written so tmux reads it back as it is
fn quote(value: &str) -> Result<String, String> {
    let plain = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._,:/+@%=-".contains(c));
    if plain {
        Ok(value.to_string())
    } else if !value.contains('\'') {
        Ok(format!("'{value}'"))
    } else {
        Err("A tmux value with ' in it is changed by editing the file, with e.".to_string())
    }
}

/// A new line for `setting`, such as `set -g mouse on`
fn added(setting: &Setting, value: &str) -> String {
    format!("{} {} {value}", setting.command, setting.key)
}

/// `text` with `setting` changed to `value`, or removed for `None`.
///
/// The value of the last line for the option is replaced, so the line keeps
/// its spacing and any comment after it. With no line, one is added at the
/// end. Removing takes out every line for the option.
pub(super) fn set(text: &str, setting: &Setting, value: Option<&str>) -> Result<String, String> {
    let lines = lines(text);
    let ours = |found: &Option<Found>| found.as_ref().is_some_and(|f| f.key == setting.key);
    let Some(value) = value else {
        return Ok(lines
            .iter()
            .filter(|(_, found)| !ours(found))
            .map(|(line, _)| *line)
            .collect());
    };
    let value = quote(value)?;
    let last = lines.iter().rposition(|(_, found)| ours(found));
    let mut out = String::with_capacity(text.len() + 32);
    for (index, (line, found)) in lines.iter().enumerate() {
        if Some(index) != last {
            out.push_str(line);
            continue;
        }
        if let Some(span) = found.as_ref().and_then(|found| found.span.clone()) {
            out.push_str(&line[..span.start]);
            out.push_str(&value);
            out.push_str(&line[span.end..]);
        } else {
            // A line that unsets it becomes one that sets it.
            let indent = &line[..line.len() - line.trim_start().len()];
            let ending = &line[line.trim_end_matches(['\n', '\r']).len()..];
            out.push_str(indent);
            out.push_str(&added(setting, &value));
            out.push_str(ending);
        }
    }
    if last.is_none() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&added(setting, &value));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::PROGRAMS;
    use super::*;

    const TMUX: &Program = &PROGRAMS[1];

    fn setting(key: &str) -> &'static Setting {
        TMUX.settings.iter().find(|s| s.key == key).unwrap()
    }

    fn value(text: &str, key: &str) -> Option<String> {
        let index = TMUX.settings.iter().position(|s| s.key == key).unwrap();
        values(TMUX, text)[index].clone()
    }

    const CONFIG: &str = r#"# ~/.config/tmux/tmux.conf
set  -g default-terminal "tmux-256color"
set -ga terminal-features ",xterm-kitty:RGB:extkeys"
set -sg escape-time 10
set  -g mouse on   # click to focus
set  -g history-limit 50000
setw -g mode-keys vi
is_vim="ps -o state= \
  | grep vim"
bind r source-file ~/.tmux.conf \; display "reloaded"
"#;

    #[test]
    fn reads_plain_set_lines() {
        assert_eq!(value(CONFIG, "mouse").as_deref(), Some("on"));
        assert_eq!(value(CONFIG, "history-limit").as_deref(), Some("50000"));
        assert_eq!(value(CONFIG, "escape-time").as_deref(), Some("10"));
        assert_eq!(value(CONFIG, "mode-keys").as_deref(), Some("vi"));
        assert_eq!(
            value(CONFIG, "default-terminal").as_deref(),
            Some("tmux-256color")
        );
        assert_eq!(value(CONFIG, "base-index"), None);
    }

    #[test]
    fn the_last_line_wins_and_unset_clears() {
        let text = "set -g mouse on\nset -g mouse off\n";
        assert_eq!(value(text, "mouse").as_deref(), Some("off"));
        let text = "set -g mouse on\nset -gu mouse\n";
        assert_eq!(value(text, "mouse"), None);
    }

    #[test]
    fn leaves_lines_it_does_not_read() {
        for text in [
            "set -ga mouse on\n",
            "set -t main mouse on\n",
            "set mouse on\n",
            "set -g mouse on ; set -g base-index 1\n",
            "%if #{==:#{host},mac}\nset -g mouse on\n%endif\n",
            "bind m \\\nset -g mouse on\n",
            "# set -g mouse on\n",
            "set -g mouse 'on\n",
        ] {
            assert_eq!(value(text, "mouse"), None, "{text}");
        }
    }

    #[test]
    fn changes_only_the_value_and_keeps_the_rest() {
        let changed = set(CONFIG, setting("mouse"), Some("off")).unwrap();
        assert_eq!(
            changed,
            CONFIG.replace("set  -g mouse on   #", "set  -g mouse off   #")
        );
        let changed = set(CONFIG, setting("default-terminal"), Some("xterm-256color")).unwrap();
        assert_eq!(
            changed,
            CONFIG.replace("\"tmux-256color\"", "xterm-256color")
        );
    }

    #[test]
    fn adds_a_line_with_the_right_command_when_there_is_none() {
        let changed = set(CONFIG, setting("base-index"), Some("1")).unwrap();
        assert_eq!(changed, format!("{CONFIG}set -g base-index 1\n"));
        let changed = set("set -g mouse on", setting("escape-time"), Some("0")).unwrap();
        assert_eq!(changed, "set -g mouse on\nset -s escape-time 0\n");
        assert_eq!(value(&changed, "escape-time").as_deref(), Some("0"));
    }

    #[test]
    fn removing_takes_out_every_line_for_it() {
        let text = "set -g mouse on\n# keep\nset -g mouse off\nset -g base-index 1\n";
        let changed = set(text, setting("mouse"), None).unwrap();
        assert_eq!(changed, "# keep\nset -g base-index 1\n");
    }

    #[test]
    fn unset_lines_become_set_lines() {
        let changed = set("  set -gu mouse\n", setting("mouse"), Some("on")).unwrap();
        assert_eq!(changed, "  set -g mouse on\n");
    }

    #[test]
    fn values_are_quoted_so_they_read_back() {
        let changed = set("", setting("default-terminal"), Some("screen 256")).unwrap();
        assert_eq!(changed, "set -g default-terminal 'screen 256'\n");
        assert_eq!(
            value(&changed, "default-terminal").as_deref(),
            Some("screen 256")
        );
        assert!(set("", setting("default-terminal"), Some("it's")).is_err());
    }
}
