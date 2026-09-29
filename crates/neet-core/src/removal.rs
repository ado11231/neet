//! App removal: the apps you may remove, the files that belong to each, and
//! a plan the cleanup steps can run. See App Removal in `docs/SAFETY.md`.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::clean::{Plan, PlanItem, RulePlan, SkipReason, Skipped, measure};
use crate::rules::{Category, Rule, Source, Tier};
use crate::safety::CleanupRoots;
use crate::size::HardLinkTracker;

/// Why an app cannot be removed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Its bundle ID starts with `com.apple.`
    AppleApp,
    /// A symbolic link, such as Safari in `/Applications`
    Link,
    /// Its `Info.plist` has no bundle ID neet could read
    NoBundleId,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AppleApp => write!(f, "Apple app"),
            Self::Link => write!(f, "link"),
            Self::NoBundleId => write!(f, "no bundle ID"),
        }
    }
}

/// An app in one of the Applications folders
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    pub path: PathBuf,
    /// The file name without `.app`
    pub name: String,
    pub bundle_id: Option<String>,
    /// `CFBundleName`, when it differs from `name`
    pub bundle_name: Option<String>,
    pub refused: Option<Refusal>,
}

/// Reads one key from an app's `Info.plist` with `plutil`, which ships with
/// macOS.
fn info(app: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

fn describe(path: PathBuf, read_info: &impl Fn(&Path, &str) -> Option<String>) -> App {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    let bundle_id = if is_link {
        None
    } else {
        read_info(&path, "CFBundleIdentifier")
    };
    let bundle_name = if is_link {
        None
    } else {
        read_info(&path, "CFBundleName").filter(|bundle_name| *bundle_name != name)
    };
    let refused = if is_link {
        Some(Refusal::Link)
    } else {
        match &bundle_id {
            None => Some(Refusal::NoBundleId),
            Some(id) if id.starts_with("com.apple.") => Some(Refusal::AppleApp),
            Some(_) => None,
        }
    };
    App {
        path,
        name,
        bundle_id,
        bundle_name,
        refused,
    }
}

/// Every `.app` directly inside the Applications folders, by name, with why
/// any of them cannot be removed.
#[must_use]
pub fn list_apps(roots: &CleanupRoots) -> Vec<App> {
    list_apps_with(roots, &info)
}

fn list_apps_with(
    roots: &CleanupRoots,
    read_info: &impl Fn(&Path, &str) -> Option<String>,
) -> Vec<App> {
    let mut apps: Vec<App> = roots
        .application_folders()
        .iter()
        .filter_map(|folder| fs::read_dir(folder).ok())
        .flat_map(Iterator::flatten)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
        .map(|path| describe(path, read_info))
        .collect();
    apps.sort_by_cached_key(|app| app.name.to_lowercase());
    apps
}

/// What a related file is marked with, and why it starts unselected
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Nothing to warn about. Starts selected.
    None,
    MayBeYourData,
    Settings,
    SharedWithOtherApps,
    StartsOnItsOwn,
    MatchedByName,
}

impl Mark {
    #[must_use]
    pub fn selected(self) -> bool {
        self == Self::None
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::MayBeYourData => "may be your data",
            Self::Settings => "settings",
            Self::SharedWithOtherApps => "shared with other apps",
            Self::StartsOnItsOwn => "starts on its own",
            Self::MatchedByName => "matched by name",
        }
    }
}

/// A file that belongs to an app
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Related {
    pub path: PathBuf,
    /// The `~/Library` folder it was found in
    pub folder: &'static str,
    pub mark: Mark,
}

