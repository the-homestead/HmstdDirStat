//! Read-only path and marker lookups over the scanner's in-memory tree.

use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use timberstat_filesystem::{EntryKind, FileTree, Node, NodeId};

use crate::patterns;

pub(crate) struct TreeIndex<'a> {
    tree: &'a FileTree,
    paths: HashMap<NodeId, PathBuf>,
}

impl<'a> TreeIndex<'a> {
    pub fn build(tree: &'a FileTree, scan_root: &Path) -> Self {
        let mut paths = HashMap::with_capacity(tree.len());
        for id in tree.attached_ids() {
            let node = tree.node(id).expect("attached node must exist");
            let path = if id == tree.root() {
                scan_root.to_path_buf()
            } else {
                let parent = node.parent().and_then(|parent| paths.get(&parent));
                parent
                    .cloned()
                    .unwrap_or_else(|| scan_root.to_path_buf())
                    .join(node.name())
            };
            paths.insert(id, path);
        }
        Self { tree, paths }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        self.tree.node(id).expect("indexed node must exist")
    }

    pub fn path(&self, id: NodeId) -> &Path {
        self.paths.get(&id).expect("indexed node must have a path")
    }

    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.tree.children(id)
    }

    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        self.tree.subtree_bottom_up(id)
    }

    pub fn has_marker(&self, id: NodeId, marker: &str) -> bool {
        self.children(id)
            .any(|child| name_eq(self.node(child).name(), OsStr::new(marker)))
    }

    pub fn has_file_marker(&self, id: NodeId, marker: &str) -> bool {
        self.children(id).any(|child| {
            self.node(child).kind() == EntryKind::File
                && name_eq(self.node(child).name(), OsStr::new(marker))
        })
    }

    pub fn has_visual_studio_project(&self, id: NodeId) -> bool {
        self.children(id).any(|child| {
            self.node(child).kind() == EntryKind::File
                && patterns::has_visual_studio_project_extension(self.node(child).name())
        })
    }

    pub fn has_visual_studio_context(&self, id: NodeId) -> bool {
        if lower(self.node(id).name()) == ".vs" {
            return true;
        }
        let Some(parent) = self.node(id).parent() else {
            return false;
        };
        if self.has_visual_studio_project(parent) {
            return true;
        }
        self.node(parent)
            .parent()
            .is_some_and(|grandparent| self.has_visual_studio_project(grandparent))
    }

    pub fn contains_rust_artifact(&self, id: NodeId) -> bool {
        self.subtree(id).into_iter().any(|member| {
            let node = self.node(member);
            node.kind() == EntryKind::File && patterns::is_rust_artifact(node.name())
        })
    }
}

#[cfg(windows)]
fn name_eq(left: &OsStr, right: &OsStr) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(not(windows))]
fn name_eq(left: &OsStr, right: &OsStr) -> bool {
    left == right
}

fn lower(name: &OsStr) -> String {
    patterns::lower(name)
}
