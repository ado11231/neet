//! Dry run plans, and running them. See how a cleanup runs in
//! `docs/SAFETY.md`.
//!
//! Every item in a plan passed
//! [`CleanupRoots::validate_deletable`](crate::safety::CleanupRoots::validate_deletable).
//! [`run`] checks it again right before the move.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::rules::{Category, Rule, Source, Tier};
use crate::safety::{CleanupRoots, SafetyError, ValidatedPath, copy_error};
use crate::size::{HardLinkTracker, allocated_size};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// One file or folder a rule would move to the Trash
#[derive(Clone, Debug)]
pub struct PlanItem {
    pub path: ValidatedPath,
    /// Space it uses on disk. A file with several names is counted once in
    /// the whole plan.
    pub size: u64,
    /// The newest change anywhere inside it
    pub changed: SystemTime,
}

/// Why a path a rule found is left out of the plan
#[derive(Debug)]
pub enum SkipReason {
    /// The path check refused it
    Refused(SafetyError),
    /// Changed within the rule's minimum age
    TooNew,
    /// The times of everything inside could not be read
    Unreadable(io::Error),
    /// It is, holds, or sits inside an item another rule already plans
    Overlaps,
    /// A symbolic link. Links are left in place.
    Link,
    /// This app is open, and the rule needs it closed
    AppOpen(String),
    /// The path now leads to a different file than the one you reviewed
    Replaced,
    /// Finder could not move it
    MoveFailed(io::Error),
}

impl Clone for SkipReason {
    fn clone(&self) -> Self {
        match self {
            Self::Refused(error) => Self::Refused(error.clone()),
            Self::TooNew => Self::TooNew,
            Self::Unreadable(error) => Self::Unreadable(copy_error(error)),
            Self::Overlaps => Self::Overlaps,
            Self::Link => Self::Link,
            Self::AppOpen(app) => Self::AppOpen(app.clone()),
            Self::Replaced => Self::Replaced,
            Self::MoveFailed(error) => Self::MoveFailed(copy_error(error)),
        }
    }
}

/// A path a rule found but the plan leaves out
#[derive(Clone, Debug)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// What one rule found
#[derive(Clone, Debug)]
pub struct RulePlan {
    pub rule: Rule,
    pub items: Vec<PlanItem>,
    pub skipped: Vec<Skipped>,
    /// Only `safe` rules are selected from the start
    pub selected: bool,
}

impl RulePlan {
    #[must_use]
    pub fn size(&self) -> u64 {
        self.items.iter().map(|item| item.size).sum()
    }
}

/// What every rule found. Making a plan changes nothing.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub rules: Vec<RulePlan>,
}

impl Plan {
    /// How many items the selected rules would move
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.selected().map(|rule| rule.items.len()).sum()
    }

    /// How much space the selected rules would free
    #[must_use]
    pub fn selected_size(&self) -> u64 {
        self.selected().map(RulePlan::size).sum()
    }

    fn selected(&self) -> impl Iterator<Item = &RulePlan> {
        self.rules.iter().filter(|rule| rule.selected)
    }
}

/// Whether `name` matches `pattern`, where `*` stands for any run of
/// characters, including none.
fn matches(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        // No `*` at all
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

/// The paths on disk a rule path matches, sorted. Reading nothing is fine.
fn expand(pattern: &Path) -> Vec<PathBuf> {
    let mut found = vec![PathBuf::from("/")];
    for part in pattern.iter().skip(1) {
        let Some(part) = part.to_str() else {
            return Vec::new();
        };
        if !part.contains('*') {
            found = found.into_iter().map(|path| path.join(part)).collect();
            continue;
        }
        let mut next = Vec::new();
        for folder in &found {
            let Ok(entries) = fs::read_dir(folder) else {
                continue;
            };
            for entry in entries.flatten() {
                // Finder's view settings, left in folders it has shown. Not
                // cache, and alone they made empty caches look found.
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name != ".DS_Store" && matches(part, name))
                {
                    next.push(entry.path());
                }
            }
        }
        found = next;
    }
    found.retain(|path| fs::symlink_metadata(path).is_ok());
    found.sort();
    found
}