/// The exact names to look for in each folder, from the table in SAFETY.md.
/// `{id}` is the bundle ID.
const BY_ID: &[(&str, &[&str], Mark)] = &[
    ("Caches", &["{id}"], Mark::None),
    ("Logs", &["{id}"], Mark::None),
    ("Saved Application State", &["{id}.savedState"], Mark::None),
    ("HTTPStorages", &["{id}", "{id}.binarycookies"], Mark::None),
    ("WebKit", &["{id}"], Mark::None),
    ("Application Support", &["{id}"], Mark::MayBeYourData),
    ("Containers", &["{id}"], Mark::MayBeYourData),
    ("Preferences", &["{id}.plist"], Mark::Settings),
    ("LaunchAgents", &["{id}.plist"], Mark::StartsOnItsOwn),
];

/// Folders also searched by the app's name
const BY_NAME: &[&str] = &["Application Support", "Logs"];

/// Whether `name` is `group.<id>`, or a 10 character team ID then `.<id>`.
fn is_group_container(name: &str, id: &str) -> bool {
    if name.strip_prefix("group.") == Some(id) {
        return true;
    }
    match name.split_once('.') {
        Some((team, rest)) => {
            rest == id
                && team.len() == 10
                && team
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        }
        None => false,
    }
}

/// The entry in `folder` whose name is `wanted`, compared the way the disk
/// does: APFS ignores case by default.
fn find_entry(folder: &Path, wanted: &str) -> Option<PathBuf> {
    fs::read_dir(folder)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        })
        .map(|entry| entry.path())
}

/// The files in `~/Library` that belong to `app`, found only by exact name.
#[must_use]
pub fn related(roots: &CleanupRoots, app: &App) -> Vec<Related> {
    let Some(id) = &app.bundle_id else {
        return Vec::new();
    };
    let library = roots.home().join("Library");
    let mut found: Vec<Related> = Vec::new();
    let mut add = |path: PathBuf, folder: &'static str, mark: Mark| {
        if !found.iter().any(|seen| seen.path == path) {
            found.push(Related { path, folder, mark });
        }
    };

    for (folder, patterns, mark) in BY_ID {
        for pattern in *patterns {
            let wanted = pattern.replace("{id}", id);
            if let Some(path) = find_entry(&library.join(folder), &wanted) {
                add(path, folder, *mark);
            }
        }
    }
    if let Ok(entries) = fs::read_dir(library.join("Group Containers")) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| is_group_container(name, id))
            {
                add(entry.path(), "Group Containers", Mark::SharedWithOtherApps);
            }
        }
    }
    let names = std::iter::once(&app.name).chain(app.bundle_name.as_ref());
    for name in names {
        for folder in BY_NAME {
            if let Some(path) = find_entry(&library.join(folder), name) {
                add(path, folder, Mark::MatchedByName);
            }
        }
    }
    found
}

fn item_rule(id: usize, name: String, path: &Path, mark: Mark, bundle_id: &str) -> Rule {
    Rule {
        id: format!("app-item-{id}"),
        name,
        category: Category::Application,
        tier: Tier::Caution,
        paths: vec![path.to_path_buf()],
        description: mark.label().to_string(),
        regenerates: false,
        requires_quit: vec![bundle_id.to_string()],
        min_age_days: 0,
        source: Source::App,
    }
}

