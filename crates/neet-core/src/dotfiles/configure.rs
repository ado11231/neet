//! Changing a program's settings one at a time, from a list neet knows.
//! Each change is made on a copy of the file, which then goes through the
//! same check, diff, and backup as an edit. See Configure in
//! `docs/SAFETY.md`.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

mod ghostty;
mod kitty;
mod tmux;
mod toml_file;

/// How long Git may take to read or change one file
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// What kind of value a setting takes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// A whole number, such as `50000`
    Number,
    /// A number that may have a sign and a decimal point, such as `13.5`
    Decimal,
    /// One of these, which `Enter` steps through
    Choice(&'static [&'static str]),
}

const ON_OFF: Kind = Kind::Choice(&["on", "off"]);
const TRUE_FALSE: Kind = Kind::Choice(&["true", "false"]);
const YES_NO: Kind = Kind::Choice(&["yes", "no"]);

/// How a program's file is read and written
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// With `git config --file`
    Git,
    /// tmux's own commands, one a line, such as `set -g mouse on`
    Tmux,
    /// TOML, where `a.b` is the key `b` in the table `[a]`
    Toml,
    /// kitty's lines of an option, spaces, and its value, such as
    /// `font_size 13.0`
    Kitty,
    /// Ghostty's lines of a key, `=`, and its value, such as
    /// `font-size = 13`
    Ghostty,
}

/// One setting neet can change
#[derive(Debug, PartialEq, Eq)]
pub struct Setting {
    /// Such as `user.name`
    pub key: &'static str,
    /// What it does, in a few words
    pub about: &'static str,
    pub kind: Kind,
    /// For tmux, the command a new line starts with, such as `set -g`
    pub command: &'static str,
}

/// A program, the files its settings live in, and the settings neet knows
#[derive(Debug, PartialEq, Eq)]
pub struct Program {
    pub name: &'static str,
    /// Paths from the home folder
    pub files: &'static [&'static str],
    pub format: Format,
    pub settings: &'static [Setting],
}

const fn setting(key: &'static str, about: &'static str, kind: Kind) -> Setting {
    Setting {
        key,
        about,
        kind,
        command: "",
    }
}

/// A tmux setting, added as `command key value` when the file has none
const fn tmux(
    command: &'static str,
    key: &'static str,
    about: &'static str,
    kind: Kind,
) -> Setting {
    Setting {
        key,
        about,
        kind,
        command,
    }
}