/// The space an item uses and its newest change, without following links or
/// leaving its disk. Fails if anything inside cannot be read.
pub(crate) fn measure(path: &Path, tracker: &mut HardLinkTracker) -> io::Result<(u64, SystemTime)> {
    let top = fs::symlink_metadata(path)?;
    let device = top.dev();
    let mut size = 0;
    let mut changed = top.modified()?;
    let mut folders = Vec::new();
    if tracker.first_sighting(&top) {
        size += allocated_size(&top);
    }
    if top.is_dir() {
        folders.push(path.to_path_buf());
    }
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            changed = changed.max(metadata.modified()?);
            if tracker.first_sighting(&metadata) {
                size += allocated_size(&metadata);
            }
            if metadata.is_dir() && metadata.dev() == device {
                folders.push(entry.path());
            }
        }
    }
    Ok((size, changed))
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Makes a dry run plan for `rules`, in order. Nothing is changed on disk.
/// An item found by more than one rule goes to the first.
#[must_use]
pub fn plan(rules: &[Rule], roots: &CleanupRoots, now: SystemTime) -> Plan {
    let mut tracker = HardLinkTracker::new();
    let mut planned: Vec<PathBuf> = Vec::new();
    let mut plan = Plan::default();

    for rule in rules {
        let mut items = Vec::new();
        let mut skipped = Vec::new();
        let mut skip = |path: PathBuf, reason| skipped.push(Skipped { path, reason });
        let oldest_allowed = now
            .checked_sub(DAY * rule.min_age_days)
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let mut found: Vec<PathBuf> = rule.paths.iter().flat_map(|path| expand(path)).collect();
        found.sort();
        found.dedup();
        for path in found {
            let validated = match roots.validate_deletable(&path) {
                Ok(validated) => validated,
                Err(error) => {
                    skip(path, SkipReason::Refused(error));
                    continue;
                }
            };
            if fs::symlink_metadata(validated.path()).is_ok_and(|m| m.file_type().is_symlink()) {
                skip(path, SkipReason::Link);
                continue;
            }
            if planned.iter().any(|done| overlaps(done, validated.path())) {
                skip(path, SkipReason::Overlaps);
                continue;
            }
            match measure(validated.path(), &mut tracker) {
                Err(error) => skip(path, SkipReason::Unreadable(error)),
                Ok((_, changed)) if changed > oldest_allowed => skip(path, SkipReason::TooNew),
                Ok((size, changed)) => {
                    planned.push(validated.path().to_path_buf());
                    items.push(PlanItem {
                        path: validated,
                        size,
                        changed,
                    });
                }
            }
        }

        plan.rules.push(RulePlan {
            selected: rule.tier == Tier::Safe,
            rule: rule.clone(),
            items,
            skipped,
        });
    }
    plan
}

/// Plans a cleanup of one item you picked in the Disk screen. It goes
/// through the same path check as any rule, and comes back selected.
///
/// # Errors
///
/// Returns why the item cannot be cleaned.
pub fn plan_path(path: &Path, roots: &CleanupRoots, now: SystemTime) -> Result<Plan, SkipReason> {
    // Rule paths treat `*` as a pattern, so a name holding one could match
    // other items.
    if path.to_string_lossy().contains('*') {
        return Err(SkipReason::Refused(SafetyError::Unsupported));
    }
    let rule = Rule {
        id: "picked-in-disk".to_string(),
        name: "Picked in Disk".to_string(),
        category: Category::Application,
        tier: Tier::Caution,
        paths: vec![path.to_path_buf()],
        description: "An item you picked in the Disk screen.".to_string(),
        regenerates: false,
        requires_quit: Vec::new(),
        min_age_days: 0,
        source: Source::Disk,
    };
    let mut plan = plan(&[rule], roots, now);
    let rule_plan = &mut plan.rules[0];
    if let Some(skipped) = rule_plan.skipped.pop() {
        return Err(skipped.reason);
    }
    if rule_plan.items.is_empty() {
        return Err(SkipReason::Unreadable(io::Error::from(
            io::ErrorKind::NotFound,
        )));
    }
    rule_plan.selected = true;
    Ok(plan)
}

/// One item moved to the Trash
#[derive(Debug)]
pub struct Moved {
    pub path: PathBuf,
    pub size: u64,
}

/// What a cleanup did
#[derive(Debug, Default)]
pub struct Outcome {
    pub moved: Vec<Moved>,
    pub skipped: Vec<Skipped>,
}

impl Outcome {
    /// The space the moved items take up in the Trash
    #[must_use]
    pub fn moved_size(&self) -> u64 {
        self.moved.iter().map(|item| item.size).sum()
    }
}

