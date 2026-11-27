use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use timberstat_filesystem::{EntryKind, FileIdentity, FileTree, NodeId};

use crate::ExportError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportedEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

impl ExportedEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Other => "other",
        }
    }
}

impl From<EntryKind> for ExportedEntryKind {
    fn from(kind: EntryKind) -> Self {
        match kind {
            EntryKind::File => Self::File,
            EntryKind::Directory => Self::Directory,
            EntryKind::Symlink => Self::Symlink,
            EntryKind::Other => Self::Other,
        }
    }
}

impl From<ExportedEntryKind> for EntryKind {
    fn from(kind: ExportedEntryKind) -> Self {
        match kind {
            ExportedEntryKind::File => Self::File,
            ExportedEntryKind::Directory => Self::Directory,
            ExportedEntryKind::Symlink => Self::Symlink,
            ExportedEntryKind::Other => Self::Other,
        }
    }
}

/// One exported filesystem row. Sizes are the entry's own scan-model values;
/// directory subtree totals are not substituted. Unknown values are empty in CSV
/// and `null` in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRow {
    pub path: String,
    pub name: String,
    pub kind: ExportedEntryKind,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
    pub modified: Option<i64>,
    pub depth: u32,
    pub hard_link_count: Option<u64>,
}

impl ExportRow {
    pub(crate) fn csv_fields(&self) -> Vec<String> {
        vec![
            self.path.clone(),
            self.name.clone(),
            self.kind.as_str().to_owned(),
            self.logical_size.to_string(),
            optional_number(self.allocated_size),
            optional_number(self.modified),
            self.depth.to_string(),
            optional_number(self.hard_link_count),
        ]
    }
}

fn optional_number(value: Option<impl ToString>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

pub(crate) type HardLinkCounts = HashMap<FileIdentity, u64>;

pub(crate) fn tree_hard_link_counts(tree: &FileTree) -> HardLinkCounts {
    let mut counts = HashMap::new();
    for id in tree.attached_ids() {
        if let Some(identity) = tree.node(id).and_then(|node| node.identity()) {
            *counts.entry(identity).or_insert(0) += 1;
        }
    }
    counts
}

pub(crate) fn row_from_tree(
    tree: &FileTree,
    id: NodeId,
    hard_links: &HardLinkCounts,
) -> Result<ExportRow, ExportError> {
    if !tree.is_attached(id) {
        return Err(ExportError::MissingNode(id));
    }
    let node = tree.node(id).ok_or(ExportError::MissingNode(id))?;
    let path = tree.path_of(id).ok_or(ExportError::MissingNode(id))?;
    let name = node
        .name()
        .to_os_string()
        .into_string()
        .map_err(|name| ExportError::NonUnicodePath(PathBuf::from(name)))?;
    let mut depth = 0_u32;
    let mut parent = node.parent();
    while let Some(id) = parent {
        let node = tree.node(id).ok_or(ExportError::MissingNode(id))?;
        depth = depth.saturating_add(1);
        parent = node.parent();
    }
    Ok(ExportRow {
        path: unicode_path(path)?,
        name,
        kind: node.kind().into(),
        logical_size: node.logical_size(),
        allocated_size: node.physical_size(),
        modified: node.metadata().modified_unix_secs,
        depth,
        hard_link_count: node
            .identity()
            .and_then(|key| hard_links.get(&key).copied()),
    })
}

pub(crate) fn unicode_path(path: PathBuf) -> Result<String, ExportError> {
    path.as_os_str()
        .to_str()
        .map(str::to_owned)
        .ok_or(ExportError::NonUnicodePath(path))
}
