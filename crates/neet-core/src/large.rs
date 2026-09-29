//! Finds large and old files in a scan. Finding a file never makes it a
//! cleanup target.

use std::time::{Duration, SystemTime};

use crate::tree::{NodeId, NodeKind, Tree};

/// Which files to find
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    /// Only files using at least this much space on disk
    pub min_size: u64,
    /// Only files not changed for at least this long. A file whose change
    /// time is unknown never counts as old.
    pub unchanged_for: Option<Duration>,
}

/// The files that match `filter`, largest first. Files with the same size
/// keep the order the scan found them in.
#[must_use]
pub fn find(tree: &Tree, filter: Filter, now: SystemTime) -> Vec<NodeId> {
    let changed_before = filter
        .unchanged_for
        .map(|age| now.checked_sub(age).unwrap_or(SystemTime::UNIX_EPOCH));
    let mut found: Vec<(NodeId, u64)> = tree
        .iter()
        .filter(|(_, node)| node.kind == NodeKind::File && node.own_size >= filter.min_size)
        .filter(|(_, node)| match changed_before {
            None => true,
            Some(before) => node.modified.is_some_and(|modified| modified <= before),
        })
        .map(|(id, node)| (id, node.own_size))
        .collect();
    found.sort_by_key(|&(_, size)| std::cmp::Reverse(size));
    found.into_iter().map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::{Filter, find};
    use crate::tree::{NodeKind, Tree};
    use std::time::{Duration, SystemTime};

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    #[test]
    fn finds_large_files_largest_first() {
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let folder = tree.add(root, "big", NodeKind::Directory, 5_000);
        let _small = tree.add(folder, "small.bin", NodeKind::File, 10);
        let large = tree.add(folder, "large.bin", NodeKind::File, 900);
        let larger = tree.add(root, "larger.bin", NodeKind::File, 1_000);

        let found = find(
            &tree,
            Filter {
                min_size: 100,
                unchanged_for: None,
            },
            SystemTime::now(),
        );

        assert_eq!(
            found,
            [larger, large],
            "folders and small files are left out"
        );
    }

    #[test]
    fn finds_old_files_and_skips_unknown_times() {
        let now = SystemTime::now();
        let mut tree = Tree::new("/Users/test");
        let root = tree.root();
        let old = tree.add(root, "old.bin", NodeKind::File, 100);
        tree.set_modified(old, now - DAY * 400);
        let recent = tree.add(root, "recent.bin", NodeKind::File, 100);
        tree.set_modified(recent, now - DAY * 3);
        let _unknown = tree.add(root, "unknown.bin", NodeKind::File, 100);

        let found = find(
            &tree,
            Filter {
                min_size: 0,
                unchanged_for: Some(DAY * 365),
            },
            now,
        );

        assert_eq!(found, [old]);
    }
}
