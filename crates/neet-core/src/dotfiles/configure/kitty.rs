//! Reading and changing settings in `kitty.conf`, one line at a time.
//!
//! kitty reads each line as an option, spaces, and the rest of the line as
//! its value. Lines that start with `#` are comments, and there are no
//! comments after a value. The last line for an option wins.

use std::ops::Range;

use super::{Program, Setting};

/// A line that sets `key`, and where its value is in the line
fn found(line: &str, key: &str) -> Option<Range<usize>> {
    let bare = line.trim_end_matches(['\n', '\r']);
    let start = bare.len() - bare.trim_start().len();
    let rest = &bare[start..];
    let name = rest.split_whitespace().next()?;
    if name != key || rest.starts_with('#') {
        return None;
    }
    let after = &rest[name.len()..];
    let gap = after.len() - after.trim_start().len();
    // A space must come between the option and its value.
    if gap == 0 {
        return None;
    }
    let value_start = start + name.len() + gap;
    let value_end = start + rest.trim_end().len();
    (value_start < value_end).then_some(value_start..value_end)
}

/// Each of `program`'s settings in `text`
pub(super) fn values(program: &Program, text: &str) -> Vec<Option<String>> {
    program
        .settings
        .iter()
        .map(|setting| {
            text.lines()
                .rev()
                .find_map(|line| found(line, setting.key).map(|span| line[span].to_string()))
        })
        .collect()
}

/// `text` with `setting` changed to `value`, or removed for `None`.
///
/// The value of the last line for the option is replaced, so the line keeps
/// its spacing. With no line, one is added at the end. Removing takes out
/// every line for the option.
pub(super) fn set(text: &str, setting: &Setting, value: Option<&str>) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some(value) = value else {
        return lines
            .into_iter()
            .filter(|line| found(line, setting.key).is_none())
            .collect();
    };
    let last = lines
        .iter()
        .rposition(|line| found(line, setting.key).is_some());
    let mut out = String::with_capacity(text.len() + 32);
    for (index, line) in lines.iter().enumerate() {
        match found(line, setting.key) {
            Some(span) if Some(index) == last => {
                out.push_str(&line[..span.start]);
                out.push_str(value);
                out.push_str(&line[span.end..]);
            }
            _ => out.push_str(line),
        }
    }
    if last.is_none() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(setting.key);
        out.push(' ');
        out.push_str(value);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::PROGRAMS;
    use super::*;

    const KITTY: &Program = &PROGRAMS[3];

    fn setting(key: &str) -> &'static Setting {
        KITTY.settings.iter().find(|s| s.key == key).unwrap()
    }

    fn value(text: &str, key: &str) -> Option<String> {
        let index = KITTY.settings.iter().position(|s| s.key == key).unwrap();
        values(KITTY, text)[index].clone()
    }

    const CONFIG: &str = "# ~/.config/kitty/kitty.conf
# font_size 99

# --- Font ---
font_family JetBrainsMono Nerd Font
font_size   13.0

window_padding_width    4 8
scrollback_lines 10000
";

    #[test]
    fn reads_the_rest_of_the_line_and_skips_comments() {
        assert_eq!(
            value(CONFIG, "font_family").as_deref(),
            Some("JetBrainsMono Nerd Font")
        );
        assert_eq!(value(CONFIG, "font_size").as_deref(), Some("13.0"));
        assert_eq!(
            value(CONFIG, "window_padding_width").as_deref(),
            Some("4 8")
        );
        assert_eq!(value(CONFIG, "background_opacity"), None);
        assert_eq!(
            value("font_size 12\nfont_size 14\n", "font_size").as_deref(),
            Some("14")
        );
        assert_eq!(value("font_size_x 12\nfont_size\n", "font_size"), None);
    }

    #[test]
    fn changes_only_the_value_and_keeps_the_spacing() {
        assert_eq!(
            set(CONFIG, setting("font_size"), Some("14.5")),
            CONFIG.replace("font_size   13.0", "font_size   14.5")
        );
        assert_eq!(
            set(CONFIG, setting("font_family"), Some("Menlo")),
            CONFIG.replace("JetBrainsMono Nerd Font", "Menlo")
        );
    }

    #[test]
    fn adds_a_missing_option_at_the_end_and_removes_every_line() {
        assert_eq!(
            set(CONFIG, setting("background_opacity"), Some("0.9")),
            format!("{CONFIG}background_opacity 0.9\n")
        );
        assert_eq!(
            set("font_size 12", setting("enable_audio_bell"), Some("no")),
            "font_size 12\nenable_audio_bell no\n"
        );
        assert_eq!(
            set(
                "font_size 12\n# keep\nfont_size 14\n",
                setting("font_size"),
                None
            ),
            "# keep\n"
        );
        assert_eq!(
            set(CONFIG, setting("font_size"), None),
            CONFIG.replace("font_size   13.0\n", "")
        );
    }
}
