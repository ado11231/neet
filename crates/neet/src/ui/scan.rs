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
        while let Ok(message) = self.messages.try_recv() {
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
}
