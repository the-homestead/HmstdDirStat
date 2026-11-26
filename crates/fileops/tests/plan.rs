mod common;

use common::{base, find_required, scan, write};
use hmstd_fileops::{PlanError, apply_delete, build_delete_plan, build_delete_plan_capped};
use hmstd_filesystem::{EntryKind, FileIdentity, FileTree, Metadata, NewEntry, NodeId};

#[test]
fn preview_counts_files_folders_and_sizes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("gone/sub/b.bin"), &[0u8; 100]);
    write(root.join("gone/c.txt"), b"cc");
    write(root.join("keep/a.txt"), b"aaaa");
    let tree = scan(&root);
    let gone = find_required(&tree, &root, "gone");

    let plan = build_delete_plan(&tree, &[gone], &base(&root)).unwrap();
    let preview = plan.preview();
    assert_eq!(preview.files, 2);
    assert_eq!(preview.directories, 2, "gone and gone/sub");
    assert_eq!(preview.logical, 102);
    assert!(preview.physical_known);
    assert_eq!(preview.physical, 102);
    assert_eq!(preview.paths_hidden, 0);
    assert_eq!(preview.paths_shown.len(), 4);
    assert_eq!(plan.confirm_phrase(), "gone");
}

#[test]
fn preview_lists_at_most_cap_paths_and_reports_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    for index in 0..5 {
        write(root.join(format!("big/f{index}.dat")), b"x");
    }
    let tree = scan(&root);
    let big = find_required(&tree, &root, "big");
    let plan = build_delete_plan_capped(&tree, &[big], &base(&root), 2).unwrap();
    let preview = plan.preview();
    assert_eq!(preview.paths_shown.len(), 2);
    assert_eq!(preview.paths_hidden, 4, "six entries in total, two shown");
    assert_eq!(preview.entry_count(), 6);
}

#[test]
fn nested_selection_is_counted_once() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("outer/inner/x.txt"), b"12345");
    let tree = scan(&root);
    let outer = find_required(&tree, &root, "outer");
    let inner = find_required(&tree, &root, "outer/inner");
    let plan = build_delete_plan(&tree, &[outer, inner], &base(&root)).unwrap();
    assert_eq!(plan.groups().len(), 1, "inner is already inside outer");
    assert_eq!(plan.preview().logical, 5);
    assert_eq!(plan.preview().directories, 2);
}

#[test]
fn scan_root_and_empty_selection_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("a.txt"), b"a");
    let tree = scan(&root);
    assert_eq!(
        build_delete_plan(&tree, &[NodeId::ROOT], &base(&root)).unwrap_err(),
        PlanError::ScanRoot
    );
    assert_eq!(
        build_delete_plan(&tree, &[], &base(&root)).unwrap_err(),
        PlanError::NothingSelected
    );
}

#[test]
fn detached_selection_is_refused_after_a_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("a.txt"), b"a");
    let mut tree = scan(&root);
    let a = find_required(&tree, &root, "a.txt");
    // Applied with a hand-built report: the Recycle Bin is never called from tests.
    let fake = hmstd_fileops::DeleteReport {
        removed: vec![a],
        ..Default::default()
    };
    apply_delete(&mut tree, &fake);
    assert_eq!(
        build_delete_plan(&tree, &[a], &base(&root)).unwrap_err(),
        PlanError::Detached(a)
    );
}

#[test]
fn permanent_delete_needs_the_exact_typed_name() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("gone/a.txt"), b"a");
    let tree = scan(&root);
    let gone = find_required(&tree, &root, "gone");
    let plan = build_delete_plan(&tree, &[gone], &base(&root)).unwrap();
    assert_eq!(
        plan.clone().confirm_permanent("Gone").unwrap_err(),
        PlanError::ConfirmationMismatch
    );
    assert_eq!(
        plan.clone().confirm_permanent("").unwrap_err(),
        PlanError::ConfirmationMismatch
    );
    assert!(
        plan.confirm_permanent("  gone  ").is_ok(),
        "ends are trimmed"
    );
    assert!(
        root.join("gone/a.txt").exists(),
        "a refused plan deletes nothing"
    );
}

#[test]
fn several_roots_confirm_with_their_count() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("one/a.txt"), b"a");
    write(root.join("two/b.txt"), b"b");
    let tree = scan(&root);
    let one = find_required(&tree, &root, "one");
    let two = find_required(&tree, &root, "two");
    let plan = build_delete_plan(&tree, &[one, two], &base(&root)).unwrap();
    assert_eq!(plan.confirm_phrase(), "2");
    assert!(plan.clone().confirm_permanent("one").is_err());
    assert!(plan.confirm_permanent("2").is_ok());
}

#[test]
fn shared_hard_link_outside_the_selection_is_not_reclaimable() {
    let shared = FileIdentity {
        volume: 1,
        file_id: 7,
    };
    let mut tree = FileTree::with_root(NewEntry::directory("scan".into(), Metadata::default()));
    let mut inside = NewEntry {
        name: "link_a.bin".into(),
        kind: EntryKind::File,
        logical_size: 10_000,
        physical_size: Some(4096),
        identity: Some(shared),
        hardlink_duplicate: false,
        unreadable: false,
        metadata: Metadata::default(),
    };
    let inside_id = tree.add_child(NodeId::ROOT, inside.clone()).unwrap();
    inside.name = "link_b.bin".into();
    inside.physical_size = Some(0);
    inside.hardlink_duplicate = true;
    tree.add_child(NodeId::ROOT, inside).unwrap();
    tree.roll_up();

    let tmp = tempfile::tempdir().unwrap();
    let plan = build_delete_plan(&tree, &[inside_id], tmp.path()).unwrap();
    let preview = plan.preview();
    assert_eq!(preview.physical, 4096, "charged once for the selection");
    assert_eq!(
        preview.reclaimable, 0,
        "the other link keeps the data alive"
    );
}
