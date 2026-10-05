//! Runs another program, with a time limit.

use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Runs `command` and returns whether it succeeded, with what it printed as
/// errors. Returns `None`, after stopping it, if it runs longer than
/// `timeout`.
pub(crate) fn with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> io::Result<Option<(bool, String)>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    Ok(Some((status.success(), stderr)))
}

#[cfg(test)]
mod tests {
    use super::with_timeout;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn stops_a_command_that_does_not_answer_in_time() {
        let started = Instant::now();

        let result = with_timeout(
            Command::new("/bin/sleep").arg("10"),
            Duration::from_millis(200),
        )
        .expect("command should start");

        assert!(result.is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn returns_what_a_command_printed_when_it_finishes() {
        let result = with_timeout(
            Command::new("/bin/sh").args(["-c", "echo refused >&2; exit 1"]),
            Duration::from_secs(5),
        )
        .expect("command should start");

        assert_eq!(result, Some((false, "refused\n".to_string())));
    }
}