/// Checks one planned item again right before it moves. Returns why it must
/// be skipped, if it must.
fn recheck(
    item: &PlanItem,
    rule: &Rule,
    roots: &CleanupRoots,
    now: SystemTime,
    is_running: &impl Fn(&str) -> bool,
) -> Option<SkipReason> {
    if let Some(app) = rule.requires_quit.iter().find(|app| is_running(app)) {
        return Some(SkipReason::AppOpen(app.clone()));
    }
    let again = match roots.validate_again(&item.path) {
        Ok(again) => again,
        Err(error) => return Some(SkipReason::Refused(error)),
    };
    if again != item.path {
        return Some(SkipReason::Replaced);
    }
    if fs::symlink_metadata(again.path()).is_ok_and(|m| m.file_type().is_symlink()) {
        return Some(SkipReason::Link);
    }
    let oldest_allowed = now
        .checked_sub(DAY * rule.min_age_days)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    match measure(again.path(), &mut HardLinkTracker::new()) {
        Err(error) => Some(SkipReason::Unreadable(error)),
        Ok((_, changed)) if changed > oldest_allowed => Some(SkipReason::TooNew),
        Ok(_) => None,
    }
}

/// Moves the items of the selected rules with `move_item`, checking each one
/// again right before. `is_running` says whether an app is open, and
/// `progress` hears how many items are done out of how many.
pub fn run(
    plan: &Plan,
    roots: &CleanupRoots,
    now: SystemTime,
    is_running: impl Fn(&str) -> bool,
    mut move_item: impl FnMut(&Path) -> io::Result<()>,
    mut progress: impl FnMut(usize, usize),
) -> Outcome {
    let total = plan.selected_count();
    let mut outcome = Outcome::default();
    let mut done = 0;
    for rule_plan in plan.selected() {
        for item in &rule_plan.items {
            let path = item.path.path().to_path_buf();
            let reason = recheck(item, &rule_plan.rule, roots, now, &is_running)
                .or_else(|| move_item(&path).err().map(SkipReason::MoveFailed));
            match reason {
                Some(reason) => outcome.skipped.push(Skipped { path, reason }),
                None => outcome.moved.push(Moved {
                    path,
                    size: item.size,
                }),
            }
            done += 1;
            progress(done, total);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::{SkipReason, expand, matches, plan, run};
    use crate::rules::{Category, Rule, Source, Tier, load};
    use crate::safety::CleanupRoots;
    use std::cell::RefCell;
    use std::fs::{self, File, FileTimes};
    use std::io;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};
    use tempfile::{TempDir, tempdir};

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn home() -> (TempDir, CleanupRoots) {
        let dir = tempdir().expect("temporary directory should be created");
        for folder in ["Library/Caches", "Library/Logs", "Documents"] {
            fs::create_dir_all(dir.path().join(folder)).expect("folder should be created");
        }
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        (dir, roots)
    }

    fn rule(id: &str, tier: Tier, paths: &[&str], roots: &CleanupRoots) -> Rule {
        Rule {
            id: id.to_string(),
            name: id.to_string(),
            category: Category::Application,
            tier,
            paths: paths.iter().map(|path| roots.home().join(path)).collect(),
            description: "Test files.".to_string(),
            regenerates: true,
            requires_quit: Vec::new(),
            min_age_days: 0,
            source: Source::Bundled,
        }
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().expect("path should have a parent"))
            .expect("folder should be created");
        fs::write(path, vec![7; bytes]).expect("file should be written");
    }

    fn age(path: &Path, days: u32) {
        let time = SystemTime::now() - DAY * days;
        File::open(path)
            .expect("file should open")
            .set_times(FileTimes::new().set_modified(time))
            .expect("time should be set");
    }

    fn listing(root: &Path) -> Vec<(PathBuf, u64)> {
        let mut all = Vec::new();
        let mut folders = vec![root.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(folder).expect("folder should be read") {
                let path = entry.expect("entry should be read").path();
                let metadata = fs::symlink_metadata(&path).expect("metadata should be read");
                if metadata.is_dir() {
                    folders.push(path.clone());
                }
                all.push((path, metadata.len()));
            }
        }
        all.sort();
        all
    }

    #[test]
    fn star_matches_any_run_of_characters() {
        assert!(matches("*", "anything"));
        assert!(matches("*", ".hidden"));
        assert!(matches("com.example.*", "com.example.app"));
        assert!(matches("com.example.*", "com.example."));
        assert!(!matches("com.example.*", "com.other.app"));
        assert!(matches("a*b*c", "a123b456c"));
        assert!(!matches("a*b*c", "a123c"));
        assert!(!matches("ab*ba", "aba"));
        assert!(matches("exact", "exact"));
        assert!(!matches("exact", "exactly"));
    }

    #[test]
    fn expands_stars_one_level_at_a_time() {
        let (dir, roots) = home();
        let caches = dir.path().join("Library/Caches");
        write(&caches.join("com.example.one/data/file"), 10);
        write(&caches.join("com.example.two/other/file"), 10);
        write(&caches.join("org.else/data/file"), 10);

        let found = expand(&roots.home().join("Library/Caches/com.example.*/data"));

        assert_eq!(
            found,
            [roots.home().join("Library/Caches/com.example.one/data")]
        );
        let found = expand(&roots.home().join("Library/Caches/missing/*"));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn stars_skip_finder_view_settings() {
        let (dir, roots) = home();
        let caches = dir.path().join("Library/Caches/pip");
        write(&caches.join(".DS_Store"), 10);

        let found = expand(&roots.home().join("Library/Caches/pip/*"));
        assert!(found.is_empty(), "{found:?}");

        write(&caches.join("http/file"), 10);
        let found = expand(&roots.home().join("Library/Caches/pip/*"));
        assert_eq!(found, [roots.home().join("Library/Caches/pip/http")]);
    }

    #[test]
    fn plans_items_with_sizes_and_selects_only_safe_rules() {
        let (dir, roots) = home();
        let app = dir.path().join("Library/Caches/com.example.app");
        write(&app.join("a"), 5000);
        write(&app.join("nested/b"), 5000);
        write(&dir.path().join("Library/Logs/app.log"), 100);
        let rules = [
            rule(
                "app",
                Tier::Safe,
                &["Library/Caches/com.example.app/*"],
                &roots,
            ),
            rule("logs", Tier::Caution, &["Library/Logs/*"], &roots),
        ];

        let plan = plan(&rules, &roots, SystemTime::now());

        let app_plan = &plan.rules[0];
        assert!(app_plan.selected);
        assert_eq!(app_plan.items.len(), 2);
        assert!(app_plan.size() >= 10_000);
        assert!(!plan.rules[1].selected);
        assert_eq!(plan.rules[1].items.len(), 1);
        assert_eq!(plan.selected_count(), 2);
        assert_eq!(plan.selected_size(), app_plan.size());
    }

    #[test]
    fn skips_items_changed_within_the_minimum_age() {
        let (dir, roots) = home();
        let logs = dir.path().join("Library/Logs");
        write(&logs.join("old.log"), 10);
        age(&logs.join("old.log"), 30);
        // The folder is old, but a file inside it changed today.
        write(&logs.join("app/recent.log"), 10);
        age(&logs.join("app/recent.log"), 0);
        let mut logs_rule = rule("logs", Tier::Caution, &["Library/Logs/*"], &roots);
        logs_rule.min_age_days = 7;

        let plan = plan(&[logs_rule], &roots, SystemTime::now());

        let logs_plan = &plan.rules[0];
        assert_eq!(logs_plan.items.len(), 1);
        assert!(logs_plan.items[0].path.path().ends_with("old.log"));
        assert!(matches!(logs_plan.skipped[0].reason, SkipReason::TooNew));
    }

    #[test]
    fn skips_links_that_lead_out() {
        let (dir, roots) = home();
        write(&dir.path().join("Documents/secret"), 10);
        symlink(
            dir.path().join("Documents/secret"),
            dir.path().join("Library/Caches/link"),
        )
        .expect("link should be created");

        let plan = plan(
            &[rule("all", Tier::Safe, &["Library/Caches/*"], &roots)],
            &roots,
            SystemTime::now(),
        );

        assert!(plan.rules[0].items.is_empty());
        assert!(matches!(
            plan.rules[0].skipped[0].reason,
            SkipReason::Refused(_)
        ));
    }

    #[test]
    fn an_item_goes_to_the_first_rule_that_finds_it() {
        let (dir, roots) = home();
        write(&dir.path().join("Library/Caches/app/file"), 10);
        let rules = [
            rule("wide", Tier::Caution, &["Library/Caches/*"], &roots),
            rule("narrow", Tier::Safe, &["Library/Caches/app/*"], &roots),
            rule("same", Tier::Safe, &["Library/Caches/app"], &roots),
        ];

        let plan = plan(&rules, &roots, SystemTime::now());

        assert_eq!(plan.rules[0].items.len(), 1);
        for later in &plan.rules[1..] {
            assert!(later.items.is_empty());
            assert!(matches!(later.skipped[0].reason, SkipReason::Overlaps));
        }
    }

    #[test]
    fn counts_a_hard_link_once_in_the_whole_plan() {
        let (dir, roots) = home();
        let caches = dir.path().join("Library/Caches");
        write(&caches.join("one/file"), 8192);
        fs::create_dir(caches.join("two")).expect("folder should be created");
        fs::hard_link(caches.join("one/file"), caches.join("two/file"))
            .expect("hard link should be created");

        let plan = plan(
            &[rule("all", Tier::Safe, &["Library/Caches/*/file"], &roots)],
            &roots,
            SystemTime::now(),
        );

        assert_eq!(plan.rules[0].items.len(), 2);
        let sizes: Vec<u64> = plan.rules[0].items.iter().map(|item| item.size).collect();
        assert!(sizes.contains(&0), "{sizes:?}");
    }

    #[test]
    fn bundled_rules_plan_without_changing_any_file() {
        let (dir, roots) = home();
        for file in [
            "Library/Developer/Xcode/DerivedData/App-abc/Build/out",
            "Library/Caches/Homebrew/downloads/bottle.tar.gz",
            "Library/Caches/Google/Chrome/Default/Cache/data",
            ".npm/_cacache/content-v2/sha512/aa",
            "Library/Logs/old.log",
            "Library/Saved Application State/com.example.savedState/window",
            "Documents/keep.txt",
        ] {
            write(&dir.path().join(file), 100);
        }
        let set = load(&roots, None);
        let before = listing(dir.path());

        let plan = plan(&set.rules, &roots, SystemTime::now() + DAY * 60);

        assert_eq!(listing(dir.path()), before);
        let items: Vec<_> = plan.rules.iter().flat_map(|rule| &rule.items).collect();
        assert_eq!(items.len(), 6);
        for item in items {
            assert!(!item.path.path().starts_with(roots.home().join("Documents")));
            assert!(roots.validate_deletable(item.path.path()).is_ok());
        }
    }

    /// A home folder with one planned item in a `safe` rule, and one in a
    /// `caution` rule nobody selected.
    fn planned_home() -> (TempDir, CleanupRoots, super::Plan) {
        let (dir, roots) = home();
        write(&dir.path().join("Library/Caches/app/file"), 100);
        write(&dir.path().join("Library/Logs/app.log"), 100);
        age(&dir.path().join("Library/Logs/app.log"), 30);
        let mut app = rule("app", Tier::Safe, &["Library/Caches/app/*"], &roots);
        app.requires_quit = vec!["com.example.app".to_string()];
        let mut logs = rule("logs", Tier::Caution, &["Library/Logs/*"], &roots);
        logs.min_age_days = 7;
        let plan = plan(&[app, logs], &roots, SystemTime::now());
        (dir, roots, plan)
    }

    #[test]
    fn moves_only_the_selected_rules() {
        let (_dir, roots, plan) = planned_home();
        let moved = RefCell::new(Vec::new());
        let mut steps = Vec::new();

        let outcome = run(
            &plan,
            &roots,
            SystemTime::now(),
            |_| false,
            |path| {
                moved.borrow_mut().push(path.to_path_buf());
                Ok(())
            },
            |done, total| steps.push((done, total)),
        );

        assert_eq!(
            *moved.borrow(),
            [roots.home().join("Library/Caches/app/file")]
        );
        assert_eq!(outcome.moved.len(), 1);
        assert_eq!(outcome.moved_size(), plan.selected_size());
        assert!(outcome.skipped.is_empty());
        assert_eq!(steps, [(1, 1)]);
    }

    #[test]
    fn skips_a_rule_whose_app_is_open() {
        let (_dir, roots, plan) = planned_home();

        let outcome = run(
            &plan,
            &roots,
            SystemTime::now(),
            |app| app == "com.example.app",
            |_| panic!("nothing should move"),
            |_, _| {},
        );

        assert!(outcome.moved.is_empty());
        assert!(matches!(
            &outcome.skipped[0].reason,
            SkipReason::AppOpen(app) if app == "com.example.app"
        ));
    }

    #[test]
    fn skips_a_file_replaced_after_the_review() {
        let (dir, roots, plan) = planned_home();
        let file = dir.path().join("Library/Caches/app/file");
        // Make the new file first, so the old number cannot be reused.
        write(&dir.path().join("Library/Caches/app/new"), 100);
        fs::rename(dir.path().join("Library/Caches/app/new"), &file)
            .expect("file should be replaced");

        let outcome = run(
            &plan,
            &roots,
            SystemTime::now(),
            |_| false,
            |_| panic!("nothing should move"),
            |_, _| {},
        );

        assert!(matches!(outcome.skipped[0].reason, SkipReason::Replaced));
    }

    #[test]
    fn skips_a_link_swapped_in_after_the_review() {
        let (dir, roots, plan) = planned_home();
        let file = dir.path().join("Library/Caches/app/file");
        write(&dir.path().join("Documents/secret"), 10);
        fs::remove_file(&file).expect("file should be removed");
        symlink(dir.path().join("Documents/secret"), &file).expect("link should be created");

        let outcome = run(
            &plan,
            &roots,
            SystemTime::now(),
            |_| false,
            |_| panic!("nothing should move"),
            |_, _| {},
        );

        assert!(matches!(outcome.skipped[0].reason, SkipReason::Refused(_)));
    }

    #[test]
    fn skips_an_item_changed_after_the_review() {
        let (dir, roots, mut plan) = planned_home();
        plan.rules[0].selected = false;
        plan.rules[1].selected = true;
        write(&dir.path().join("Library/Logs/app.log"), 200);

        let outcome = run(
            &plan,
            &roots,
            SystemTime::now(),
            |_| false,
            |_| panic!("nothing should move"),
            |_, _| {},
        );

        assert!(matches!(outcome.skipped[0].reason, SkipReason::TooNew));
    }

    #[test]
    fn reports_a_move_that_failed() {
        let (_dir, roots, plan) = planned_home();

        let outcome = run(
            &plan,
            &roots,
            SystemTime::now(),
            |_| false,
            |_| Err(io::Error::other("Finder said no")),
            |_, _| {},
        );

        assert!(outcome.moved.is_empty());
        assert!(matches!(
            outcome.skipped[0].reason,
            SkipReason::MoveFailed(_)
        ));
    }

    #[test]
    fn leaves_links_inside_the_root_out_of_the_plan() {
        let (dir, roots) = home();
        write(&dir.path().join("Library/Caches/real/file"), 10);
        symlink(
            dir.path().join("Library/Caches/real"),
            dir.path().join("Library/Caches/link"),
        )
        .expect("link should be created");

        let plan = plan(
            &[rule("all", Tier::Safe, &["Library/Caches/*"], &roots)],
            &roots,
            SystemTime::now(),
        );

        assert_eq!(plan.rules[0].items.len(), 1);
        assert!(matches!(plan.rules[0].skipped[0].reason, SkipReason::Link));
    }

    #[test]
    fn plans_one_picked_item_or_says_why_not() {
        let (dir, roots) = home();
        write(&dir.path().join("Library/Caches/app/file"), 100);
        write(&dir.path().join("Documents/keep.txt"), 100);
        let now = SystemTime::now();

        let plan = super::plan_path(&dir.path().join("Library/Caches/app"), &roots, now)
            .expect("a cache folder should be planned");
        assert!(plan.rules[0].selected);
        assert_eq!(plan.selected_count(), 1);

        for refused in [
            dir.path().join("Documents/keep.txt"),
            dir.path().join("Library/Caches"),
            dir.path().to_path_buf(),
        ] {
            assert!(matches!(
                super::plan_path(&refused, &roots, now),
                Err(SkipReason::Refused(_))
            ));
        }
        assert!(super::plan_path(&dir.path().join("Library/Caches/missing"), &roots, now).is_err());

        // A name with a `*` must not match its neighbours.
        write(&dir.path().join("Library/Caches/a*"), 10);
        write(&dir.path().join("Library/Caches/abc"), 10);
        assert!(matches!(
            super::plan_path(&dir.path().join("Library/Caches/a*"), &roots, now),
            Err(SkipReason::Refused(_))
        ));
    }

    #[test]
    fn a_copied_skip_reason_keeps_its_error() {
        let reason = super::SkipReason::Unreadable(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "no access",
        ));

        let super::SkipReason::Unreadable(copy) = reason.clone() else {
            panic!("the copy should be the same kind of reason");
        };
        assert_eq!(copy.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(copy.to_string(), "no access");
    }
}
