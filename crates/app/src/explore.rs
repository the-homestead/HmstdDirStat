//! Folder data for the views: children sorted by size, the rows for the Files tab, and
//! the slices for the pie chart and treemap. Pure data with no GPUI types.

use timberstat_filesystem::{EntryKind, FileTree, NodeId};

use crate::tree_model::{self, Sort};

/// One child of a folder. `bytes` is its logical size, including everything below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub node: NodeId,
    pub name: String,
    pub bytes: u64,
    pub is_dir: bool,
}

/// Which size the views show: apparent (logical) or on-disk (allocated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Basis {
    #[default]
    Logical,
    Allocated,
}

/// The size of one node under `basis`. Allocated size is 0 where the scan did not record it.
pub fn bytes_of(tree: &FileTree, node: NodeId, basis: Basis) -> u64 {
    let Some(entry) = tree.node(node) else {
        return 0;
    };
    let is_dir = entry.kind() == EntryKind::Directory;
    match (basis, is_dir) {
        (Basis::Logical, true) => entry.totals().logical,
        (Basis::Logical, false) => entry.logical_size(),
        (Basis::Allocated, true) => {
            let totals = entry.totals();
            if totals.physical_known {
                totals.physical
            } else {
                0
            }
        }
        (Basis::Allocated, false) => entry.physical_size().unwrap_or(0),
    }
}

/// True when the scan recorded allocated sizes for the whole tree. A walk on Windows does
/// not, so the allocated view is not offered for it.
pub fn allocated_known(tree: &FileTree) -> bool {
    tree.node(NodeId::ROOT)
        .is_some_and(|root| root.totals().physical_known)
}

/// True when the scan recorded modification times. The root's time is set on every walk, and
/// the NTFS table read records none.
pub fn modified_known(tree: &FileTree) -> bool {
    tree.node(NodeId::ROOT)
        .is_some_and(|root| root.metadata().modified_unix_secs.is_some())
}

/// Direct children of `parent`, every one, unsorted.
fn unsorted_children(tree: &FileTree, parent: NodeId, basis: Basis) -> Vec<Child> {
    tree.children(parent)
        .filter_map(|node_id| {
            let node = tree.node(node_id)?;
            let is_dir = node.kind() == EntryKind::Directory;
            Some(Child {
                node: node_id,
                name: node.name().to_string_lossy().into_owned(),
                bytes: bytes_of(tree, node_id, basis),
                is_dir,
            })
        })
        .collect()
}

/// Direct children of `parent`, largest first under `basis`, ties broken by name.
pub fn children(tree: &FileTree, parent: NodeId, basis: Basis) -> Vec<Child> {
    let mut out = unsorted_children(tree, parent, basis);
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Direct children of `parent` in the Files tab's `sort` order, logical sizes in the rows.
pub fn sorted_children(tree: &FileTree, parent: NodeId, sort: Sort) -> Vec<Child> {
    let mut out = unsorted_children(tree, parent, Basis::Logical);
    out.sort_by(|a, b| tree_model::compare(tree, a, b, sort));
    out
}

/// One folder level in the Files tab: the rows shown, and how many children were left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Level {
    pub rows: Vec<Row>,
    pub hidden: usize,
}

/// A child in the Files tab. `below` holds its own children only when that folder is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub child: Child,
    pub below: Option<Level>,
}

/// Rows under `parent` that `keep` accepts, sorted by `sort`, at most `limit` per folder. A
/// folder that `descend` accepts gets its own rows, at any depth; any other folder is listed
/// with no rows below it, so nothing is built until it is opened. `budget` caps the rows
/// across the whole result: once it runs out, folders list nothing and count their kept rows
/// as hidden.
pub fn filtered_level(
    tree: &FileTree,
    parent: NodeId,
    limit: usize,
    sort: Sort,
    keep: &dyn Fn(NodeId) -> bool,
    descend: &dyn Fn(NodeId) -> bool,
    budget: &mut usize,
) -> Level {
    let kept: Vec<Child> = sorted_children(tree, parent, sort)
        .into_iter()
        .filter(|child| keep(child.node))
        .collect();
    let shown = kept.len().min(limit).min(*budget);
    *budget -= shown;
    let hidden = kept.len() - shown;
    let rows = kept
        .into_iter()
        .take(shown)
        .map(|child| {
            let below = (child.is_dir && descend(child.node))
                .then(|| filtered_level(tree, child.node, limit, sort, keep, descend, budget));
            Row { child, below }
        })
        .collect();
    Level { rows, hidden }
}

/// One piece of a pie chart or treemap. `node` is `None` for the combined "other" piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slice {
    pub node: Option<NodeId>,
    pub name: String,
    pub bytes: u64,
    pub is_dir: bool,
}

