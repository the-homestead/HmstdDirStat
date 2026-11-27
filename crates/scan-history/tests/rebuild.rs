use std::path::PathBuf;

use timberstat_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};
use timberstat_scan_history::{Scan, ScanEntry, ScanMethod, ScanProvenance, ScanStore};

fn provenance() -> ScanProvenance {
    ScanProvenance {
        method: ScanMethod::GenericTraversal,
        timestamp_unix_secs: 1_700_000_000,
        root: PathBuf::from("C:/data"),
    }
}

fn file(name: &str, size: u64) -> NewEntry {
    NewEntry {
        name: name.into(),
        kind: EntryKind::File,
        logical_size: size,
        physical_size: Some(size.next_multiple_of(4096)),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: Metadata {
            modified_unix_secs: Some(1_690_000_000),
            readonly: true,
            unix_mode: None,
            windows_attributes: Some(0x20),
        },
    }
}

/// root/ a.bin(300) sub/ b.bin(700) sub/inner/ c.bin(5) locked/ (unreadable, empty)
fn sample_tree() -> FileTree {
    let mut tree = FileTree::with_root(NewEntry::directory("data".into(), Metadata::default()));
    let root = tree.root();
    tree.add_child(root, file("a.bin", 300)).unwrap();
    let sub = tree
        .add_child(root, NewEntry::directory("sub".into(), Metadata::default()))
        .unwrap();
    tree.add_child(sub, file("b.bin", 700)).unwrap();
    let inner = tree
        .add_child(
            sub,
            NewEntry::directory("inner".into(), Metadata::default()),
        )
        .unwrap();
    tree.add_child(inner, file("c.bin", 5)).unwrap();
    let locked = tree
        .add_child(
            root,
            NewEntry::directory("locked".into(), Metadata::default()),
        )
        .unwrap();
    tree.mark_unreadable(locked).unwrap();
    tree.roll_up();
    tree
}

/// Every attached node as (path, kind, logical size, unreadable), in arena order.
fn shape(tree: &FileTree) -> Vec<(PathBuf, EntryKind, u64, bool)> {
    tree.attached_ids()
        .map(|id: NodeId| {
            let node = tree.node(id).unwrap();
            (
                tree.path_of(id).unwrap(),
                node.kind(),
                node.logical_size(),
                node.is_unreadable(),
            )
        })
        .collect()
}

fn totals_of(tree: &FileTree) -> (u64, u64, u64, u64, bool) {
    let totals = tree.node(tree.root()).unwrap().totals();
    (
        totals.logical,
        totals.physical,
        totals.files,
        totals.directories,
        totals.complete,
    )
}

#[test]
fn rebuilt_tree_matches_the_saved_tree() {
    let tree = sample_tree();
    let scan = Scan::from_tree(provenance(), &tree).unwrap();
    let rebuilt = scan.to_tree().unwrap();

    assert_eq!(shape(&rebuilt), shape(&tree));
    assert_eq!(totals_of(&rebuilt), totals_of(&tree));
    assert!(
        !totals_of(&rebuilt).4,
        "the unreadable folder keeps the totals incomplete"
    );
}

#[test]
fn rebuilt_tree_keeps_each_entry_metadata() {
    let tree = sample_tree();
    let rebuilt = Scan::from_tree(provenance(), &tree)
        .unwrap()
        .to_tree()
        .unwrap();
    let a = rebuilt.children(rebuilt.root()).next().unwrap();
    let node = rebuilt.node(a).unwrap();
    assert_eq!(node.metadata().modified_unix_secs, Some(1_690_000_000));
    assert!(node.metadata().readonly);
    assert_eq!(node.metadata().windows_attributes, Some(0x20));
    assert_eq!(node.physical_size(), Some(4096));
}

#[test]
fn saved_then_reopened_scan_rebuilds_the_same_tree() {
    let dir = tempfile::tempdir().unwrap();
    let store = ScanStore::new(dir.path());
    let tree = sample_tree();
    let scan = Scan::from_tree(provenance(), &tree).unwrap();
    store.save(&scan).unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    let id = listed[0].id.clone();
    let reopened = store.load(&id).unwrap().to_tree().unwrap();

    assert_eq!(shape(&reopened), shape(&tree));
    assert_eq!(totals_of(&reopened), totals_of(&tree));
}

#[test]
fn record_with_a_forward_parent_is_rejected() {
    let entry = |parent: Option<u32>, name: &str| ScanEntry {
        parent,
        name: name.into(),
        kind: EntryKind::Directory,
        logical_size: 0,
        physical_size: None,
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        modified_unix_secs: None,
        readonly: false,
        windows_attributes: None,
    };
    let root = entry(None, "root");
    let bad = Scan::new(provenance(), vec![root.clone(), entry(Some(5), "x")]);
    assert!(bad.to_tree().is_err());

    let no_root = Scan::new(provenance(), vec![entry(Some(0), "x")]);
    assert!(no_root.to_tree().is_err());

    let empty = Scan::new(provenance(), Vec::new());
    assert!(empty.to_tree().is_err());
}
