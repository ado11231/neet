//! Runs another program, with a time limit.

use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// What a finished command printed
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Captured {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `command` and returns what it printed. Both outputs are read while
/// it runs, so a lot of output never stalls it. Returns `None`, after
/// stopping it, if it runs longer than `timeout`.
pub(crate) fn capture(command: &mut Command, timeout: Duration) -> io::Result<Option<Captured>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut text);
            }
            String::from_utf8_lossy(&text).into_owned()
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(status.map(|status| Captured {
        success: status.success(),
        stdout,
        stderr,
    }))
}

/// Runs `command` and returns whether it succeeded, with what it printed as
/// errors. Returns `None`, after stopping it, if it runs longer than
/// `timeout`.
pub(crate) fn with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> io::Result<Option<(bool, String)>> {
    Ok(capture(command, timeout)?.map(|captured| (captured.success, captured.stderr)))
}

#[cfg(test)]
mod tests {
    use super::{capture, with_timeout};
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

    #[test]
    fn captures_a_lot_of_output_without_stalling() {
        let captured = capture(
            Command::new("/bin/sh").args(["-c", "yes x | head -c 300000"]),
            Duration::from_secs(10),
        )
        .expect("command should start")
        .expect("command should finish");

        assert!(captured.success);
        assert_eq!(captured.stdout.len(), 300_000);
    }
}
