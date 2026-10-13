//! Runs another program, with a time limit.

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// What a finished command printed
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Captured {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `command` and reads both outputs while it runs. Returns `None`
/// if the command or its output takes longer than `timeout`.
pub(crate) fn capture(command: &mut Command, timeout: Duration) -> io::Result<Option<Captured>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let result = read_output(&mut child, Instant::now() + timeout);
    if !matches!(result, Ok(Some(_))) {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

struct Output {
    pipe: File,
    bytes: Vec<u8>,
    closed: bool,
}

impl Output {
    fn new(pipe: impl Into<OwnedFd>) -> io::Result<Self> {
        let pipe = File::from(pipe.into());
        let flags = rustix::fs::fcntl_getfl(&pipe)?;
        rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Self {
            pipe,
            bytes: Vec::new(),
            closed: false,
        })
    }

    // Read one chunk so neither output can hold up the other or the deadline.
    fn read(&mut self) -> io::Result<bool> {
        let mut buffer = [0; 8192];
        match self.pipe.read(&mut buffer) {
            Ok(0) => self.closed = true,
            Ok(count) => {
                self.bytes.extend_from_slice(&buffer[..count]);
                return Ok(true);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        Ok(false)
    }
}

fn read_output(child: &mut Child, deadline: Instant) -> io::Result<Option<Captured>> {
    let mut stdout = Output::new(child.stdout.take().expect("stdout is piped"))?;
    let mut stderr = Output::new(child.stderr.take().expect("stderr is piped"))?;
    let mut status = None;
    loop {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let out_read = stdout.read()?;
        let err_read = stderr.read()?;
        if status.is_none() {
            status = child.try_wait()?;
        }
        if let Some(status) = status
            && stdout.closed
            && stderr.closed
        {
            return Ok(Some(Captured {
                success: status.success(),
                code: status.code(),
                stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
                stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
            }));
        }
        if !out_read && !err_read {
            thread::sleep(
                Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
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
            Command::new("/bin/sh")
                .args(["-c", "yes x | head -c 300000; yes y | head -c 300000 >&2"]),
            Duration::from_secs(10),
        )
        .expect("command should start")
        .expect("command should finish");

        assert!(captured.success);
        assert_eq!(captured.stdout.len(), 300_000);
        assert_eq!(captured.stderr.len(), 300_000);
    }

    #[test]
    fn a_background_child_cannot_keep_output_open_past_the_timeout() {
        for script in [
            "sleep 3 & exit 0",
            "sleep 3 >/dev/null & exit 0",
            "sleep 3 2>/dev/null & exit 0",
            "sleep 3 & wait",
        ] {
            let started = Instant::now();
            let result = capture(
                Command::new("/bin/sh").args(["-c", script]),
                Duration::from_millis(100),
            )
            .expect("command should start");

            assert!(started.elapsed() < Duration::from_secs(2), "{script}");
            assert!(result.is_none(), "{script}");
        }
    }

    #[test]
    fn closed_pipes_or_constant_output_do_not_skip_the_timeout() {
        for script in [
            "exec 1>&- 2>&-; sleep 3",
            "while :; do printf out; printf err >&2; done",
        ] {
            let started = Instant::now();
            let result = capture(
                Command::new("/bin/sh").args(["-c", script]),
                Duration::from_millis(100),
            )
            .expect("command should start");

            assert!(started.elapsed() < Duration::from_secs(2), "{script}");
            assert!(result.is_none(), "{script}");
        }
    }
}
