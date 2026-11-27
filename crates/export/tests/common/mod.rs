#![allow(dead_code)]

use std::ffi::OsString;
use std::path::PathBuf;

use timberstat_filesystem::{EntryKind, FileIdentity, FileTree, Metadata, NewEntry};
use timberstat_scan_history::{ScanMethod, ScanProvenance};

pub fn provenance(root: &str) -> ScanProvenance {
    ScanProvenance {
        method: ScanMethod::GenericTraversal,
        timestamp_unix_secs: 1_700_000_123,
        root: PathBuf::from(root),
    }
}

pub fn directory(name: &str) -> NewEntry {
    let mut entry = NewEntry::directory(OsString::from(name), Metadata::default());
    entry.physical_size = Some(0);
    entry
}

pub fn file(name: &str, size: u64) -> NewEntry {
    NewEntry {
        name: OsString::from(name),
        kind: EntryKind::File,
        logical_size: size,
        physical_size: Some(size),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: Metadata {
            modified_unix_secs: Some(1_699_999_999),
            ..Metadata::default()
        },
    }
}

pub fn identified_file(name: &str, identity: FileIdentity, hardlink_duplicate: bool) -> NewEntry {
    let mut entry = file(name, 42);
    entry.physical_size = Some(4096);
    entry.identity = Some(identity);
    entry.hardlink_duplicate = hardlink_duplicate;
    entry
}

pub fn tree_with_files(names: &[&str]) -> FileTree {
    let mut tree = FileTree::with_root(directory("root"));
    for &name in names {
        tree.add_child(tree.root(), file(name, 42)).unwrap();
    }
    tree
}

/// Small independent RFC 4180 reader used only as a test oracle.
pub fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        match character {
            '"' if field.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(character),
        }
    }
    assert!(!quoted, "unterminated quoted field");
    assert!(field.is_empty() && record.is_empty(), "missing final CRLF");
    records
}
