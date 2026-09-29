use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug)]
pub struct Node {
    pub name: OsString,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub own_size: u64,
    pub total_size: u64,
}
#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
}
impl Tree {
    /// Creates a tree that holds only the root folder.
    #[must_use]
    pub fn new(root_name: impl Into<OsString>) -> Self {
        Self {
            nodes: vec![Node {
                name: root_name.into(),
                kind: NodeKind::Directory,
                parent: None,
                children: Vec::new(),
                own_size: 0,
                total_size: 0,
            }],
        }
    }

    /// Returns the id of the root folder
    #[must_use]
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Returns the node with this id
    #[must_use]
    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    /// Returns how many nodes are in the tree, including the root
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Adds a node under a parent and adds size to every folder above it
    #[must_use]
    pub fn add(
        &mut self,
        parent: NodeId,
        name: impl Into<OsString>,
        kind: NodeKind,
        size: u64,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());

        self.nodes.push(Node {
            name: name.into(),
            kind,
            parent: Some(parent),
            children: Vec::new(),
            own_size: size,
            total_size: size,
        });

        self.nodes[parent.0].children.push(id);

        let mut current = Some(parent);

        while let Some(folder) = current {
            self.nodes[folder.0].total_size += size;
            current = self.nodes[folder.0].parent;
        }

        id
    }

    /// Returns the full path of a node, from root down
    #[must_use]
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut names = Vec::new();
        let mut current = Some(id);

        while let Some(node) = current {
            names.push(&self.nodes[node.0].name);
            current = self.nodes[node.0].parent;
        }

        names.iter().rev().collect()
    }

    /// Returns every node with its id, in order
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (NodeId(index), node))
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeKind, Tree};
    use std::path::PathBuf;

    #[test]
    fn a_new_tree_holds_only_the_root() {
        let tree = Tree::new("/scan/root");
        let root = tree.get(tree.root());

        assert_eq!(tree.node_count(), 1);
        assert_eq!(root.kind, NodeKind::Directory);
        assert_eq!(root.parent, None);
        assert!(root.children.is_empty());
        assert_eq!(root.own_size, 0);
        assert_eq!(root.total_size, 0);
        assert_eq!(tree.path(tree.root()), PathBuf::from("/scan/root"));
    }

    #[test]
    fn adding_a_node_links_parent_and_child() {
        let mut tree = Tree::new("/scan/root");
        let root = tree.root();

        let folder = tree.add(root, "folder", NodeKind::Directory, 0);
        let file = tree.add(folder, "file.txt", NodeKind::File, 10);

        assert_eq!(tree.node_count(), 3);
        assert_eq!(tree.get(root).children, vec![folder]);
        assert_eq!(tree.get(folder).parent, Some(root));
        assert_eq!(tree.get(folder).children, vec![file]);
        assert_eq!(tree.get(file).parent, Some(folder));
        assert_eq!(tree.get(file).kind, NodeKind::File);
        assert_eq!(tree.get(file).name, "file.txt");
    }

    #[test]
    fn sizes_add_up_to_every_parent() {
        let mut tree = Tree::new("/scan/root");
        let root = tree.root();

        let outer = tree.add(root, "outer", NodeKind::Directory, 4);
        let inner = tree.add(outer, "inner", NodeKind::Directory, 2);
        let _ = tree.add(inner, "a.bin", NodeKind::File, 100);
        let _ = tree.add(inner, "b.bin", NodeKind::File, 50);
        let _ = tree.add(outer, "c.bin", NodeKind::File, 7);

        assert_eq!(tree.get(inner).own_size, 2);
        assert_eq!(tree.get(inner).total_size, 152);
        assert_eq!(tree.get(outer).own_size, 4);
        assert_eq!(tree.get(outer).total_size, 163);
        assert_eq!(tree.get(root).total_size, 163);
    }

    #[test]
    fn sibling_folders_keep_separate_totals() {
        let mut tree = Tree::new("/scan/root");
        let root = tree.root();

        let left = tree.add(root, "left", NodeKind::Directory, 0);
        let right = tree.add(root, "right", NodeKind::Directory, 0);
        let _ = tree.add(left, "l.bin", NodeKind::File, 30);
        let _ = tree.add(right, "r.bin", NodeKind::File, 5);

        assert_eq!(tree.get(left).total_size, 30);
        assert_eq!(tree.get(right).total_size, 5);
        assert_eq!(tree.get(root).total_size, 35);
    }

    #[test]
    fn path_joins_every_name_from_the_root() {
        let mut tree = Tree::new("/scan/root");
        let root = tree.root();

        let a = tree.add(root, "a", NodeKind::Directory, 0);
        let b = tree.add(a, "b", NodeKind::Directory, 0);
        let file = tree.add(b, "file.txt", NodeKind::File, 1);

        assert_eq!(tree.path(file), PathBuf::from("/scan/root/a/b/file.txt"));
        assert_eq!(tree.path(a), PathBuf::from("/scan/root/a"));
    }

    #[test]
    fn iter_visits_every_node_in_the_order_added() {
        let mut tree = Tree::new("/scan/root");
        let root = tree.root();

        let first = tree.add(root, "first", NodeKind::File, 1);
        let second = tree.add(root, "second", NodeKind::Symlink, 0);

        let ids = tree.iter().map(|(id, _)| id).collect::<Vec<_>>();
        let names = tree
            .iter()
            .map(|(_, node)| node.name.clone())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![root, first, second]);
        assert_eq!(names, vec!["/scan/root", "first", "second"]);
    }
}
