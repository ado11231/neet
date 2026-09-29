//! Whether an app is open, so a rule can wait until it is closed.

use std::process::Command;

/// Whether the app with this bundle ID is open. Asks `lsappinfo`, which
/// never opens the app. If neet cannot tell, it answers yes, so the rule's
/// items are skipped rather than moved while the app may be using them.
#[must_use]
pub fn is_running(bundle_id: &str) -> bool {
    Command::new("/usr/bin/lsappinfo")
        .args(["find", &format!("bundleid={bundle_id}")])
        .output()
        .map_or(true, |output| {
            !output.status.success() || !output.stdout.trim_ascii().is_empty()
        })
}

#[cfg(test)]
mod tests {
    use super::is_running;

    #[test]
    fn a_missing_app_is_not_running() {
        assert!(!is_running("dev.neet.test.missing-app"));
    }
}
