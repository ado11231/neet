//! Reading and changing settings in a TOML file, such as `starship.toml`.
//! `toml_edit` changes the one value and keeps every comment, blank line,
//! and the order of the rest.

use toml_edit::{DocumentMut, Item, TableLike, Value};

use super::{Kind, Program, Setting};

fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|error| format!("It is not valid TOML: {}", error.message()))
}

/// The item at `key`, where `a.b` is `b` in the table `a`
fn find<'a>(document: &'a DocumentMut, key: &str) -> Option<&'a Item> {
    let mut parts = key.split('.');
    let mut table: &dyn TableLike = document.as_table();
    let mut part = parts.next()?;
    for next in parts {
        table = table.get(part)?.as_table_like()?;
        part = next;
    }
    table.get(part)
}

/// A value as the settings table shows it: text without its quotes
fn shown(item: &Item) -> String {
    match item.as_value() {
        Some(Value::String(text)) => text.value().clone(),
        Some(value) => value.clone().decorated("", "").to_string(),
        None => item.to_string().trim().to_string(),
    }
}

/// Each of `program`'s settings in `text`
pub(super) fn values(program: &Program, text: &str) -> Result<Vec<Option<String>>, String> {
    let document = parse(text)?;
    Ok(program
        .settings
        .iter()
        .map(|setting| find(&document, setting.key).map(shown))
        .collect())
}

/// `value` as TOML: a number, true or false, or else text
fn typed(setting: &Setting, value: &str) -> Value {
    match setting.kind {
        Kind::Number => value
            .parse::<i64>()
            .map_or_else(|_| Value::from(value), Value::from),
        Kind::Choice(["true", "false"]) => Value::from(value == "true"),
        Kind::Text | Kind::Choice(_) => Value::from(value),
    }
}

/// `text` with `setting` changed to `value`, or removed for `None`. A table
/// that is missing is added at the end, such as `[character]`.
pub(super) fn set(text: &str, setting: &Setting, value: Option<&str>) -> Result<String, String> {
    if value.is_none()
        && let Some(removed) = remove_line(text, setting.key)
    {
        return Ok(removed);
    }
    let mut document = parse(text)?;
    let not_a_value = || {
        format!(
            "{} is not a single value here. Change it with e.",
            setting.key
        )
    };
    let mut parts: Vec<&str> = setting.key.split('.').collect();
    let last = parts.pop().unwrap_or(setting.key);
    let mut table: &mut dyn TableLike = document.as_table_mut();
    for part in parts {
        if table.get(part).is_none() {
            if value.is_none() {
                return Ok(text.to_string());
            }
            table.insert(part, toml_edit::table());
        }
        table = table
            .get_mut(part)
            .and_then(Item::as_table_like_mut)
            .ok_or_else(not_a_value)?;
    }
    match (table.get_mut(last), value) {
        (Some(item), _) if !item.is_value() => return Err(not_a_value()),
        (Some(item), Some(value)) => {
            // Keep the spacing and any comment around the old value.
            let decor = item.as_value().map(|old| old.decor().clone());
            let mut new = typed(setting, value);
            if let Some(decor) = decor {
                *new.decor_mut() = decor;
            }
            *item = Item::Value(new);
        }
        (None, Some(value)) => {
            table.insert(last, Item::Value(typed(setting, value)));
        }
        (_, None) => {
            table.remove(last);
        }
    }
    Ok(document.to_string())
}

/// `text` without the line that sets `key`, so the comments above it stay.
/// `None` when the key is not on a line of its own, or when the result
/// would mean anything other than `text` without that one value.
fn remove_line(text: &str, key: &str) -> Option<String> {
    let document = toml_edit::Document::parse(text).ok()?;
    let mut parts: Vec<&str> = key.split('.').collect();
    let last = parts.pop()?;
    let mut table: &dyn TableLike = document.as_table();
    for part in parts {
        table = table.get(part)?.as_table_like()?;
    }
    let (found, item) = table.get_key_value(last)?;
    let start = text[..found.span()?.start]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let end = item.span()?.end;
    let end = text[end..].find('\n').map_or(text.len(), |at| end + at + 1);
    let removed = format!("{}{}", &text[..start], &text[end..]);

    // The same values, less the one removed
    let mut expected: toml::Table = toml::from_str(text).ok()?;
    let mut parts: Vec<&str> = key.split('.').collect();
    let last = parts.pop()?;
    let mut table = &mut expected;
    for part in parts {
        table = table.get_mut(part)?.as_table_mut()?;
    }
    table.remove(last)?;
    (toml::from_str::<toml::Table>(&removed).ok()? == expected).then_some(removed)
}

