use std::{ffi::OsString, path::PathBuf};

use crate::node::{EntryKind, FileIdentity, Metadata, Node, NodeId, Totals};

/// Errors from structural tree edits.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TreeError {
    #[error("node {0:?} does not exist")]
    UnknownNode(NodeId),
    #[error("node {0:?} is not a directory and cannot have children")]
    NotADirectory(NodeId),
    #[error("tree would exceed {0} nodes, the limit of the NodeId range")]
    Capacity(usize),
}

/// Data for one entry being added to the tree.
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub name: OsString,
    pub kind: EntryKind,
    pub logical_size: u64,
    pub physical_size: Option<u64>,
    pub identity: Option<FileIdentity>,
    pub hardlink_duplicate: bool,
    pub unreadable: bool,
    pub metadata: Metadata,
}

impl NewEntry {
    /// A directory entry with no size of its own.
    pub fn directory(name: OsString, metadata: Metadata) -> Self {
        Self {
            name,
            kind: EntryKind::Directory,
            logical_size: 0,
            physical_size: None,
            identity: None,
            hardlink_duplicate: false,
            unreadable: false,
            metadata,
        }
    }
}

/// Arena-backed filesystem tree.
///
/// Nodes live in one `Vec` and refer to each other by [`NodeId`], so a
/// multi-million-entry tree needs no per-node heap allocation beyond its name.
#[derive(Debug, Clone)]
pub struct FileTree {
    nodes: Vec<Node>,
    rolled_up: bool,
}

impl FileTree {
    /// Creates a tree whose root is the given directory.
    pub fn with_root(root: NewEntry) -> Self {
        let node = make_node(None, root);
        Self {
            nodes: vec![node],
            rolled_up: false,
        }
    }

    pub const fn root(&self) -> NodeId {
        NodeId::ROOT
    }

    /// Number of nodes, including the root.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// Appends a child to a directory and returns its id.
    pub fn add_child(&mut self, parent: NodeId, entry: NewEntry) -> Result<NodeId, TreeError> {
        let parent_index = parent.index();
        let parent_node = self
            .nodes
            .get(parent_index)
            .ok_or(TreeError::UnknownNode(parent))?;
        if parent_node.kind != EntryKind::Directory {
            return Err(TreeError::NotADirectory(parent));
        }

        let id =
            NodeId::from_index(self.nodes.len()).ok_or(TreeError::Capacity(self.nodes.len()))?;

        match parent_node.last_child {
            Some(last) => self.nodes[last.index()].next_sibling = Some(id),
            None => self.nodes[parent_index].first_child = Some(id),
        }
        self.nodes[parent_index].last_child = Some(id);

        self.nodes.push(make_node(Some(parent), entry));
        self.rolled_up = false;
        Ok(id)
    }

    /// Marks a directory whose contents could not be fully read.
    /// Its ancestors' totals become incomplete on the next [`roll_up`](Self::roll_up).
    pub fn mark_unreadable(&mut self, id: NodeId) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get_mut(id.index())
            .ok_or(TreeError::UnknownNode(id))?;
        node.unreadable = true;
        self.rolled_up = false;
        Ok(())
    }

    /// Iterates the direct children of a node in insertion order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children {
            tree: self,
            next: self.node(id).and_then(|node| node.first_child),
        }
    }

    /// Recomputes every node's subtree totals. Idempotent; cheap when nothing changed.
    ///
    /// Runs in two linear passes with no recursion, so depth is unbounded.
    pub fn roll_up(&mut self) {
        if self.rolled_up {
            return;
        }
        for node in &mut self.nodes {
            node.totals = Totals {
                logical: node.logical_size,
                physical: node.physical_size.unwrap_or(0),
                physical_known: node.physical_size.is_some(),
                files: u64::from(node.kind == EntryKind::File),
                directories: u64::from(node.kind == EntryKind::Directory),
                complete: !node.unreadable,
            };
        }
        // Children always have larger ids than their parent, so walking backwards
        // finishes every child before its contribution is added to the parent.
        for index in (1..self.nodes.len()).rev() {
            let child = self.nodes[index].totals;
            let Some(parent) = self.nodes[index].parent else {
                continue;
            };
            let parent_totals = &mut self.nodes[parent.index()].totals;
            parent_totals.logical = parent_totals.logical.saturating_add(child.logical);
            parent_totals.physical = parent_totals.physical.saturating_add(child.physical);
            parent_totals.physical_known &= child.physical_known;
            parent_totals.files = parent_totals.files.saturating_add(child.files);
            parent_totals.directories = parent_totals.directories.saturating_add(child.directories);
            parent_totals.complete &= child.complete;
        }
        self.rolled_up = true;
    }

    /// Full path of a node relative to the root's own name, e.g. `root\a\b.txt`.
    /// Components are joined with the platform separator. The root's name is the first component.
    pub fn path_of(&self, id: NodeId) -> Option<PathBuf> {
        let mut components = Vec::new();
        let mut current = Some(id);
        while let Some(node_id) = current {
            let node = self.node(node_id)?;
            components.push(node.name.to_os_string());
            current = node.parent;
        }
        let mut path = PathBuf::new();
        for component in components.iter().rev() {
            path.push(component);
        }
        Some(path)
    }
}

