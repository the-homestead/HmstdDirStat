//! Turns a scanned tree into the work items the pipeline reads.

use std::path::Path;

use hmstd_fileops::WorkItem;
use hmstd_filesystem::{EntryKind, FileTree};

/// One work item per attached regular file in `tree`. Paths are resolved
/// against `base`, the on-disk folder that holds the scan root (see
/// `hmstd_fileops::tree_base`). Folders, links and special entries are left out.
pub fn file_items(tree: &FileTree, base: &Path) -> Vec<WorkItem> {
    tree.attached_ids()
        .filter_map(|id| {
            let node = tree.node(id)?;
            if node.kind() != EntryKind::File {
                return None;
            }
            Some(WorkItem {
                id,
                parent: node.parent(),
                path: base.join(tree.path_of(id)?),
                kind: EntryKind::File,
                logical: node.logical_size(),
                physical: node.physical_size(),
                identity: node.identity(),
            })
        })
        .collect()
}
