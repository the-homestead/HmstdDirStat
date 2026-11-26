//! Byte verification gate. These tests force every member into one class with an equal digest,
//! so only the byte comparison can decide what is reported. A digest match alone must never
//! produce a group.

use std::fs;
use std::path::Path;

use hmstd_filesystem::NodeId;

use crate::pipeline::{Class, verify_class};
use crate::progress::Progress;
use crate::report::DuplicateMember;

fn member(dir: &Path, name: &str, bytes: &[u8], id: usize) -> DuplicateMember {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write fixture");
    DuplicateMember {
        id: NodeId::from_index(id).expect("node id"),
        path,
    }
}

fn names(members: &[DuplicateMember]) -> Vec<String> {
    members
        .iter()
        .map(|m| {
            m.path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn equal_digest_with_different_bytes_is_not_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let progress = Progress::new();
    let class = Class {
        size: 4,
        digest: [7u8; 32],
        members: vec![
            member(dir.path(), "a.bin", b"aaaa", 1),
            member(dir.path(), "b.bin", b"bbbb", 2),
        ],
    };
    let (groups, failures) = verify_class(class, &progress);
    assert!(groups.is_empty(), "digest collision must not form a group");
    assert!(failures.is_empty());
}

#[test]
fn only_byte_identical_members_of_a_colliding_class_are_grouped() {
    // a and c are byte-identical; b shares the digest but not the bytes.
    let dir = tempfile::tempdir().expect("tempdir");
    let progress = Progress::new();
    let class = Class {
        size: 4,
        digest: [9u8; 32],
        members: vec![
            member(dir.path(), "a.bin", b"same", 1),
            member(dir.path(), "b.bin", b"diff", 2),
            member(dir.path(), "c.bin", b"same", 3),
        ],
    };
    let (groups, failures) = verify_class(class, &progress);
    assert!(failures.is_empty());
    assert_eq!(groups.len(), 1, "{groups:?}");
    assert_eq!(names(&groups[0].members), ["a.bin", "c.bin"]);
    assert!(!names(&groups[0].members).contains(&"b.bin".to_string()));
}

#[test]
fn a_member_that_cannot_be_compared_is_dropped_and_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let progress = Progress::new();
    let present = member(dir.path(), "a.bin", b"data", 1);
    let twin = member(dir.path(), "b.bin", b"data", 2);
    let missing = DuplicateMember {
        id: NodeId::from_index(3).expect("node id"),
        path: dir.path().join("gone.bin"),
    };
    let class = Class {
        size: 4,
        digest: [3u8; 32],
        members: vec![present, twin, missing],
    };
    let (groups, failures) = verify_class(class, &progress);
    assert_eq!(groups.len(), 1);
    assert_eq!(names(&groups[0].members), ["a.bin", "b.bin"]);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].path.ends_with("gone.bin"));
}

#[test]
fn cancelled_verify_offers_no_group() {
    let dir = tempfile::tempdir().expect("tempdir");
    let progress = Progress::new();
    progress.cancel();
    let class = Class {
        size: 4,
        digest: [5u8; 32],
        members: vec![
            member(dir.path(), "a.bin", b"same", 1),
            member(dir.path(), "b.bin", b"same", 2),
        ],
    };
    let (groups, _failures) = verify_class(class, &progress);
    assert!(groups.is_empty());
}