/// Every program Configure knows. Keep in step with the table in SAFETY.md.
pub const PROGRAMS: &[Program] = &[
    Program {
        name: "Git",
        files: &[".gitconfig", ".config/git/config"],
        format: Format::Git,
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
                TRUE_FALSE,
            ),
            setting(
                "push.autoSetupRemote",
                "Push a new branch without naming the remote.",
                TRUE_FALSE,
            ),
        ],
    },
    Program {
        name: "tmux",
        files: &[".tmux.conf", ".config/tmux/tmux.conf"],
        format: Format::Tmux,
        settings: &[
            tmux(
                "set -g",
                "mouse",
                "Click, scroll, and resize panes with the mouse.",
                ON_OFF,
            ),
            tmux(
                "set -g",
                "history-limit",
                "Lines each pane keeps to scroll back through.",
                Kind::Number,
            ),
            tmux(
                "set -g",
                "base-index",
                "Number of the first window: 0 or 1.",
                Kind::Number,
            ),
            tmux(
                "set -g",
                "renumber-windows",
                "Number windows again when one closes.",
                ON_OFF,
            ),
            tmux(
                "setw -g",
                "mode-keys",
                "Keys for copy mode.",
                Kind::Choice(&["vi", "emacs"]),
            ),
            tmux(
                "set -s",
                "escape-time",
                "Milliseconds to wait after Esc. Low suits vim.",
                Kind::Number,
            ),
            tmux(
                "set -g",
                "status-position",
                "Where the status line sits.",
                Kind::Choice(&["top", "bottom"]),
            ),
            tmux(
                "set -g",
                "default-terminal",
                "Terminal type inside tmux, such as tmux-256color.",
                Kind::Text,
            ),
        ],
    },
    Program {
        name: "Starship",
        files: &[".config/starship.toml"],
        format: Format::Toml,
        settings: &[
            setting("add_newline", "Blank line before each prompt.", TRUE_FALSE),
            setting(
                "line_break.disabled",
                "Keep the prompt on the same line as the folder.",
                TRUE_FALSE,
            ),
            setting(
                "character.success_symbol",
                "Prompt mark after a command works.",
                Kind::Text,
            ),
            setting(
                "character.error_symbol",
                "Prompt mark after a command fails.",
                Kind::Text,
            ),
            setting(
                "directory.truncation_length",
                "How many folders the path shows.",
                Kind::Number,
            ),
            setting(
                "cmd_duration.min_time",
                "Milliseconds a command runs before its time shows.",
                Kind::Number,
            ),
            setting(
                "command_timeout",
                "Milliseconds a command may take before it is skipped.",
                Kind::Number,
            ),
            setting(
                "scan_timeout",
                "Milliseconds to look through the folder's files.",
                Kind::Number,
            ),
        ],
    },
    Program {
        name: "kitty",
        files: &[".config/kitty/kitty.conf"],
        format: Format::Kitty,
        settings: &[
            setting("font_family", "Font for the terminal.", Kind::Text),
            setting(
                "font_size",
                "Font size in points, such as 13.0.",
                Kind::Decimal,
            ),
            setting(
                "scrollback_lines",
                "Lines kept to scroll back through.",
                Kind::Number,
            ),
            setting(
                "window_padding_width",
                "Space around the text in points: one to four numbers.",
                Kind::Text,
            ),
            setting(
                "background_opacity",
                "How solid the background is, from 0 to 1.",
                Kind::Decimal,
            ),
            setting(
                "cursor_blink_interval",
                "Seconds between cursor blinks. 0 stops blinking.",
                Kind::Decimal,
            ),
            setting(
                "macos_option_as_alt",
                "Which Option keys act as Alt, for terminal shortcuts.",
                Kind::Choice(&["no", "left", "right", "both"]),
            ),
            setting("enable_audio_bell", "Beep on the terminal bell.", YES_NO),
        ],
    },
    Program {
        name: "mise",
        files: &[".config/mise/config.toml"],
        format: Format::Toml,
        settings: &[
            setting(
                "tools.node",
                "Node version for every folder, such as 24 or lts.",
                Kind::Text,
            ),
            setting(
                "tools.python",
                "Python version for every folder, such as 3.12.",
                Kind::Text,
            ),
            setting(
                "tools.go",
                "Go version for every folder, such as latest.",
                Kind::Text,
            ),
            setting(
                "tools.ruby",
                "Ruby version for every folder, such as 3.3.",
                Kind::Text,
            ),
            setting(
                "settings.auto_install",
                "Install a missing version when a command needs it.",
                TRUE_FALSE,
            ),
            setting(
                "settings.jobs",
                "How many tools install at once.",
                Kind::Number,
            ),
            setting(
                "settings.experimental",
                "Turn on features mise is still trying out.",
                TRUE_FALSE,
            ),
        ],
    },
    Program {
        name: "Ghostty",
        files: &[".config/ghostty/config", ".config/ghostty/config.ghostty"],
        format: Format::Ghostty,
        settings: &[
            setting(
                "theme",
                "Color theme, such as Catppuccin Mocha.",
                Kind::Text,
            ),
            setting(
                "font-size",
                "Font size in points, such as 13.",
                Kind::Decimal,
            ),
            setting(
                "background-opacity",
                "How solid the background is, from 0 to 1.",
                Kind::Decimal,
            ),
            setting(
                "window-padding-x",
                "Space left and right of the text, such as 4 or 4,8.",
                Kind::Text,
            ),
            setting(
                "window-padding-y",
                "Space above and below the text, such as 4 or 4,8.",
                Kind::Text,
            ),
            setting(
                "cursor-style",
                "Shape of the cursor.",
                Kind::Choice(&["block", "bar", "underline", "block_hollow"]),
            ),
            setting(
                "scrollback-limit",
                "Bytes of output kept to scroll back through.",
                Kind::Number,
            ),
            setting(
                "macos-option-as-alt",
                "Which Option keys act as Alt, for terminal shortcuts.",
                Kind::Choice(&["false", "true", "left", "right"]),
            ),
            setting(
                "mouse-hide-while-typing",
                "Hide the mouse pointer while you type.",
                TRUE_FALSE,
            ),
            setting(
                "copy-on-select",
                "Copy text when you select it.",
                Kind::Choice(&["true", "false", "clipboard"]),
            ),
        ],
    },
];

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

/// Why `value` cannot be `setting`'s value, before anything is written
///
/// # Errors
///
/// Returns the reason in plain words.
pub fn check(setting: &Setting, value: &str) -> Result<(), String> {
    if value.contains(['\n', '\r', '\0']) {
        return Err("A value cannot have more than one line.".to_string());
    }
    match setting.kind {
        Kind::Text => Ok(()),
        Kind::Number if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            value
                .parse::<i64>()
                .map(|_| ())
                .map_err(|_| format!("{} is too large.", setting.key))
        }
        Kind::Number => Err(format!("{} is a whole number, such as 10.", setting.key)),
        Kind::Decimal if is_decimal(value) => Ok(()),
        Kind::Decimal => Err(format!("{} is a number, such as 13.5.", setting.key)),
        Kind::Choice(choices) if choices.contains(&value) => Ok(()),
        Kind::Choice(choices) => Err(format!("{} is {}.", setting.key, choices.join(" or "))),
    }
}

