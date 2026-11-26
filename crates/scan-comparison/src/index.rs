use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use timberstat_filesystem::EntryKind;
use timberstat_scan_history::{CURRENT_SCHEMA, Scan, ScanEntry};

use crate::{ComparisonError, model::SizeTotals};

pub(crate) fn ensure_compatible_schema(
    version: u32,
    snapshot: usize,
) -> Result<(), ComparisonError> {
    if version == CURRENT_SCHEMA {
        Ok(())
    } else {
        Err(ComparisonError::IncompatibleSchema {
            snapshot,
            found: version,
            supported: CURRENT_SCHEMA,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct BuildNode {
    parent: Option<u32>,
    total: SizeTotals,
}

#[derive(Debug, Clone, Copy)]
struct IndexedEntry {
    source: u32,
    total: SizeTotals,
}

/// Validated paths and exact subtree totals for one scan.
pub(crate) struct IndexedScan<'a> {
    scan: &'a Scan,
    root: PathBuf,
    entries: HashMap<PathBuf, IndexedEntry>,
}

impl<'a> IndexedScan<'a> {
    pub(crate) fn build(scan: &'a Scan, snapshot: usize) -> Result<Self, ComparisonError> {
        if scan.entries.is_empty() {
            return Err(invalid(snapshot, "scan has no root entry"));
        }

        let mut paths = Vec::with_capacity(scan.entries.len());
        let mut nodes = Vec::with_capacity(scan.entries.len());
        let mut by_path = HashMap::with_capacity(scan.entries.len());

        for (index, entry) in scan.entries.iter().enumerate() {
            let source = u32::try_from(index)
                .map_err(|_| invalid(snapshot, "scan exceeds the entry index range"))?;
            let path = entry_path(scan, &paths, index, entry, snapshot)?;
            if by_path.insert(path.clone(), source).is_some() {
                return Err(invalid(snapshot, format!("duplicate entry path {path:?}")));
            }
            paths.push(path);
            nodes.push(BuildNode {
                parent: entry.parent,
                total: SizeTotals {
                    logical: u128::from(entry.logical_size),
                    physical: entry.physical_size.map(u128::from),
                },
            });
        }

        // Parents precede children, so reverse order aggregates every subtree exactly.
        for index in (1..nodes.len()).rev() {
            let Some(parent) = nodes[index].parent else {
                continue;
            };
            let child = nodes[index].total;
            let parent_total = &mut nodes[parent as usize].total;
            parent_total.logical += child.logical;
            parent_total.physical = match (parent_total.physical, child.physical) {
                (Some(parent), Some(child)) => Some(parent + child),
                _ => None,
            };
        }

        let entries = by_path
            .into_iter()
            .map(|(path, source)| {
                let total = nodes[source as usize].total;
                (path, IndexedEntry { source, total })
            })
            .collect();
        let root = scan.root_path().to_path_buf();
        Ok(Self {
            scan,
            root,
            entries,
        })
    }

    pub(crate) fn root_total(&self) -> SizeTotals {
        self.entries[&self.root].total
    }

    pub(crate) fn entry(&self, path: &Path) -> Option<&ScanEntry> {
        let indexed = self.entries.get(path)?;
        self.scan.entry(indexed.source)
    }

    pub(crate) fn total(&self, path: &Path) -> Option<SizeTotals> {
        self.entries.get(path).map(|entry| entry.total)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Path, &ScanEntry)> {
        self.entries.iter().map(|(path, indexed)| {
            let entry = self
                .scan
                .entry(indexed.source)
                .expect("validated scan index must resolve");
            (path.as_path(), entry)
        })
    }
}

fn entry_path(
    scan: &Scan,
    paths: &[PathBuf],
    index: usize,
    entry: &ScanEntry,
    snapshot: usize,
) -> Result<PathBuf, ComparisonError> {
    if index == 0 {
        if entry.parent.is_some() {
            return Err(invalid(snapshot, "the first entry is not the scan root"));
        }
        if entry.kind != EntryKind::Directory {
            return Err(invalid(snapshot, "the scan root is not a directory"));
        }
        return Ok(scan.root_path().to_path_buf());
    }

    let parent = entry
        .parent
        .ok_or_else(|| invalid(snapshot, format!("entry {index} is a second root")))?;
    let parent_index = parent as usize;
    if parent_index >= index {
        return Err(invalid(
            snapshot,
            format!("entry {index} has a parent that does not precede it"),
        ));
    }
    let parent_entry = scan
        .entry(parent)
        .ok_or_else(|| invalid(snapshot, format!("entry {index} has an unknown parent")))?;
    if parent_entry.kind != EntryKind::Directory {
        return Err(invalid(
            snapshot,
            format!("entry {index} has a non-directory parent"),
        ));
    }

    let name = Path::new(entry.name.as_os_str());
    let mut components = name.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(invalid(
            snapshot,
            format!("entry {index} has an invalid path component"),
        ));
    }
    paths
        .get(parent_index)
        .map(|parent| parent.join(name))
        .ok_or_else(|| {
            invalid(
                snapshot,
                format!("entry {index} has an unknown parent path"),
            )
        })
}

fn invalid(snapshot: usize, reason: impl Into<String>) -> ComparisonError {
    ComparisonError::InvalidScan {
        snapshot,
        reason: reason.into(),
    }
}
