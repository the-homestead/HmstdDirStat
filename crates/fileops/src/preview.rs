//! Turns selected tree nodes into work items and preview totals.
//!
//! Everything here reads the in-memory tree only. Callers run it on a worker
//! thread; the UI thread never walks a large selection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use hmstd_filesystem::{EntryKind, FileIdentity, FileTree, NodeId};

use crate::model::{PlanError, Preview, WorkItem};

/// How many paths a preview lists before it reports the rest as hidden.
pub const DEFAULT_PREVIEW_CAP: usize = 200;

/// Drops duplicate roots and roots that sit under another selected root, so
/// nothing is counted or deleted twice. Refuses the scan root and detached nodes.
pub fn normalize_roots(tree: &FileTree, roots: &[NodeId]) -> Result<Vec<NodeId>, PlanError> {
    if roots.is_empty() {
        return Err(PlanError::NothingSelected);
    }
    let selected: HashSet<NodeId> = roots.iter().copied().collect();
    let mut kept: Vec<NodeId> = Vec::new();
    for &root in roots {
        if !tree.is_attached(root) {
            return Err(PlanError::Detached(root));
        }
        if root == tree.root() {
            return Err(PlanError::ScanRoot);
        }
        if has_selected_ancestor(tree, root, &selected) || kept.contains(&root) {
            continue;
        }
        kept.push(root);
    }
    Ok(kept)
}

fn has_selected_ancestor(tree: &FileTree, id: NodeId, selected: &HashSet<NodeId>) -> bool {
    let mut current = tree.node(id).and_then(|node| node.parent());
    while let Some(ancestor) = current {
        if selected.contains(&ancestor) {
            return true;
        }
        current = tree.node(ancestor).and_then(|node| node.parent());
    }
    false
}

/// The on-disk directory that contains the tree's root folder: the scan root's
/// parent. Tree paths are relative to it, because a tree root is named by its
/// own file name.
pub fn tree_base(scan_root: &Path) -> PathBuf {
    scan_root
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf)
}

/// Every entry under the normalized roots, roots' subtrees in bottom-up order.
/// `base` is the on-disk directory from [`tree_base`].
pub fn collect_items(
    tree: &FileTree,
    roots: &[NodeId],
    base: &Path,
) -> Result<Vec<WorkItem>, PlanError> {
    let mut items = Vec::new();
    for root in normalize_roots(tree, roots)? {
        for id in tree.subtree_bottom_up(root) {
            let node = tree.node(id).ok_or(PlanError::Detached(id))?;
            let relative = tree.path_of(id).ok_or(PlanError::Detached(id))?;
            items.push(WorkItem {
                id,
                parent: node.parent(),
                path: base.join(relative),
                kind: node.kind(),
                logical: node.logical_size(),
                physical: node.physical_size(),
                identity: node.identity(),
            });
        }
    }
    Ok(items)
}

/// Preview totals for a set of items. `tree` is needed only to see which hard
/// links are also reachable from outside the selection.
pub fn summarize(tree: &FileTree, items: &[WorkItem], cap: usize) -> Preview {
    let in_plan: HashSet<NodeId> = items.iter().map(|item| item.id).collect();
    let plan_identities: HashSet<FileIdentity> =
        items.iter().filter_map(|item| item.identity).collect();

    let mut shared_outside: HashSet<FileIdentity> = HashSet::new();
    if !plan_identities.is_empty() {
        for id in tree.attached_ids() {
            if in_plan.contains(&id) {
                continue;
            }
            if let Some(identity) = tree.node(id).and_then(|node| node.identity())
                && plan_identities.contains(&identity)
            {
                shared_outside.insert(identity);
            }
        }
    }

    let mut preview = Preview {
        physical_known: true,
        ..Preview::default()
    };
    for item in items {
        match item.kind {
            EntryKind::File => preview.files += 1,
            EntryKind::Directory => preview.directories += 1,
            EntryKind::Symlink | EntryKind::Other => preview.links_and_other += 1,
        }
        preview.logical = preview.logical.saturating_add(item.logical);
        match item.physical {
            Some(bytes) => {
                preview.physical = preview.physical.saturating_add(bytes);
                let freed_by_op = item
                    .identity
                    .is_none_or(|identity| !shared_outside.contains(&identity));
                if freed_by_op {
                    preview.reclaimable = preview.reclaimable.saturating_add(bytes);
                }
            }
            None => preview.physical_known = false,
        }
    }

    let mut sorted: Vec<&PathBuf> = items.iter().map(|item| &item.path).collect();
    sorted.sort_unstable();
    preview.paths_hidden = sorted.len().saturating_sub(cap);
    preview.paths_shown = sorted.into_iter().take(cap).cloned().collect();
    preview
}
