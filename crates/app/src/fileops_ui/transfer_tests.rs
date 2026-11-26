//! Temp-directory checks for the copy, move and rename worker paths the dialogs drive.
//! Every path is inside a `tempfile` directory created by the test.

use std::ffi::OsStr;
use std::path::Path;

use timberstat_fileops::{
    Collision, Control, RenameOutcome, TransferKind, apply_moves, execute_transfer, plan_transfer,
    tree_base,
};
use timberstat_filesystem::{FileTree, NodeId};
use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

use super::rename::{RenameStep, rename_step};
use super::transfer::target_taken;

fn scan(root: &Path) -> FileTree {
    GenericScanner
        .scan(
            root,
            &ScanOptions::default(),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap()
        .tree
}

fn child(tree: &FileTree, name: &str) -> NodeId {
    tree.children(NodeId::ROOT)
        .find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.name() == OsStr::new(name))
        })
        .unwrap()
}

/// src/ holds the item; dest/ is the destination folder.
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("src")).unwrap();
    std::fs::create_dir(temp.path().join("dest")).unwrap();
    std::fs::write(temp.path().join("src/a.txt"), b"new").unwrap();
    temp
}

#[test]
fn move_runs_on_disk_and_leaves_the_in_memory_tree() {
    let temp = fixture();
    let src = temp.path().join("src");
    let mut tree = scan(&src);
    let node = child(&tree, "a.txt");
    let plan = plan_transfer(
        &tree,
        &[node],
        &tree_base(&src),
        &temp.path().join("dest"),
        TransferKind::Move,
    )
    .unwrap();
    assert!(!target_taken(&plan, None));

    let report = execute_transfer(plan, Collision::Skip, &Control::new());

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!src.join("a.txt").exists());
    assert_eq!(
        std::fs::read(temp.path().join("dest/a.txt")).unwrap(),
        b"new"
    );
    assert_eq!(apply_moves(&mut tree, &report), 1);
    assert!(!tree.is_attached(node));
}

#[test]
fn a_taken_name_is_seen_before_anything_runs_and_skip_keeps_both_files_as_they_were() {
    let temp = fixture();
    std::fs::write(temp.path().join("dest/a.txt"), b"old").unwrap();
    let src = temp.path().join("src");
    let tree = scan(&src);
    let node = child(&tree, "a.txt");
    let plan = plan_transfer(
        &tree,
        &[node],
        &tree_base(&src),
        &temp.path().join("dest"),
        TransferKind::Copy,
    )
    .unwrap();
    assert!(target_taken(&plan, None));
    assert!(!target_taken(&plan, Some("free-name.txt")));

    let report = execute_transfer(plan, Collision::Skip, &Control::new());

    assert_eq!(report.skipped.len(), 1);
    assert_eq!(
        std::fs::read(temp.path().join("dest/a.txt")).unwrap(),
        b"old"
    );
    assert_eq!(std::fs::read(src.join("a.txt")).unwrap(), b"new");
}

#[test]
fn rename_over_a_taken_name_asks_first_and_changes_nothing() {
    let temp = fixture();
    std::fs::write(temp.path().join("src/b.txt"), b"other").unwrap();
    let path = temp.path().join("src/a.txt");

    let step = rename_step(&path, "b.txt", "a.txt", Collision::Overwrite, false);

    assert!(matches!(step, RenameStep::Confirm(ref target) if target.ends_with("b.txt")));
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert_eq!(
        std::fs::read(temp.path().join("src/b.txt")).unwrap(),
        b"other"
    );
}

#[test]
fn rename_to_a_free_name_renames_on_disk() {
    let temp = fixture();
    let path = temp.path().join("src/a.txt");

    let step = rename_step(&path, "c.txt", "a.txt", Collision::Skip, false);

    let RenameStep::Done(Ok(RenameOutcome::Renamed { to })) = step else {
        panic!("expected a rename");
    };
    assert!(to.ends_with("c.txt"));
    assert!(!path.exists());
    assert_eq!(
        std::fs::read(temp.path().join("src/c.txt")).unwrap(),
        b"new"
    );
}
