//! Merges MFT records into one catalog keyed by base record, then rebuilds the
//! directory tree. Each file keeps only its best name (Win32 over POSIX over DOS
//! 8.3): the tree places a file under exactly one parent.

use std::{collections::HashMap, collections::VecDeque, ffi::OsString};

use hmstd_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};

use crate::{DataInfo, NtfsError, Record, reader::ROOT_RECORD};

/// Rank for a name that never appeared; lower ranks win.
const NO_NAME: u8 = u8::MAX;

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// Directory record that holds the chosen name.
    pub parent: u64,
    pub name: String,
    rank: u8,
    /// Other directories that also hold a name for this file (hard links).
    links: Vec<u64>,
    pub is_dir: bool,
    pub data: Option<DataInfo>,
}

#[derive(Debug, Default)]
pub struct Catalog {
    entries: HashMap<u64, CatalogEntry>,
}

/// Outcome of rebuilding the tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildStats {
    /// Entries placed under the root, not counting the root itself.
    pub placed: u64,
    /// In-use records with no path back to the root (for example, a deleted parent).
    pub orphans: u64,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one parsed record. Extension records are folded into their base record,
    /// so attributes split across records still count toward the base file.
    pub fn insert(&mut self, number: u64, record: Record) {
        if !record.in_use {
            return;
        }
        let key = if record.base != 0 {
            record.base
        } else {
            number
        };
        let entry = self.entries.entry(key).or_insert_with(|| CatalogEntry {
            parent: 0,
            name: String::new(),
            rank: NO_NAME,
            links: Vec::new(),
            is_dir: false,
            data: None,
        });
        entry.is_dir |= record.is_dir;
        for name in record.names {
            let rank = rank_of(name.namespace);
            if rank < entry.rank {
                if entry.rank != NO_NAME {
                    entry.links.push(entry.parent);
                }
                entry.rank = rank;
                entry.parent = name.parent;
                entry.name = name.name;
            } else {
                entry.links.push(name.parent);
            }
        }
        match (record.base, record.data) {
            // The base record's $DATA carries the real size; prefer it over fragments.
            (0, Some(data)) => entry.data = Some(data),
            (_, Some(data)) if entry.data.is_none() => entry.data = Some(data),
            _ => {}
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Builds the tree from the root directory down, breadth-first.
    pub fn into_tree(&self, volume_name: &str) -> Result<(FileTree, BuildStats), NtfsError> {
        // Every directory this file is linked from. The walk places the file under the
        // first one it reaches, so the result does not depend on record arrival order.
        let mut pairs: Vec<(u64, u64)> = Vec::new();
        for (number, entry) in &self.entries {
            if *number == ROOT_RECORD || entry.rank == NO_NAME {
                continue;
            }
            pairs.push((entry.parent, *number));
            pairs.extend(entry.links.iter().map(|parent| (*parent, *number)));
        }
        pairs.sort_unstable();
        pairs.dedup();

        let mut tree = FileTree::with_root(NewEntry {
            name: OsString::from(volume_name),
            kind: EntryKind::Directory,
            logical_size: 0,
            physical_size: Some(0),
            identity: None,
            hardlink_duplicate: false,
            unreadable: false,
            metadata: plain_metadata(),
        });

        // Dense map from record number to its node; `None` means not placed yet.
        let slots = self.entries.keys().max().map_or(0, |max| *max as usize + 1);
        let mut placed_as: Vec<Option<NodeId>> = vec![None; slots.max(ROOT_RECORD as usize + 1)];
        placed_as[ROOT_RECORD as usize] = Some(NodeId::ROOT);

        let mut queue = VecDeque::from([ROOT_RECORD]);
        let mut placed: u64 = 0;
        while let Some(parent_record) = queue.pop_front() {
            let Some(parent_node) = placed_as.get(parent_record as usize).copied().flatten() else {
                continue;
            };
            let start = pairs.partition_point(|pair| pair.0 < parent_record);
            for &(_, number) in pairs[start..]
                .iter()
                .take_while(|pair| pair.0 == parent_record)
            {
                let slot = number as usize;
                if slot >= placed_as.len() || placed_as[slot].is_some() {
                    continue;
                }
                let entry = &self.entries[&number];
                let (kind, logical, allocated) = if entry.is_dir {
                    (EntryKind::Directory, 0, Some(0))
                } else {
                    let data = entry.data.unwrap_or_default();
                    (EntryKind::File, data.logical, Some(data.allocated))
                };
                let node = tree.add_child(
                    parent_node,
                    NewEntry {
                        name: OsString::from(&entry.name),
                        kind,
                        logical_size: logical,
                        physical_size: allocated,
                        identity: None,
                        hardlink_duplicate: false,
                        unreadable: false,
                        metadata: plain_metadata(),
                    },
                )?;
                placed_as[slot] = Some(node);
                placed += 1;
                if kind == EntryKind::Directory {
                    queue.push_back(number);
                }
            }
        }

        tree.roll_up();
        let candidates = self
            .entries
            .iter()
            .filter(|(number, entry)| **number != ROOT_RECORD && entry.rank != NO_NAME)
            .count() as u64;
        Ok((
            tree,
            BuildStats {
                placed,
                orphans: candidates - placed,
            },
        ))
    }
}

/// Win32 and combined names first, then POSIX, then DOS 8.3 as a last resort.
fn rank_of(namespace: u8) -> u8 {
    match namespace {
        1 | 3 => 0,
        0 => 1,
        _ => 2,
    }
}

fn plain_metadata() -> Metadata {
    Metadata {
        modified_unix_secs: None,
        readonly: false,
        unix_mode: None,
        windows_attributes: None,
    }
}
