//! Brings the in-memory tree in line with what an operation did on disk.
//!
//! These functions only edit memory. They never rescan or stat the disk, so
//! views update at once and nothing new is read from the filesystem.

use std::ffi::OsStr;

use timberstat_filesystem::{FileTree, NodeId};

use crate::delete::DeleteReport;
use crate::transfer::TransferReport;

/// Removes every node a delete took off disk from the tree, then refreshes totals.
/// Returns how many nodes left the tree. Nodes that failed stay in the tree.
pub fn apply_delete(tree: &mut FileTree, report: &DeleteReport) -> usize {
    apply_removed(tree, &report.removed)
}

/// Removes source roots that a move took out of their old location.
pub fn apply_moves(tree: &mut FileTree, report: &TransferReport) -> usize {
    apply_removed(tree, &report.moved)
}

/// Renames a node in memory after a successful rename on disk. Descendants' paths follow.
pub fn apply_rename(tree: &mut FileTree, id: NodeId, new_name: &OsStr) -> bool {
    tree.set_name(id, new_name.to_os_string()).is_ok()
}

fn apply_removed(tree: &mut FileTree, ids: &[NodeId]) -> usize {
    let mut detached = 0;
    for &id in ids {
        if let Ok(count) = tree.detach(id) {
            detached += count;
        }
    }
    if detached > 0 {
        tree.roll_up();
    }
    detached
}
