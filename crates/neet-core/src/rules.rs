//! Cleanup rules: short TOML entries that say which files neet may clean, and
//! how risky that is. See the rule format in `docs/SAFETY.md`.
//!
//! Rules only say where to look. Every path they find still has to pass
//! [`CleanupRoots::validate_deletable`](crate::safety::CleanupRoots::validate_deletable).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::safety::CleanupRoots;

/// Rules built into neet, as `(file name, contents)`.
const BUNDLED: &[(&str, &str)] = &[
    ("developer.toml", include_str!("../rules/developer.toml")),
    ("package.toml", include_str!("../rules/package.toml")),
    ("browser.toml", include_str!("../rules/browser.toml")),
    ("logs.toml", include_str!("../rules/logs.toml")),
    ("system.toml", include_str!("../rules/system.toml")),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Developer,
    Package,
    Application,
    Browser,
    Logs,
    System,
}

/// How risky a rule is, which decides how it is selected
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// The files come back, and the only cost is time. Selected from the start.
    Safe,
    /// You may need to download, index, or sign in again. You select it.
    Caution,
    /// The files may not exist anywhere else. You select it and type a confirmation.
    Expert,
}

/// Where a rule came from
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Bundled,
    /// One of your own rule files
    User(PathBuf),
    /// An item you picked in the Disk screen
    Disk,
    /// An app, or one of its related files, in the Remove App screen
    App,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub tier: Tier,
    /// Full paths, with `~` expanded. May hold `*` within one folder level.
    pub paths: Vec<PathBuf>,
    pub description: String,
    pub regenerates: bool,
    /// Bundle IDs of apps that must be closed first
    pub requires_quit: Vec<String>,
    /// Skip items changed within this many days
    pub min_age_days: u32,
    pub source: Source,
}

/// A rule, or a rule file, that could not be loaded
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleError {
    /// The file the problem is in
    pub file: String,
    /// The rule's `id`, when the problem is in one rule
    pub rule: Option<String>,
    pub message: String,
}

/// The rules that loaded, and what went wrong with the ones that did not
#[derive(Debug, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
    pub errors: Vec<RuleError>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    #[serde(default)]
    rule: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    name: String,
    category: Category,
    tier: Tier,
    paths: Vec<String>,
    description: String,
    #[serde(default)]
    regenerates: bool,
    #[serde(default)]
    requires_quit: Vec<String>,
    #[serde(default)]
    min_age_days: u32,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_bundle_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// Expands `~` and checks a rule path: `~` only at the start, `*` as the only
/// pattern, no `.` or `..` parts, and inside a cleanup root.
fn check_path(path: &str, roots: &CleanupRoots) -> Result<PathBuf, String> {
    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        roots.home().join(rest)
    } else if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        return Err(format!("`{path}` must start with `~/` or `/`"));
    };
    if expanded.to_string_lossy().contains('~') {
        return Err(format!("`{path}` may only use `~` at the start"));
    }
    if path.contains("**") || path.contains(['?', '[', ']', '{', '}']) {
        return Err(format!("`{path}` uses a pattern other than `*`"));
    }
    if path.split('/').any(|part| part == "." || part == "..") {
        return Err(format!("`{path}` has a `.` or `..` part"));
    }
    if !roots.covers_pattern(&expanded) {
        return Err(format!(
            "`{path}` is not inside a cleanup root, or has a `*` before the end of one"
        ));
    }
    Ok(expanded)
}

fn check_rule(raw: RawRule, source: &Source, roots: &CleanupRoots) -> Result<Rule, String> {
    if !valid_id(&raw.id) {
        return Err("`id` must be lowercase letters, numbers, and hyphens".to_string());
    }
    if raw.name.trim().is_empty() {
        return Err("`name` is empty".to_string());
    }
    if raw.description.trim().is_empty() {
        return Err("`description` is empty".to_string());
    }
    if raw.paths.is_empty() {
        return Err("`paths` is empty".to_string());
    }
    if let Some(id) = raw.requires_quit.iter().find(|id| !valid_bundle_id(id)) {
        return Err(format!("`{id}` in `requires_quit` is not a bundle ID"));
    }
    let paths = raw
        .paths
        .iter()
        .map(|path| check_path(path, roots))
        .collect::<Result<Vec<_>, _>>()?;

    // Your own rules are never selected from the start.
    let tier = match (source, raw.tier) {
        (Source::User(_), Tier::Safe) => Tier::Caution,
        (_, tier) => tier,
    };

    Ok(Rule {
        id: raw.id,
        name: raw.name,
        category: raw.category,
        tier,
        paths,
        description: raw.description,
        regenerates: raw.regenerates,
        requires_quit: raw.requires_quit,
        min_age_days: raw.min_age_days,
        source: source.clone(),
    })
}