fn make_node(parent: Option<NodeId>, entry: NewEntry) -> Node {
    Node {
        name: entry.name.into_boxed_os_str(),
        parent,
        first_child: None,
        last_child: None,
        next_sibling: None,
        kind: entry.kind,
        logical_size: entry.logical_size,
        physical_size: entry.physical_size,
        identity: entry.identity,
        hardlink_duplicate: entry.hardlink_duplicate,
        unreadable: entry.unreadable,
        metadata: entry.metadata,
        totals: Totals::default(),
    }
}

/// Iterator over the direct children of one node.
pub struct Children<'a> {
    tree: &'a FileTree,
    next: Option<NodeId>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let current = self.next?;
        self.next = self.tree.node(current)?.next_sibling;
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn file(name: &str, size: u64, physical: Option<u64>) -> NewEntry {
        NewEntry {
            name: OsString::from(name),
            kind: EntryKind::File,
            logical_size: size,
            physical_size: physical,
            identity: None,
            hardlink_duplicate: false,
            unreadable: false,
            metadata: Metadata::default(),
        }
    }

    fn dir(name: &str) -> NewEntry {
        // Unix scans always report a directory's own physical size (st_blocks).
        // Mirror that so `physical_known` reflects the files under test.
        let mut entry = NewEntry::directory(OsString::from(name), Metadata::default());
        entry.physical_size = Some(0);
        entry
    }

    fn new_tree() -> FileTree {
        FileTree::with_root(dir("root"))
    }

    #[test]
    fn root_is_node_zero_and_a_directory() {
        let tree = new_tree();
        assert_eq!(tree.root(), NodeId::ROOT);
        assert_eq!(tree.node(tree.root()).unwrap().kind(), EntryKind::Directory);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn children_keep_insertion_order() {
        let mut tree = new_tree();
        let a = tree.add_child(NodeId::ROOT, file("a", 1, Some(1))).unwrap();
        let b = tree.add_child(NodeId::ROOT, dir("b")).unwrap();
        let c = tree.add_child(NodeId::ROOT, file("c", 1, Some(1))).unwrap();
        let order: Vec<NodeId> = tree.children(NodeId::ROOT).collect();
        assert_eq!(order, vec![a, b, c]);
    }

    #[test]
    fn child_ids_are_greater_than_parent_id() {
        let mut tree = new_tree();
        let d = tree.add_child(NodeId::ROOT, dir("d")).unwrap();
        let f = tree.add_child(d, file("f", 1, Some(1))).unwrap();
        assert!(f > d && d > NodeId::ROOT);
    }

    #[test]
    fn adding_under_a_file_is_rejected() {
        let mut tree = new_tree();
        let f = tree.add_child(NodeId::ROOT, file("f", 1, Some(1))).unwrap();
        assert_eq!(
            tree.add_child(f, file("x", 1, Some(1))),
            Err(TreeError::NotADirectory(f))
        );
    }

    #[test]
    fn unknown_parent_is_rejected() {
        let mut tree = new_tree();
        let bogus = NodeId::from_index(99).unwrap();
        assert_eq!(
            tree.add_child(bogus, file("x", 1, Some(1))),
            Err(TreeError::UnknownNode(bogus))
        );
    }

    #[test]
    fn rollup_sums_children_and_counts_entries() {
        let mut tree = new_tree();
        let sub = tree.add_child(NodeId::ROOT, dir("sub")).unwrap();
        tree.add_child(NodeId::ROOT, file("a", 10, Some(4096)))
            .unwrap();
        tree.add_child(sub, file("b", 200, Some(4096))).unwrap();
        tree.add_child(sub, file("c", 0, Some(0))).unwrap();
        tree.roll_up();

        let root = tree.node(NodeId::ROOT).unwrap().totals();
        assert_eq!(root.logical, 210);
        assert_eq!(root.physical, 8192);
        assert_eq!(root.files, 3);
        assert_eq!(root.directories, 2, "root plus one subdirectory");
        assert!(root.physical_known && root.complete);

        let sub_totals = tree.node(sub).unwrap().totals();
        assert_eq!(sub_totals.logical, 200);
        assert_eq!(sub_totals.files, 2);
        assert_eq!(sub_totals.directories, 1);
    }

    #[test]
    fn unknown_physical_size_makes_subtree_physical_unknown() {
        let mut tree = new_tree();
        tree.add_child(NodeId::ROOT, file("a", 10, Some(4096)))
            .unwrap();
        tree.add_child(NodeId::ROOT, file("b", 10, None)).unwrap();
        tree.roll_up();
        let root = tree.node(NodeId::ROOT).unwrap().totals();
        assert!(!root.physical_known);
        assert_eq!(root.logical, 20, "logical is still fully known");
    }

    #[test]
    fn unreadable_directory_makes_ancestors_incomplete() {
        let mut tree = new_tree();
        let sub = tree.add_child(NodeId::ROOT, dir("sub")).unwrap();
        tree.mark_unreadable(sub).unwrap();
        tree.roll_up();
        assert!(!tree.node(NodeId::ROOT).unwrap().totals().complete);
        assert!(!tree.node(sub).unwrap().totals().complete);
    }

    #[test]
    fn rollup_is_idempotent_and_refreshes_after_edits() {
        let mut tree = new_tree();
        tree.add_child(NodeId::ROOT, file("a", 5, Some(5))).unwrap();
        tree.roll_up();
        tree.roll_up();
        assert_eq!(tree.node(NodeId::ROOT).unwrap().totals().logical, 5);
        tree.add_child(NodeId::ROOT, file("b", 7, Some(7))).unwrap();
        tree.roll_up();
        assert_eq!(tree.node(NodeId::ROOT).unwrap().totals().logical, 12);
    }

    #[test]
    fn deep_chain_rolls_up_without_recursion() {
        let mut tree = new_tree();
        let mut parent = NodeId::ROOT;
        for depth in 0..10_000 {
            parent = tree.add_child(parent, dir(&format!("d{depth}"))).unwrap();
        }
        tree.add_child(parent, file("leaf", 42, Some(42))).unwrap();
        tree.roll_up();
        assert_eq!(tree.node(NodeId::ROOT).unwrap().totals().logical, 42);
        assert_eq!(
            tree.node(NodeId::ROOT).unwrap().totals().directories,
            10_001
        );
    }

    #[test]
    fn path_of_joins_names_from_root() {
        let mut tree = FileTree::with_root(dir("root"));
        let sub = tree.add_child(NodeId::ROOT, dir("sub")).unwrap();
        let leaf = tree
            .add_child(sub, file("naïve 日本.txt", 1, Some(1)))
            .unwrap();
        let path = tree.path_of(leaf).unwrap();
        assert_eq!(
            path,
            PathBuf::from("root").join("sub").join("naïve 日本.txt")
        );
        assert_eq!(
            tree.node(leaf).unwrap().name(),
            OsStr::new("naïve 日本.txt")
        );
    }
}