/// The largest `max - 1` children as slices, with the rest combined into one "other" slice.
/// Children of zero size are left out.
pub fn slices(tree: &FileTree, parent: NodeId, max: usize, basis: Basis) -> Vec<Slice> {
    let mut all: Vec<Child> = children(tree, parent, basis)
        .into_iter()
        .filter(|child| child.bytes > 0)
        .collect();
    if all.len() <= max {
        return all.into_iter().map(into_slice).collect();
    }
    let keep = max.saturating_sub(1);
    let rest = all.split_off(keep);
    let other = Slice {
        node: None,
        name: format!("{} other items", rest.len()),
        bytes: rest.iter().map(|child| child.bytes).sum(),
        is_dir: false,
    };
    let mut out: Vec<Slice> = all.into_iter().map(into_slice).collect();
    out.push(other);
    out
}

/// Children as (node, size) pairs for the sunburst, largest first, at most `limit`.
pub fn sized_children(
    tree: &FileTree,
    parent: NodeId,
    limit: usize,
    basis: Basis,
) -> Vec<(NodeId, f64)> {
    children(tree, parent, basis)
        .into_iter()
        .filter(|child| child.bytes > 0)
        .take(limit)
        .map(|child| (child.node, child.bytes as f64))
        .collect()
}

fn into_slice(child: Child) -> Slice {
    Slice {
        node: Some(child.node),
        name: child.name,
        bytes: child.bytes,
        is_dir: child.is_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn scan(root: &Path) -> FileTree {
        GenericScanner
            .scan(
                root,
                &ScanOptions::default(),
                &CancelToken::new(),
                &mut |_| {},
            )
            .unwrap()
            .tree
    }

    fn sample() -> (tempfile::TempDir, FileTree) {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("big/inner/x.bin"), 5000);
        write(&dir.path().join("mid.bin"), 900);
        write(&dir.path().join("small.bin"), 100);
        write(&dir.path().join("tiny.bin"), 50);
        let tree = scan(dir.path());
        (dir, tree)
    }

    #[test]
    fn children_are_sorted_by_size_with_folder_totals() {
        let (_dir, tree) = sample();
        let names: Vec<(String, u64, bool)> = children(&tree, NodeId::ROOT, Basis::Logical)
            .into_iter()
            .map(|child| (child.name, child.bytes, child.is_dir))
            .collect();
        assert_eq!(
            names,
            vec![
                ("big".to_string(), 5000, true),
                ("mid.bin".to_string(), 900, false),
                ("small.bin".to_string(), 100, false),
                ("tiny.bin".to_string(), 50, false),
            ]
        );
    }

    #[test]
    fn level_limits_rows_and_counts_the_hidden_ones() {
        let (_dir, tree) = sample();
        let mut budget = usize::MAX;
        let shown = filtered_level(
            &tree,
            NodeId::ROOT,
            2,
            Sort::default(),
            &|_| true,
            &|_| true,
            &mut budget,
        );
        assert_eq!(shown.rows.len(), 2);
        assert_eq!(shown.hidden, 2);
        let below_big = shown.rows[0].below.as_ref().expect("folder is expanded");
        assert_eq!(below_big.rows[0].child.name, "inner");
        assert!(shown.rows[1].below.is_none(), "files have no children");
    }

    #[test]
    fn folders_build_rows_only_when_they_are_open() {
        let (_dir, tree) = sample();
        let big = tree
            .children(NodeId::ROOT)
            .find(|&id| tree.node(id).unwrap().name() == "big")
            .unwrap();
        let mut budget = usize::MAX;
        let closed = filtered_level(
            &tree,
            NodeId::ROOT,
            40,
            Sort::default(),
            &|_| true,
            &|_| false,
            &mut budget,
        );
        assert!(closed.rows.iter().all(|row| row.below.is_none()));
        let opened = filtered_level(
            &tree,
            NodeId::ROOT,
            40,
            Sort::default(),
            &|_| true,
            &|node| node == big,
            &mut budget,
        );
        let below = opened.rows[0].below.as_ref().expect("big is open");
        assert_eq!(below.rows[0].child.name, "inner");
        assert!(below.rows[0].below.is_none(), "inner is not open");
    }

    #[test]
    fn modified_known_follows_the_root_time() {
        let (_dir, tree) = sample();
        assert!(modified_known(&tree), "a walk records times");
    }

    #[test]
    fn slices_combine_the_rest_into_one_other_piece() {
        let (_dir, tree) = sample();
        let pie = slices(&tree, NodeId::ROOT, 3, Basis::Logical);
        assert_eq!(pie.len(), 3);
        assert_eq!(pie[0].name, "big");
        assert_eq!(pie[1].name, "mid.bin");
        assert_eq!(pie[2].node, None);
        assert_eq!(pie[2].name, "2 other items");
        assert_eq!(pie[2].bytes, 150);
    }
}