/// Digits, with a `-` before them and one `.` among them at most
fn is_decimal(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    !whole.is_empty()
        && !fraction.is_empty()
        && whole.len() <= 12
        && whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
}

/// The value of each of `program`'s settings in `file`, in the same order,
/// or `None` when it is not set. Only that file is read, not the files it
/// includes or sources.
///
/// # Errors
///
/// Returns why the file cannot be read.
pub fn values(program: &Program, file: &Path) -> Result<Vec<Option<String>>, String> {
    match program.format {
        Format::Git => git_values(program, file),
        Format::Tmux => Ok(tmux::values(program, &read(file)?)),
        Format::Toml => toml_file::values(program, &read(file)?),
        Format::Kitty => Ok(kitty::values(program, &read(file)?)),
        Format::Ghostty => Ok(ghostty::values(program, &read(file)?)),
    }
}

/// Sets `setting` in `file` to `value`, or removes it for `None`. Every
/// other line, comment, and their order stay as they are.
///
/// # Errors
///
/// Returns why the value was refused, or why the file could not be changed.
pub fn set(
    program: &Program,
    file: &Path,
    setting: &Setting,
    value: Option<&str>,
) -> Result<(), String> {
    if let Some(value) = value {
        check(setting, value)?;
    }
    match program.format {
        Format::Git => git_set(file, setting, value),
        Format::Tmux => write(file, &tmux::set(&read(file)?, setting, value)?),
        Format::Toml => write(file, &toml_file::set(&read(file)?, setting, value)?),
        Format::Kitty => write(file, &kitty::set(&read(file)?, setting, value)),
        Format::Ghostty => write(file, &ghostty::set(&read(file)?, setting, value)?),
    }
}

fn read(file: &Path) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|error| format!("It could not be read: {error}."))
}

/// Writes the copy. It is neet's own copy, so it is written in place.
fn write(file: &Path, text: &str) -> Result<(), String> {
    std::fs::write(file, text).map_err(|error| format!("It could not be changed: {error}."))
}

fn git_values(program: &Program, file: &Path) -> Result<Vec<Option<String>>, String> {
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

fn git_set(file: &Path, setting: &Setting, value: Option<&str>) -> Result<(), String> {
    let captured = if let Some(value) = value {
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

    fn set(file: &std::path::Path, setting: &Setting, value: Option<&str>) -> Result<(), String> {
        super::set(GIT, file, setting, value)
    }

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
        assert_eq!(
            program_for(".config/tmux/tmux.conf").map(|p| p.name),
            Some("tmux")
        );
        assert_eq!(
            program_for(".config/starship.toml").map(|p| p.name),
            Some("Starship")
        );
        assert_eq!(
            program_for(".config/kitty/kitty.conf").map(|p| p.name),
            Some("kitty")
        );
        assert_eq!(
            program_for(".config/mise/config.toml").map(|p| p.name),
            Some("mise")
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
    fn checks_numbers_and_choices() {
        let tmux = &PROGRAMS[1];
        let find = |key: &str| tmux.settings.iter().find(|s| s.key == key).unwrap();
        assert!(check(find("history-limit"), "50000").is_ok());
        assert!(check(find("history-limit"), "lots").is_err());
        assert!(check(find("history-limit"), "-1").is_err());
        assert!(check(find("history-limit"), "99999999999999999999").is_err());
        assert!(check(find("mouse"), "on").is_ok());
        assert_eq!(
            check(find("mouse"), "yes"),
            Err("mouse is on or off.".to_string())
        );
        assert!(check(find("default-terminal"), "a\nb").is_err());
        let kitty = &PROGRAMS[3];
        let size = kitty
            .settings
            .iter()
            .find(|s| s.key == "font_size")
            .unwrap();
        for good in ["13", "13.5", "-1", "0.25"] {
            assert!(check(size, good).is_ok(), "{good}");
        }
        for bad in ["", "big", "1.", ".5", "1.2.3", "--1", "1e9"] {
            assert!(check(size, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_setting_is_listed_once_and_tmux_ones_say_how_to_add_them() {
        for program in PROGRAMS {
            for (index, setting) in program.settings.iter().enumerate() {
                assert!(
                    !program.settings[..index]
                        .iter()
                        .any(|s| s.key == setting.key),
                    "{} twice",
                    setting.key
                );
                assert_eq!(
                    program.format == Format::Tmux,
                    !setting.command.is_empty(),
                    "{}",
                    setting.key
                );
            }
        }
    }

    #[test]
    fn refuses_values_git_would_misread() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".gitconfig");
        fs::write(&file, "").unwrap();
        assert!(set(&file, setting("user.name"), Some("a\nb")).is_err());
        assert!(set(&file, setting("pull.rebase"), Some("yes")).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "");
        set(&file, setting("user.name"), Some("--global")).unwrap();
        assert_eq!(values(GIT, &file).unwrap()[0].as_deref(), Some("--global"));
    }
}