#[cfg(test)]
mod tests {
    use super::super::PROGRAMS;
    use super::*;

    const STARSHIP: &Program = &PROGRAMS[2];

    fn setting(key: &str) -> &'static Setting {
        STARSHIP.settings.iter().find(|s| s.key == key).unwrap()
    }

    fn value(text: &str, key: &str) -> Option<String> {
        let index = STARSHIP.settings.iter().position(|s| s.key == key).unwrap();
        values(STARSHIP, text).unwrap()[index].clone()
    }

    const CONFIG: &str = r#"# ~/.config/starship.toml -- prompt.

add_newline = false
line_break.disabled = true

[character]
success_symbol = "[❯](bold white)" # mine
error_symbol = "[❯](bold red)"

# --- Icons ---
[git_branch]
symbol = " "
"#;

    #[test]
    fn reads_values_tables_and_dotted_keys() {
        assert_eq!(value(CONFIG, "add_newline").as_deref(), Some("false"));
        assert_eq!(
            value(CONFIG, "line_break.disabled").as_deref(),
            Some("true")
        );
        assert_eq!(
            value(CONFIG, "character.success_symbol").as_deref(),
            Some("[❯](bold white)")
        );
        assert_eq!(value(CONFIG, "command_timeout"), None);
        assert_eq!(value(CONFIG, "directory.truncation_length"), None);
    }

    #[test]
    fn changes_one_value_and_keeps_comments_and_icons() {
        let changed = set(
            CONFIG,
            setting("character.success_symbol"),
            Some("[>](green)"),
        )
        .unwrap();
        assert_eq!(
            changed,
            CONFIG.replace("\"[❯](bold white)\" # mine", "\"[>](green)\" # mine")
        );
        let changed = set(CONFIG, setting("line_break.disabled"), Some("false")).unwrap();
        assert_eq!(
            changed,
            CONFIG.replace("line_break.disabled = true", "line_break.disabled = false")
        );
    }

    #[test]
    fn adds_numbers_and_missing_tables() {
        let changed = set(CONFIG, setting("command_timeout"), Some("1000")).unwrap();
        assert!(
            changed.contains("line_break.disabled = true\ncommand_timeout = 1000\n"),
            "{changed}"
        );
        let changed = set(CONFIG, setting("directory.truncation_length"), Some("3")).unwrap();
        assert!(changed.starts_with(CONFIG), "{changed}");
        assert!(
            changed.ends_with("[directory]\ntruncation_length = 3\n"),
            "{changed}"
        );
        assert_eq!(
            value(&changed, "directory.truncation_length").as_deref(),
            Some("3")
        );
    }

    #[test]
    fn removes_a_value_and_leaves_the_rest() {
        let changed = set(CONFIG, setting("add_newline"), None).unwrap();
        assert_eq!(changed, CONFIG.replace("add_newline = false\n", ""));
        let changed = set(CONFIG, setting("character.success_symbol"), None).unwrap();
        assert_eq!(
            changed,
            CONFIG.replace("success_symbol = \"[❯](bold white)\" # mine\n", "")
        );
        // Nothing to remove changes nothing.
        let same = set(CONFIG, setting("cmd_duration.min_time"), None).unwrap();
        assert_eq!(same, CONFIG);
    }

    #[test]
    fn refuses_what_it_cannot_change_safely() {
        assert!(values(STARSHIP, "add_newline = \n").is_err());
        let text = "[character.success_symbol]\nx = 1\n";
        assert!(set(text, setting("character.success_symbol"), Some(">")).is_err());
        let text = "character = \"x\"\n";
        assert!(set(text, setting("character.error_symbol"), Some(">")).is_err());
    }
}
