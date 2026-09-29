mod ui;

use std::process::ExitCode;

const HELP: &str = "\
neet: see what fills your Mac's disk, and safely clear files apps can make again.

Usage: neet

Options:
  --help     Print this note
  --version  Print the version
";

const ROOT: &str = "\
neet: do not run neet with sudo or as root.
It would look at root's home folder instead of yours, and cleanup never needs
admin rights. Run `neet` on its own.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [flag] if flag == "--help" => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        [flag] if flag == "--version" => {
            println!("neet {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("neet: unknown option. Run `neet --help` for usage.");
            return ExitCode::from(2);
        }
    }

    if neet_core::safety::running_as_root() {
        eprintln!("{ROOT}");
        return ExitCode::FAILURE;
    }

    match ratatui::run(ui::run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("neet: {err}");
            ExitCode::FAILURE
        }
    }
}
