//! A program neet hands the terminal to, such as an assistant resuming a
//! chat, or `sudo` asking for your password.

use std::path::PathBuf;

/// A program, its arguments, and the folder to run it in
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub program: &'static str,
    pub args: Vec<String>,
    pub folder: PathBuf,
}

impl Program {
    /// The command as you would type it, for a review
    #[must_use]
    pub fn shown(&self) -> String {
        std::iter::once(self.program.to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
