//! In-memory scan fixtures; comparison tests never touch real user data.
#![allow(dead_code, reason = "each integration test uses a helper subset")]

use std::{ffi::OsString, path::PathBuf};

use timberstat_filesystem::{EntryKind, FileIdentity};
use timberstat_scan_history::{Scan, ScanEntry, ScanMethod, ScanProvenance};

pub const ROOT: &str = "C:/root";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileState {
    pub logical: u64,
    pub physical: Option<u64>,
    pub modified: Option<i64>,
    pub identity: Option<FileIdentity>,
    pub hardlink_duplicate: bool,
}

impl FileState {
    pub fn new(logical: u64, modified: i64) -> Self {
        Self {
            logical,
            physical: Some(logical),
            modified: Some(modified),
            identity: None,
            hardlink_duplicate: false,
        }
    }
}

pub fn scan(root: &str, timestamp: i64, entries: Vec<ScanEntry>) -> Scan {
    Scan::new(
        ScanProvenance {
            method: ScanMethod::GenericTraversal,
            timestamp_unix_secs: timestamp,
            root: PathBuf::from(root),
        },
        entries,
    )
}

pub fn root_entry() -> ScanEntry {
    ScanEntry {
        parent: None,
        name: OsString::from("root"),
        kind: EntryKind::Directory,
        logical_size: 0,
        physical_size: Some(0),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        modified_unix_secs: Some(1),
        readonly: false,
        windows_attributes: None,
    }
}

pub fn directory(name: &str, parent: Option<u32>) -> ScanEntry {
    ScanEntry {
        parent,
        name: OsString::from(name),
        kind: EntryKind::Directory,
        logical_size: 0,
        physical_size: Some(0),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        modified_unix_secs: Some(1),
        readonly: false,
        windows_attributes: None,
    }
}

pub fn file(name: &str, parent: Option<u32>, logical: u64, modified: i64) -> ScanEntry {
    file_state(name, parent, FileState::new(logical, modified))
}

pub fn file_state(name: &str, parent: Option<u32>, state: FileState) -> ScanEntry {
    ScanEntry {
        parent,
        name: OsString::from(name),
        kind: EntryKind::File,
        logical_size: state.logical,
        physical_size: state.physical,
        identity: state.identity,
        hardlink_duplicate: state.hardlink_duplicate,
        unreadable: false,
        modified_unix_secs: state.modified,
        readonly: false,
        windows_attributes: None,
    }
}

pub fn scan_with_file(root: &str, timestamp: i64, size: u64) -> Scan {
    scan(
        root,
        timestamp,
        vec![root_entry(), file("data", Some(0), size, 1)],
    )
}
