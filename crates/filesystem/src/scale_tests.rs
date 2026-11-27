//! Size and scale checks for [`Node`] and [`FileTree`].
//!
//! `node_size_is_reported` prints the measured layout and asserts nothing about it: no
//! memory budget has been agreed yet. The million-entry test is `#[ignore]`d because it
//! allocates about one million nodes. Run it with:
//!
//! `cargo test -p timberstat-filesystem --release -- --ignored --nocapture scale`

use std::{ffi::OsString, mem::size_of, time::Instant};

use crate::{EntryKind, FileTree, Metadata, NewEntry, Node, NodeId};

const DIRECTORIES: usize = 1_000;
const FILES_PER_DIRECTORY: usize = 1_000;

/// Mirrors the scanner: every directory reports its own physical size (zero), so a
/// fully measured tree keeps `physical_known` true. `NewEntry::directory` leaves it `None`.
fn directory(name: &str) -> NewEntry {
    let mut entry = NewEntry::directory(OsString::from(name), Metadata::default());
    entry.physical_size = Some(0);
    entry
}

#[test]
fn node_size_is_reported() {
    println!("scale: size_of::<Node>() = {} bytes", size_of::<Node>());
    println!(
        "scale: size_of::<Option<NodeId>>() = {} bytes",
        size_of::<Option<NodeId>>()
    );
    println!(
        "scale: size_of::<Option<FileIdentity>>() = {} bytes",
        size_of::<Option<crate::FileIdentity>>()
    );
}

#[test]
#[ignore = "allocates about one million nodes; run in release with --ignored"]
fn scale_one_million_entries_roll_up() {
    let started = Instant::now();
    let mut tree = FileTree::with_root(directory("root"));

    let mut directories = Vec::with_capacity(DIRECTORIES);
    for index in 0..DIRECTORIES {
        let id = tree
            .add_child(NodeId::ROOT, directory(&format!("d{index}")))
            .expect("directory under root");
        directories.push(id);
    }

    for &directory in &directories {
        for index in 0..FILES_PER_DIRECTORY {
            tree.add_child(
                directory,
                NewEntry {
                    name: OsString::from(format!("f{index}.bin")),
                    kind: EntryKind::File,
                    logical_size: 10,
                    physical_size: Some(4096),
                    identity: None,
                    hardlink_duplicate: false,
                    unreadable: false,
                    metadata: Metadata::default(),
                },
            )
            .expect("file under directory");
        }
    }
    let built = started.elapsed();

    let rolled = Instant::now();
    tree.roll_up();
    let rolled_in = rolled.elapsed();

    let expected_entries = 1 + DIRECTORIES + DIRECTORIES * FILES_PER_DIRECTORY;
    assert_eq!(tree.len(), expected_entries);

    let totals = tree.node(NodeId::ROOT).expect("root").totals();
    let files = (DIRECTORIES * FILES_PER_DIRECTORY) as u64;
    assert_eq!(totals.files, files);
    assert_eq!(totals.directories, (DIRECTORIES + 1) as u64);
    assert_eq!(totals.logical, 10 * files);
    assert_eq!(totals.physical, 4096 * files);
    assert!(totals.physical_known);
    assert!(totals.complete);

    let leaf = tree
        .children(*directories.last().expect("directory"))
        .last()
        .expect("leaf");
    let path = tree.path_of(leaf).expect("leaf path");
    assert!(path.ends_with(format!("f{}.bin", FILES_PER_DIRECTORY - 1)));

    let node_bytes = size_of::<Node>() * tree.len();
    println!(
        "scale: entries={} build={:?} roll_up={:?} node_arena={} MiB (Node only, names excluded)",
        tree.len(),
        built,
        rolled_in,
        node_bytes / (1024 * 1024)
    );
}