/// A plan to remove `app` and its related files, one entry per item, each
/// selected as SAFETY.md says. Every item passes the app removal check.
///
/// # Errors
///
/// Returns why the app cannot be removed.
pub fn plan(roots: &CleanupRoots, app: &App) -> Result<Plan, Refusal> {
    if let Some(refusal) = app.refused {
        return Err(refusal);
    }
    let bundle_id = app.bundle_id.clone().ok_or(Refusal::NoBundleId)?;
    let mut targets = vec![(app.path.clone(), "The app".to_string(), Mark::None)];
    targets.extend(related(roots, app).into_iter().map(|related| {
        let label = match related.mark {
            Mark::None => related.folder.to_string(),
            mark => format!("{} ({})", related.folder, mark.label()),
        };
        (related.path, label, related.mark)
    }));

    let mut tracker = HardLinkTracker::new();
    let mut plan = Plan::default();
    for (index, (path, label, mark)) in targets.into_iter().enumerate() {
        let rule = item_rule(index, label, &path, mark, &bundle_id);
        let mut items = Vec::new();
        let mut skipped = Vec::new();
        match roots.validate_app_removal(&path) {
            Err(error) => skipped.push(Skipped {
                path,
                reason: SkipReason::Refused(error),
            }),
            Ok(validated) => match measure(validated.path(), &mut tracker) {
                Err(error) => skipped.push(Skipped {
                    path,
                    reason: SkipReason::Unreadable(error),
                }),
                Ok((size, changed)) => items.push(PlanItem {
                    path: validated,
                    size,
                    changed,
                }),
            },
        }
        plan.rules.push(RulePlan {
            selected: mark.selected() && !items.is_empty(),
            rule,
            items,
            skipped,
        });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::{App, Mark, Refusal, is_group_container, list_apps_with, plan, related};
    use crate::safety::CleanupRoots;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};

    fn fake_info(app: &Path, key: &str) -> Option<String> {
        let name = app.file_stem()?.to_str()?;
        match (name, key) {
            ("Discord", "CFBundleIdentifier") => Some("com.hnc.Discord".to_string()),
            ("Visual Studio Code", "CFBundleIdentifier") => {
                Some("com.microsoft.VSCode".to_string())
            }
            ("Visual Studio Code", "CFBundleName") => Some("Code".to_string()),
            ("Notes", "CFBundleIdentifier") => Some("com.apple.Notes".to_string()),
            _ => None,
        }
    }

    fn setup() -> (TempDir, TempDir, CleanupRoots) {
        let home = tempdir().expect("home should be created");
        let shared = tempdir().expect("applications should be created");
        for app in ["Discord", "Visual Studio Code", "Notes", "Broken"] {
            fs::create_dir_all(shared.path().join(format!("{app}.app/Contents")))
                .expect("app should be created");
        }
        fs::create_dir_all(shared.path().join("Utilities/Tool.app"))
            .expect("nested app should be created");
        symlink(
            shared.path().join("Notes.app"),
            shared.path().join("Safari.app"),
        )
        .expect("link should be created");
        let library = home.path().join("Library");
        for folder in [
            "Caches/com.hnc.Discord",
            "Caches/com.hnc.Discord.helper",
            "Application Support/discord",
            "Application Support/Code",
            "HTTPStorages/com.hnc.Discord",
            "Group Containers/group.com.hnc.Discord",
            "Group Containers/ABCDE12345.com.hnc.Discord",
            "Group Containers/ABC.com.hnc.Discord",
            "Containers/com.microsoft.VSCode",
            "Logs",
        ] {
            fs::create_dir_all(library.join(folder)).expect("folder should be created");
        }
        fs::create_dir_all(library.join("Preferences")).expect("folder should be created");
        fs::write(library.join("Preferences/com.hnc.Discord.plist"), "x")
            .expect("file should be written");
        fs::write(library.join("Caches/com.hnc.Discord/data"), vec![1; 5000])
            .expect("file should be written");
        let roots = CleanupRoots::with_applications(home.path(), shared.path())
            .expect("roots should be made");
        (home, shared, roots)
    }

    fn app(roots: &CleanupRoots, name: &str) -> App {
        list_apps_with(roots, &fake_info)
            .into_iter()
            .find(|app| app.name == name)
            .expect("app should be listed")
    }

    #[test]
    fn lists_apps_directly_in_the_folders_with_refusals() {
        let (_home, _shared, roots) = setup();

        let apps = list_apps_with(&roots, &fake_info);
        let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();

        assert_eq!(
            names,
            ["Broken", "Discord", "Notes", "Safari", "Visual Studio Code"]
        );
        assert_eq!(app(&roots, "Notes").refused, Some(Refusal::AppleApp));
        assert_eq!(app(&roots, "Safari").refused, Some(Refusal::Link));
        assert_eq!(app(&roots, "Broken").refused, Some(Refusal::NoBundleId));
        assert_eq!(app(&roots, "Discord").refused, None);
        assert_eq!(
            app(&roots, "Visual Studio Code").bundle_name.as_deref(),
            Some("Code")
        );
    }

    #[test]
    fn finds_only_exact_matches_with_their_marks() {
        let (_home, _shared, roots) = setup();
        let library = roots.home().join("Library");

        let found = related(&roots, &app(&roots, "Discord"));
        let mark_of = |path: &str| {
            found
                .iter()
                .find(|related| related.path == library.join(path))
                .map(|related| related.mark)
        };

        assert_eq!(mark_of("Caches/com.hnc.Discord"), Some(Mark::None));
        assert_eq!(mark_of("HTTPStorages/com.hnc.Discord"), Some(Mark::None));
        assert_eq!(
            mark_of("Preferences/com.hnc.Discord.plist"),
            Some(Mark::Settings)
        );
        assert_eq!(
            mark_of("Group Containers/group.com.hnc.Discord"),
            Some(Mark::SharedWithOtherApps)
        );
        assert_eq!(
            mark_of("Group Containers/ABCDE12345.com.hnc.Discord"),
            Some(Mark::SharedWithOtherApps)
        );
        assert_eq!(
            mark_of("Application Support/discord"),
            Some(Mark::MatchedByName)
        );
        assert_eq!(mark_of("Caches/com.hnc.Discord.helper"), None);
        assert_eq!(mark_of("Group Containers/ABC.com.hnc.Discord"), None);
        assert_eq!(found.len(), 6);
    }

    #[test]
    fn finds_folders_named_after_the_bundle_name() {
        let (_home, _shared, roots) = setup();
        let library = roots.home().join("Library");

        let found = related(&roots, &app(&roots, "Visual Studio Code"));

        assert!(found.iter().any(|related| {
            related.path == library.join("Application Support/Code")
                && related.mark == Mark::MatchedByName
        }));
        assert!(found.iter().any(|related| {
            related.path == library.join("Containers/com.microsoft.VSCode")
                && related.mark == Mark::MayBeYourData
        }));
    }

    #[test]
    fn plans_the_app_and_selects_only_unmarked_items() {
        let (_home, _shared, roots) = setup();

        let plan = plan(&roots, &app(&roots, "Discord")).expect("app should be planned");

        assert_eq!(plan.rules[0].rule.name, "The app");
        assert!(plan.rules[0].selected);
        let selected: Vec<&str> = plan
            .rules
            .iter()
            .filter(|rule| rule.selected)
            .map(|rule| rule.rule.name.as_str())
            .collect();
        assert_eq!(selected, ["The app", "Caches", "HTTPStorages"]);
        assert!(plan.rules.iter().all(|rule| {
            rule.rule.requires_quit == ["com.hnc.Discord"] && rule.skipped.is_empty()
        }));
        assert!(plan.selected_size() >= 5000);
    }

    #[test]
    fn refuses_to_plan_a_refused_app() {
        let (_home, _shared, roots) = setup();

        assert_eq!(
            plan(&roots, &app(&roots, "Notes")).err(),
            Some(Refusal::AppleApp)
        );
        assert_eq!(
            plan(&roots, &app(&roots, "Safari")).err(),
            Some(Refusal::Link)
        );
    }

    #[test]
    fn group_containers_need_an_exact_form() {
        assert!(is_group_container("group.com.a.b", "com.a.b"));
        assert!(is_group_container("ABCDE12345.com.a.b", "com.a.b"));
        assert!(!is_group_container("abcde12345.com.a.b", "com.a.b"));
        assert!(!is_group_container("ABCDE12345.com.a.b.c", "com.a.b"));
        assert!(!is_group_container("com.a.b", "com.a.b"));
    }
}
