//! Read-only indexes computed from the scanner's in-memory tree.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use timberstat_filesystem::{EntryKind, FileTree, NodeId};

use crate::patterns;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MediaAggregate {
    pub files: u64,
    pub logical: u64,
}

pub(crate) struct TreeIndex<'a> {
    tree: &'a FileTree,
    paths: HashMap<NodeId, PathBuf>,
    media: HashMap<NodeId, MediaAggregate>,
}

impl<'a> TreeIndex<'a> {
    pub fn build(tree: &'a FileTree, scan_root: &Path) -> Self {
        let ids: Vec<NodeId> = tree.attached_ids().collect();
        let mut paths = HashMap::with_capacity(ids.len());
        let mut media: HashMap<NodeId, MediaAggregate> = HashMap::with_capacity(ids.len());

        // Parents precede children in the arena.
        for &id in &ids {
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

        // Reverse arena order gives bottom-up media totals without recursion.
        for &id in ids.iter().rev() {
            let node = tree.node(id).expect("attached node must exist");
            let mut aggregate = MediaAggregate::default();
            if node.kind() == EntryKind::File
                && patterns::is_media_extension(&patterns::extension_of(node.name()))
            {
                aggregate.files = 1;
                aggregate.logical = node.logical_size();
            }
            for child in tree.children(id) {
                let child = media.get(&child).copied().unwrap_or_default();
                aggregate.files = aggregate.files.saturating_add(child.files);
                aggregate.logical = aggregate.logical.saturating_add(child.logical);
            }
            media.insert(id, aggregate);
        }

        Self { tree, paths, media }
    }

    pub fn path(&self, id: NodeId) -> &Path {
        self.paths.get(&id).expect("indexed node must have a path")
    }

    pub fn media(&self, id: NodeId) -> MediaAggregate {
        self.media.get(&id).copied().unwrap_or_default()
    }

    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        self.tree.subtree_bottom_up(id)
    }

    pub fn files_under(&self, id: NodeId) -> Vec<NodeId> {
        self.subtree(id)
            .into_iter()
            .filter(|node| {
                self.tree
                    .node(*node)
                    .is_some_and(|node| node.kind() == EntryKind::File)
            })
            .collect()
    }

    /// Logical bytes with each known file identity charged once. Used only when
    /// allocated bytes are unavailable, so hard links still cannot inflate totals.
    pub fn unique_logical_under(&self, id: NodeId) -> u64 {
        let mut identities = HashSet::new();
        self.files_under(id).into_iter().fold(0_u64, |total, id| {
            let node = self.tree.node(id).expect("subtree node must exist");
            let already_counted = node
                .identity()
                .is_some_and(|identity| !identities.insert(identity));
            if already_counted {
                total
            } else {
                total.saturating_add(node.logical_size())
            }
        })
    }
}
