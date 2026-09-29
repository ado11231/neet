mod app;
mod art;
mod clean;
mod disk;
mod format;
mod help;
mod home;
mod scan;
mod skipped;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::App;
use scan::ScanTask;

/// How long to wait for a key before redrawing anyway.
const TICK: Duration = Duration::from_millis(250);

/// Runs the interface loop until the user quits.
pub fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let scan = match &home {
        Some(home) => ScanTask::start(home.clone()),
        None => ScanTask::failed("HOME is not set, so there is no home folder to scan."),
    };
    let mut app = App::new(scan, home);
    while !app.should_quit() {
        app.poll();
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
