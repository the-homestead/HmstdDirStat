//! Shared fixtures for the query tests: a sample scan and check helpers.
//! Each test file pulls this in with `mod common;`.
#![allow(dead_code)] // every test file uses a different subset of these helpers

use std::ffi::OsString;

use hmstd_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};
use hmstd_search::{Evaluation, Expr, ParseError, compile, run};

/// Fixed "now" for age terms: 2027-01-15, a Friday.
pub const NOW: i64 = 1_800_000_000;
pub const DAY: i64 = 86_400;
pub const GB: u64 = 1 << 30;
pub const MB: u64 = 1 << 20;
/// 2024-01-15 12:00 UTC.
pub const JAN_15_2024_NOON: i64 = 1_705_320_000;
pub const HIDDEN: u32 = 0x0000_0002;
pub const REPARSE: u32 = 0x0000_0400;

pub fn meta(modified: Option<i64>) -> Metadata {
    Metadata {
        modified_unix_secs: modified,
        ..Metadata::default()
    }
}

/// A file. `on` says whether the scan recorded an on-disk size.
pub fn file(name: &str, logical: u64, on: bool, modified: Option<i64>) -> NewEntry {
    NewEntry {
        name: OsString::from(name),
        kind: EntryKind::File,
        logical_size: logical,
        physical_size: on.then_some(logical + 4096),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: meta(modified),
    }
}

pub fn dir(name: &str, on: bool, modified: Option<i64>) -> NewEntry {
    let mut entry = NewEntry::directory(OsString::from(name), meta(modified));
    entry.physical_size = on.then_some(0);
    entry
}

pub fn link(name: &str, on: bool, attributes: u32) -> NewEntry {
    NewEntry {
        name: OsString::from(name),
        kind: EntryKind::Symlink,
        logical_size: 0,
        physical_size: on.then_some(0),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: Metadata {
            windows_attributes: Some(attributes),
            ..Metadata::default()
        },
    }
}

pub fn with_attributes(mut entry: NewEntry, attributes: u32, readonly: bool) -> NewEntry {
    entry.metadata.windows_attributes = Some(attributes);
    entry.metadata.readonly = readonly;
    entry
}

pub fn add(tree: &mut FileTree, parent: NodeId, entry: NewEntry) -> NodeId {
    tree.add_child(parent, entry).expect("add child")
}

/// The sample scan, rolled up. `on` controls whether on-disk sizes are recorded.
///
/// ```text
/// W/
///   Games/   setup.iso 1.5gb (readonly), readme.txt 100b, My Report.pdf 10mb (2024-01-15),
///            .secret 7b (hidden)
///   Docs/    notes.md 50b (400 days old), archive.zip 2gb (100 days old), empty.txt 0b,
///            backup.tmp 1b (hidden attribute), link (symlink, reparse point)
///   Split/   part1.bin 600mb, part2.bin 600mb        (folder total 1.17gb, no child > 1gb)
///   Empty/
/// ```
pub fn sample_tree(on: bool) -> FileTree {
    let mut tree = FileTree::with_root(dir("W", on, Some(NOW - DAY)));
    let root = NodeId::ROOT;

    let games = add(&mut tree, root, dir("Games", on, Some(NOW - DAY)));
    add(
        &mut tree,
        games,
        with_attributes(file("setup.iso", GB + GB / 2, on, None), 0, true),
    );
    add(
        &mut tree,
        games,
        file("readme.txt", 100, on, Some(NOW - 3 * DAY)),
    );
    add(
        &mut tree,
        games,
        file("My Report.pdf", 10 * MB, on, Some(JAN_15_2024_NOON)),
    );
    add(
        &mut tree,
        games,
        with_attributes(file(".secret", 7, on, Some(NOW - DAY)), HIDDEN, false),
    );

    let docs = add(&mut tree, root, dir("Docs", on, Some(NOW - 2 * DAY)));
    add(
        &mut tree,
        docs,
        file("notes.md", 50, on, Some(NOW - 400 * DAY)),
    );
    add(
        &mut tree,
        docs,
        file("archive.zip", 2 * GB, on, Some(NOW - 100 * DAY)),
    );
    add(&mut tree, docs, file("empty.txt", 0, on, None));
    add(
        &mut tree,
        docs,
        with_attributes(file("backup.tmp", 1, on, None), HIDDEN, false),
    );
    add(&mut tree, docs, link("link", on, REPARSE));

    let split = add(&mut tree, root, dir("Split", on, None));
    add(&mut tree, split, file("part1.bin", 600 * MB, on, None));
    add(&mut tree, split, file("part2.bin", 600 * MB, on, None));

    add(&mut tree, root, dir("Empty", on, None));
    tree.roll_up();
    tree
}

pub fn expr_of(query: &str) -> Expr {
    compile(query)
        .unwrap_or_else(|error| panic!("{query:?} should parse: {error}"))
        .expect("query is not blank")
}

pub fn evaluate(tree: &FileTree, query: &str) -> Evaluation {
    run(tree, &expr_of(query), NOW)
}

/// Names of the matches, sorted, so the assertions do not depend on tree order.
pub fn names(tree: &FileTree, query: &str) -> Vec<String> {
    let mut out: Vec<String> = evaluate(tree, query)
        .ids
        .iter()
        .map(|id| {
            tree.node(*id)
                .expect("matched node exists")
                .name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    out.sort();
    out
}

pub fn sorted(expected: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = expected.iter().map(|name| name.to_string()).collect();
    out.sort();
    out
}

pub fn check(tree: &FileTree, query: &str, expected: &[&str]) {
    assert_eq!(names(tree, query), sorted(expected), "query {query:?}");
}

pub fn parse_error(query: &str) -> ParseError {
    match compile(query) {
        Err(error) => error,
        Ok(expr) => panic!("{query:?} should fail to parse, got {expr:?}"),
    }
}

pub fn check_error(query: &str, at: usize, message_part: &str) {
    let error = parse_error(query);
    assert_eq!(error.at, at, "position for {query:?}: {error}");
    assert!(
        error.message.contains(message_part),
        "message for {query:?} should contain {message_part:?}, got {:?}",
        error.message
    );
}