/// Parses one rule file. Rules with problems are left out and reported.
fn parse(text: &str, file: &str, source: &Source, roots: &CleanupRoots) -> RuleSet {
    let mut set = RuleSet::default();
    let parsed: RuleFile = match toml::from_str(text) {
        Ok(parsed) => parsed,
        Err(error) => {
            set.errors.push(RuleError {
                file: file.to_string(),
                rule: None,
                message: error.message().to_string(),
            });
            return set;
        }
    };

    let mut seen = HashSet::new();
    for raw in parsed.rule {
        let id = raw.id.clone();
        let result = if seen.insert(id.clone()) {
            check_rule(raw, source, roots)
        } else {
            Err("another rule in this file has the same `id`".to_string())
        };
        match result {
            Ok(rule) => set.rules.push(rule),
            Err(message) => set.errors.push(RuleError {
                file: file.to_string(),
                rule: Some(id),
                message,
            }),
        }
    }
    set
}

/// Adds `incoming` to `set`. A rule with the same `id` as one already there
/// replaces it.
fn merge(set: &mut RuleSet, incoming: RuleSet) {
    for rule in incoming.rules {
        if let Some(existing) = set.rules.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule;
        } else {
            set.rules.push(rule);
        }
    }
    set.errors.extend(incoming.errors);
}

