//! Leaves the screen to open a file in your editor, then comes back.

use std::io;
use std::path::Path;
use std::process::Command;

use ratatui::DefaultTerminal;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

/// Your editor: `$VISUAL`, then `$EDITOR`, then `nano`, split into the
/// program and its options, such as `code --wait`
fn command() -> Vec<String> {
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|value| {
            value
                .to_string_lossy()
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .find(|words| !words.is_empty())
        .unwrap_or_else(|| vec!["nano".to_string()])
}

/// Opens `path` in your editor and waits for it to close.
fn open(path: &Path) -> Result<(), String> {
    let words = command();
    let (program, options) = words.split_first().expect("the command is never empty");
    match Command::new(program).args(options).arg(path).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("{program} quit with an error, so nothing changed.")),
        Err(error) => Err(format!(
            "{program} could not open: {error}. Set $EDITOR to the editor you use."
        )),
    }
}

/// Puts the terminal back as it was, opens `path` in your editor, then
/// takes the terminal again.
pub fn suspend(terminal: &mut DefaultTerminal, path: &Path) -> Result<(), String> {
    ratatui::restore();
    let result = open(path);
    let resumed = enable_raw_mode()
        .and_then(|()| execute!(io::stdout(), EnterAlternateScreen))
        .and_then(|()| terminal.clear());
    match (result, resumed) {
        (result, Ok(())) => result,
        (_, Err(error)) => Err(format!("The screen could not come back: {error}.")),
    }
}
