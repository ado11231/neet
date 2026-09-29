mod app;
mod art;
mod help;
mod home;
mod placeholder;

use std::io;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::App;

/// How long to wait for a key before redrawing anyway.
const TICK: Duration = Duration::from_millis(250);

/// Runs the interface loop until the user quits.
pub fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut app = App::new();
    while !app.should_quit() {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}
