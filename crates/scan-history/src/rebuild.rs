//! Rebuilds an in-memory [`FileTree`] from a persisted [`Scan`], the reverse of
//! [`Scan::from_tree`]. Reopening a saved scan uses this so the views draw it the same way
//! as a live scan.

use std::io;

use timberstat_filesystem::{FileTree, Metadata, NewEntry, NodeId};

use crate::record::{Scan, ScanEntry};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn new_entry(entry: &ScanEntry) -> NewEntry {
    NewEntry {
        name: entry.name.clone(),
        kind: entry.kind,
        logical_size: entry.logical_size,
        physical_size: entry.physical_size,
        identity: entry.identity,
        hardlink_duplicate: entry.hardlink_duplicate,
        unreadable: entry.unreadable,
        metadata: Metadata {
            modified_unix_secs: entry.modified_unix_secs,
            readonly: entry.readonly,
            // Not persisted: POSIX mode bits are not part of the scan record.
            unix_mode: None,
            windows_attributes: entry.windows_attributes,
        },
    }
}

impl Scan {
    /// Builds the tree this scan was saved from.
    ///
    /// The first entry is the root; every other entry names a parent with a smaller index,
    /// as [`Scan::from_tree`] writes them. A record that breaks either rule is rejected
    /// rather than rebuilt into a different tree.
    pub fn to_tree(&self) -> Result<FileTree, io::Error> {
        let mut entries = self.entries.iter().enumerate();
        let Some((_, root)) = entries.next() else {
            return Err(invalid("scan has no root entry"));
        };
        if root.parent.is_some() {
            return Err(invalid("scan root entry has a parent"));
        }
        let mut tree = FileTree::with_root(new_entry(root));
        let mut id_of: Vec<NodeId> = Vec::with_capacity(self.entries.len());
        id_of.push(tree.root());

        for (index, entry) in entries {
            let parent_index = entry
                .parent
                .map(|parent| parent as usize)
                .filter(|&parent| parent < index)
                .ok_or_else(|| invalid(format!("scan entry {index} has no valid parent")))?;
            let parent = id_of[parent_index];
            let id = tree
                .add_child(parent, new_entry(entry))
                .map_err(|error| invalid(format!("scan entry {index}: {error}")))?;
            id_of.push(id);
        }
        tree.roll_up();
        Ok(tree)
    }
}
