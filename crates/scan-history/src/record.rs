use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use timberstat_filesystem::{EntryKind, FileIdentity, FileTree, Totals};

use crate::provenance::ScanProvenance;
use crate::schema::CURRENT_SCHEMA;

/// One filesystem entry, metadata only. File contents are never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    /// Index into [`Scan::entries`] of the parent entry; `None` for the scan root.
    pub parent: Option<u32>,
    pub name: OsString,
    pub kind: EntryKind,
    pub logical_size: u64,
    pub physical_size: Option<u64>,
    pub identity: Option<FileIdentity>,
    pub hardlink_duplicate: bool,
    pub unreadable: bool,
    pub modified_unix_secs: Option<i64>,
    pub readonly: bool,
    pub windows_attributes: Option<u32>,
}

/// One persisted scan. Entries reference each other by index into [`Scan::entries`];
/// a `None` parent means the entry is the scan root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    pub provenance: ScanProvenance,
    pub entries: Vec<ScanEntry>,
    pub(crate) schema_version: u32,
}

impl Scan {
    pub fn new(provenance: ScanProvenance, entries: Vec<ScanEntry>) -> Self {
        Self {
            provenance,
            entries,
            schema_version: CURRENT_SCHEMA,
        }
    }

    pub fn root_path(&self) -> &Path {
        &self.provenance.root
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn entry(&self, index: u32) -> Option<&ScanEntry> {
        self.entries.get(index as usize)
    }

    /// Full path of `index` relative to the scan root.
    pub fn path_of(&self, index: u32) -> Option<PathBuf> {
        let mut components: Vec<OsString> = Vec::new();
        let mut current = Some(index);
        while let Some(i) = current {
            let entry = self.entries.get(i as usize)?;
            components.push(entry.name.clone());
            current = entry.parent;
        }
        let mut path = PathBuf::new();
        for component in components.iter().rev() {
            path.push(component);
        }
        Some(path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Subtree-agnostic aggregation of every entry.
    pub fn totals(&self) -> Totals {
        let mut totals = Totals::default();
        for entry in &self.entries {
            totals.logical = totals.logical.saturating_add(entry.logical_size);
            totals.physical = totals
                .physical
                .saturating_add(entry.physical_size.unwrap_or(0));
            totals.physical_known &= entry.physical_size.is_some();
            totals.files += u64::from(entry.kind == EntryKind::File);
            totals.directories += u64::from(entry.kind == EntryKind::Directory);
            totals.complete &= !entry.unreadable;
        }
        totals
    }

    /// Builds a [`Scan`] from an in-memory [`FileTree`], preserving arena order so a
    /// child's index is always greater than its parent's index.
    pub fn from_tree(provenance: ScanProvenance, tree: &FileTree) -> Result<Self, std::io::Error> {
        let mut index_of = vec![None as Option<u32>; tree.len()];
        let mut entries = Vec::with_capacity(tree.len());
        for id in tree.attached_ids() {
            let node = tree.node(id).ok_or_else(|| missing_node(id))?;
            let index = u32::try_from(entries.len())
                .map_err(|_| std::io::Error::other("scan history tree exceeds the index range"))?;
            index_of[id.index()] = Some(index);
            let parent = node
                .parent()
                .and_then(|parent_id| index_of[parent_id.index()]);
            entries.push(ScanEntry {
                parent,
                name: node.name().to_os_string(),
                kind: node.kind(),
                logical_size: node.logical_size(),
                physical_size: node.physical_size(),
                identity: node.identity(),
                hardlink_duplicate: node.is_hardlink_duplicate(),
                unreadable: node.is_unreadable(),
                modified_unix_secs: node.metadata().modified_unix_secs,
                readonly: node.metadata().readonly,
                windows_attributes: node.metadata().windows_attributes,
            });
        }
        Ok(Self::new(provenance, entries))
    }
}

fn missing_node(id: timberstat_filesystem::NodeId) -> std::io::Error {
    std::io::Error::other(format!("tree node {id:?} is missing"))
}
