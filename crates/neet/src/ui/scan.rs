use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use neet_core::scan::{self, Progress, Scan};

/// What the scan thread sends back.
enum Message {
    Progress(Progress),
    Done(io::Result<Scan>),
}

pub enum ScanStatus {
    Running(Progress),
    Done { scan: Scan, elapsed: Duration },
    Failed(String),
}

/// The home folder scan, running on its own thread so the screen keeps responding.
pub struct ScanTask {
    status: ScanStatus,
    started: Instant,
    messages: Receiver<Message>,
}

impl ScanTask {
    pub fn start(root: PathBuf) -> Self {
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            let result = scan::scan(&root, |progress| {
                // The receiver is gone only when neet is quitting.
                let _ = sender.send(Message::Progress(progress));
            });
            let _ = sender.send(Message::Done(result));
        });
        Self {
            status: ScanStatus::Running(Progress::default()),
            started: Instant::now(),
            messages,
        }
    }

    pub fn failed(reason: impl Into<String>) -> Self {
        let (_, messages) = mpsc::channel();
        Self {
            status: ScanStatus::Failed(reason.into()),
            started: Instant::now(),
            messages,
        }
    }

    pub fn status(&self) -> &ScanStatus {
        &self.status
    }

    /// Applies every message the scan thread has sent so far.
    pub fn poll(&mut self) {
        loop {
            let message = match self.messages.try_recv() {
                Ok(message) => message,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if matches!(self.status, ScanStatus::Running(_)) {
                        self.status = ScanStatus::Failed(
                            "The scan ended without a result. Restart neet to try again.".into(),
                        );
                    }
                    break;
                }
            };
            self.status = match message {
                Message::Progress(progress) => ScanStatus::Running(progress),
                Message::Done(Ok(scan)) => ScanStatus::Done {
                    scan,
                    elapsed: self.started.elapsed(),
                },
                Message::Done(Err(error)) => ScanStatus::Failed(error.to_string()),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn wait_until_done(task: &mut ScanTask) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while matches!(task.status(), ScanStatus::Running(_)) {
            assert!(Instant::now() < deadline, "scan should finish");
            thread::sleep(Duration::from_millis(5));
            task.poll();
        }
    }

    #[test]
    fn a_finished_scan_reports_its_tree() {
        let root = tempdir().expect("temporary directory should be created");
        fs::write(root.path().join("a.txt"), "x").expect("file should be written");

        let mut task = ScanTask::start(root.path().to_path_buf());
        wait_until_done(&mut task);

        let ScanStatus::Done { scan, .. } = task.status() else {
            panic!("scan should succeed");
        };
        assert_eq!(scan.tree.node_count(), 2);
    }

    #[test]
    fn a_missing_root_fails() {
        let root = tempdir().expect("temporary directory should be created");

        let mut task = ScanTask::start(root.path().join("missing"));
        wait_until_done(&mut task);

        assert!(matches!(task.status(), ScanStatus::Failed(_)));
    }

    #[test]
    fn a_scan_without_a_final_result_stops_loading() {
        let (sender, messages) = mpsc::channel();
        let mut task = ScanTask {
            status: ScanStatus::Running(Progress::default()),
            started: Instant::now(),
            messages,
        };
        task.poll();
        assert!(matches!(task.status(), ScanStatus::Running(_)));
        sender.send(Message::Progress(Progress::default())).unwrap();
        drop(sender);
        task.poll();
        assert!(
            matches!(task.status(), ScanStatus::Failed(reason) if reason.contains("without a result"))
        );
    }

    #[test]
    fn a_queued_scan_result_is_kept_after_the_worker_exits() {
        let root = tempdir().unwrap();
        for result in [
            scan::scan(root.path(), |_| {}),
            Err(io::Error::other("scan failed")),
        ] {
            let failed = result.is_err();
            let (sender, messages) = mpsc::channel();
            let mut task = ScanTask {
                status: ScanStatus::Running(Progress::default()),
                started: Instant::now(),
                messages,
            };
            sender.send(Message::Done(result)).unwrap();
            drop(sender);
            task.poll();
            task.poll();
            if failed {
                assert!(
                    matches!(task.status(), ScanStatus::Failed(reason) if reason == "scan failed")
                );
            } else {
                assert!(matches!(task.status(), ScanStatus::Done { .. }));
            }
        }
    }
}
