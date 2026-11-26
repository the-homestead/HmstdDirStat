mod common;

use std::fs;

use common::{find, find_required, scan, write};
use timberstat_fileops::{
    Collision, FailureReason, PlanError, RenameOutcome, apply_rename, rename_path, validate_name,
};
use timberstat_filesystem::NodeId;

#[test]
fn names_with_separators_or_reserved_characters_are_refused() {
    for bad in [
        "",
        "   ",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:b",
        "a*b",
        "trailing.",
        "trailing ",
        "tab\tname",
    ] {
        assert!(
            matches!(validate_name(bad), Err(PlanError::BadName(..))),
            "{bad:?} should be refused"
        );
    }
    for good in ["report 2026.pdf", "naïve 日本.txt", ".hidden", "a"] {
        assert!(validate_name(good).is_ok(), "{good:?} should be allowed");
    }
}

#[test]
fn rename_to_a_free_name_moves_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("old.txt");
    write(&file, b"data");
    let outcome = rename_path(&file, "new.txt", Collision::Rename).unwrap();
    assert_eq!(
        outcome,
        RenameOutcome::Renamed {
            to: tmp.path().join("new.txt")
        }
    );
    assert!(!file.exists());
    assert_eq!(fs::read(tmp.path().join("new.txt")).unwrap(), b"data");
}

#[test]
fn same_name_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("same.txt");
    write(&file, b"x");
    assert_eq!(
        rename_path(&file, "same.txt", Collision::Skip).unwrap(),
        RenameOutcome::Unchanged
    );
}

#[test]
fn taken_name_follows_the_policy() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.txt");
    write(&file, b"mine");
    write(tmp.path().join("b.txt"), b"theirs");

    let skipped = rename_path(&file, "b.txt", Collision::Skip).unwrap();
    assert!(matches!(skipped, RenameOutcome::Skipped { .. }));
    assert_eq!(fs::read(tmp.path().join("b.txt")).unwrap(), b"theirs");

    let refused = rename_path(&file, "b.txt", Collision::Rename).unwrap_err();
    assert_eq!(refused.reason, FailureReason::Conflict);
    assert!(file.exists());

    let kept = rename_path(&file, "b.txt", Collision::KeepBoth).unwrap();
    assert_eq!(
        kept,
        RenameOutcome::Renamed {
            to: tmp.path().join("b (2).txt")
        }
    );
    assert_eq!(fs::read(tmp.path().join("b.txt")).unwrap(), b"theirs");

    write(tmp.path().join("c.txt"), b"other");
    write(&file, b"mine");
    let replaced = rename_path(&file, "c.txt", Collision::Overwrite).unwrap();
    assert!(matches!(replaced, RenameOutcome::Renamed { .. }));
    assert_eq!(fs::read(tmp.path().join("c.txt")).unwrap(), b"mine");
}

#[test]
fn overwrite_never_replaces_a_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.txt");
    write(&file, b"mine");
    fs::create_dir(tmp.path().join("folder")).unwrap();
    let failure = rename_path(&file, "folder", Collision::Overwrite).unwrap_err();
    assert_eq!(failure.reason, FailureReason::Conflict);
    assert!(tmp.path().join("folder").is_dir());
}

#[test]
fn invalid_name_is_reported_as_invalid_and_nothing_moves() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.txt");
    write(&file, b"x");
    let failure = rename_path(&file, "bad/name", Collision::Skip).unwrap_err();
    assert_eq!(failure.reason, FailureReason::InvalidName);
    assert!(file.exists());
}

#[test]
fn applying_a_rename_updates_memory_and_descendant_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("old_dir/child.txt"), b"c");
    let mut tree = scan(&root);
    let old_dir = find_required(&tree, &root, "old_dir");
    fs::rename(root.join("old_dir"), root.join("new_dir")).unwrap();

    assert!(apply_rename(&mut tree, old_dir, "new_dir".as_ref()));

    assert!(find(&tree, &root, "old_dir").is_none());
    assert!(
        find(&tree, &root, "new_dir/child.txt").is_some(),
        "children follow the rename"
    );
    assert!(!apply_rename(
        &mut tree,
        NodeId::from_index(9999).unwrap(),
        "x".as_ref()
    ));
}