/// Loads the bundled rules, then your own `.toml` files from `user_dir`, if
/// it exists. Your rule replaces a bundled rule with the same `id`.
#[must_use]
pub fn load(roots: &CleanupRoots, user_dir: Option<&Path>) -> RuleSet {
    let mut set = RuleSet::default();
    for (file, text) in BUNDLED {
        merge(&mut set, parse(text, file, &Source::Bundled, roots));
    }

    let Some(user_dir) = user_dir else {
        return set;
    };
    let Ok(entries) = fs::read_dir(user_dir) else {
        // No folder of your own rules is fine.
        return set;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    for path in files {
        let file = path.display().to_string();
        match fs::read_to_string(&path) {
            Ok(text) => merge(
                &mut set,
                parse(&text, &file, &Source::User(path.clone()), roots),
            ),
            Err(error) => set.errors.push(RuleError {
                file,
                rule: None,
                message: error.to_string(),
            }),
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::{BUNDLED, Category, Source, Tier, load, parse};
    use crate::safety::CleanupRoots;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::{TempDir, tempdir};

    fn roots() -> (TempDir, CleanupRoots) {
        let dir = tempdir().expect("temporary directory should be created");
        let roots = CleanupRoots::new(dir.path()).expect("roots should be made");
        (dir, roots)
    }

    const GOOD: &str = r#"
        [[rule]]
        id = "xcode-derived-data"
        name = "Xcode DerivedData"
        category = "developer"
        tier = "safe"
        paths = ["~/Library/Developer/Xcode/DerivedData/*"]
        description = "Build files. Xcode makes them again during the next build."
        regenerates = true
        requires_quit = ["com.apple.dt.Xcode"]
        min_age_days = 2
    "#;

    fn rule_with(field: &str) -> String {
        format!(
            r#"
            [[rule]]
            id = "example"
            name = "Example"
            category = "application"
            tier = "caution"
            description = "Example cache."
            {field}
            "#
        )
    }

    fn only_error(text: &str, roots: &CleanupRoots) -> String {
        let set = parse(text, "test.toml", &Source::Bundled, roots);
        assert!(set.rules.is_empty(), "rule should be refused");
        assert_eq!(set.errors.len(), 1);
        set.errors[0].message.clone()
    }

    #[test]
    fn parses_a_rule() {
        let (_dir, roots) = roots();

        let set = parse(GOOD, "developer.toml", &Source::Bundled, &roots);

        assert!(set.errors.is_empty(), "{:?}", set.errors);
        let rule = &set.rules[0];
        assert_eq!(rule.id, "xcode-derived-data");
        assert_eq!(rule.category, Category::Developer);
        assert_eq!(rule.tier, Tier::Safe);
        assert_eq!(
            rule.paths,
            [roots.home().join("Library/Developer/Xcode/DerivedData/*")]
        );
        assert!(rule.regenerates);
        assert_eq!(rule.requires_quit, ["com.apple.dt.Xcode"]);
        assert_eq!(rule.min_age_days, 2);
    }

    #[test]
    fn refuses_unknown_fields_and_bad_values() {
        let (_dir, roots) = roots();
        let path = r#"paths = ["~/Library/Caches/com.example"]"#;

        assert!(
            only_error(&rule_with(&format!("{path}\ncolour = \"red\"")), &roots).contains("colour")
        );
        assert!(
            only_error(
                &rule_with(path).replace(r#"tier = "caution""#, r#"tier = "reckless""#),
                &roots
            )
            .contains("reckless")
        );
        assert!(
            only_error(
                &rule_with(path).replace(r#"id = "example""#, r#"id = "Bad_ID""#),
                &roots
            )
            .contains("`id`")
        );
        assert!(only_error(&rule_with("paths = []"), &roots).contains("`paths` is empty"));
    }

    #[test]
    fn refuses_patterns_other_than_a_star() {
        let (_dir, roots) = roots();

        for pattern in [
            "~/Library/Caches/**",
            "~/Library/Caches/app?",
            "~/Library/Caches/[ab]",
            "~/Library/Caches/{a,b}",
        ] {
            let message = only_error(&rule_with(&format!(r#"paths = ["{pattern}"]"#)), &roots);
            assert!(
                message.contains("pattern other than"),
                "{pattern}: {message}"
            );
        }
    }

    #[test]
    fn refuses_paths_outside_the_roots() {
        let (_dir, roots) = roots();

        for path in [
            "~/Documents/*",
            "~/Library/*/Caches",
            "~/Library/Caches",
            "~/Library/Developer/Xcode/Archives/*",
            "/tmp/cache",
            "Library/Caches/x",
            "~/Library/Caches/../Documents",
            "~/Library/Caches/~backup",
        ] {
            let set = parse(
                &rule_with(&format!(r#"paths = ["{path}"]"#)),
                "test.toml",
                &Source::Bundled,
                &roots,
            );
            assert!(set.rules.is_empty(), "{path} should be refused");
        }
    }

    #[test]
    fn a_bad_rule_does_not_stop_the_others() {
        let (_dir, roots) = roots();
        let text = format!("{GOOD}\n{}", rule_with(r#"paths = ["~/Documents/*"]"#));

        let set = parse(&text, "mixed.toml", &Source::Bundled, &roots);

        assert_eq!(set.rules.len(), 1);
        assert_eq!(set.errors.len(), 1);
        assert_eq!(set.errors[0].rule.as_deref(), Some("example"));
    }

    #[test]
    fn refuses_a_repeated_id_in_one_file() {
        let (_dir, roots) = roots();

        let set = parse(
            &format!("{GOOD}\n{GOOD}"),
            "twice.toml",
            &Source::Bundled,
            &roots,
        );

        assert_eq!(set.rules.len(), 1);
        assert!(set.errors[0].message.contains("same `id`"));
    }

    #[test]
    fn your_own_rules_are_never_selected_from_the_start() {
        let (dir, roots) = roots();
        let user_dir = dir.path().join("rules");
        fs::create_dir(&user_dir).expect("rules folder should be created");
        fs::write(user_dir.join("mine.toml"), GOOD).expect("rule file should be written");
        fs::write(user_dir.join("notes.txt"), "not a rule").expect("other file should be written");

        let set = load(&roots, Some(&user_dir));

        let rule = set
            .rules
            .iter()
            .find(|rule| rule.id == "xcode-derived-data")
            .expect("your rule should load");
        assert_eq!(rule.tier, Tier::Caution);
        assert!(matches!(rule.source, Source::User(_)));
        assert!(set.errors.is_empty(), "{:?}", set.errors);
    }

    #[test]
    fn every_bundled_rule_loads() {
        let (_dir, roots) = roots();

        let set = load(&roots, None);

        assert!(set.errors.is_empty(), "{:?}", set.errors);
        let ids: HashSet<_> = set.rules.iter().map(|rule| rule.id.as_str()).collect();
        assert_eq!(ids.len(), set.rules.len(), "bundled rule IDs must differ");
        assert_eq!(set.rules.len(), 14);
        assert!(set.rules.iter().all(|rule| rule.source == Source::Bundled));
    }

    #[test]
    fn bundled_download_caches_are_never_safe() {
        let (_dir, roots) = roots();

        let set = load(&roots, None);

        let safe: HashSet<_> = set
            .rules
            .iter()
            .filter(|rule| rule.tier == Tier::Safe)
            .map(|rule| rule.id.as_str())
            .collect();
        assert_eq!(
            safe,
            HashSet::from(["xcode-derived-data", "simulator-caches"])
        );
        assert!(
            set.rules
                .iter()
                .filter(|rule| rule.category == Category::Package)
                .all(|rule| rule.tier == Tier::Caution)
        );
    }

    #[test]
    fn bundled_files_have_rules() {
        let (_dir, roots) = roots();

        for (file, text) in BUNDLED {
            let set = parse(text, file, &Source::Bundled, &roots);
            assert!(!set.rules.is_empty(), "{file} has no rules");
        }
    }

    #[test]
    fn a_missing_rules_folder_is_fine() {
        let (dir, roots) = roots();

        let set = load(&roots, Some(&dir.path().join("missing")));

        assert!(set.errors.is_empty(), "{:?}", set.errors);
    }

    #[test]
    fn a_broken_file_is_reported() {
        let (dir, roots) = roots();
        let user_dir = dir.path().join("rules");
        fs::create_dir(&user_dir).expect("rules folder should be created");
        fs::write(user_dir.join("broken.toml"), "[[rule]\nid = ").expect("file should be written");

        let set = load(&roots, Some(&user_dir));

        assert_eq!(set.errors.len(), 1);
        assert!(set.errors[0].file.ends_with("broken.toml"));
        assert_eq!(set.errors[0].rule, None);
    }
}
