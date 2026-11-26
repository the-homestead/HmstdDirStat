mod common;

use std::fs;
use std::path::PathBuf;

use common::{base, find, find_required, scan, write};
use hmstd_fileops::{
    Collision, Control, FailureReason, PlanError, SkipReason, TransferKind, apply_moves,
    execute_transfer, plan_transfer,
};

fn dest_of(root: &std::path::Path) -> PathBuf {
    let dest = root.join("dst");
    fs::create_dir_all(&dest).unwrap();
    dest
}

#[test]
fn keep_both_writes_a_numbered_copy_and_leaves_both_originals() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"new");
    write(root.join("dst/a.txt"), b"old");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();

    let report = execute_transfer(plan, Collision::KeepBoth, &Control::new());

    assert!(report.is_complete(), "{}", report.summary());
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old");
    assert_eq!(fs::read(dest.join("a (2).txt")).unwrap(), b"new");
    assert_eq!(
        fs::read(root.join("src/a.txt")).unwrap(),
        b"new",
        "copy keeps the source"
    );
}

#[test]
fn skip_leaves_the_existing_destination_untouched_and_reports_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"new");
    write(root.join("dst/a.txt"), b"old");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();

    let report = execute_transfer(plan, Collision::Skip, &Control::new());

    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].reason, SkipReason::CollisionSkipped);
    assert_eq!(report.done_top_level, 0);
}

#[test]
fn overwrite_replaces_file_content_and_leaves_no_staging_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"replacement");
    write(root.join("dst/a.txt"), b"old");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();

    let report = execute_transfer(plan, Collision::Overwrite, &Control::new());

    assert!(report.is_complete(), "{}", report.summary());
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"replacement");
    let leftovers: Vec<_> = fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".part"))
        .collect();
    assert!(leftovers.is_empty(), "staging file removed: {leftovers:?}");
}

#[test]
fn rename_policy_without_a_new_name_fails_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"new");
    write(root.join("dst/a.txt"), b"old");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();

    let report = execute_transfer(plan, Collision::Rename, &Control::new());

    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].reason, FailureReason::Conflict);
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old");
}

#[test]
fn rename_policy_uses_the_name_the_user_typed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"new");
    write(root.join("dst/a.txt"), b"old");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let mut plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();
    plan.set_name(0, "renamed.txt").unwrap();

    let report = execute_transfer(plan, Collision::Rename, &Control::new());

    assert!(report.is_complete(), "{}", report.summary());
    assert_eq!(fs::read(dest.join("renamed.txt")).unwrap(), b"new");
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old");
}

#[test]
fn file_and_folder_never_replace_each_other() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/item/inner.txt"), b"folder");
    write(root.join("dst/item"), b"a file named item");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/item");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();

    let report = execute_transfer(plan, Collision::Overwrite, &Control::new());

    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].reason, FailureReason::Conflict);
    assert_eq!(fs::read(dest.join("item")).unwrap(), b"a file named item");
}

#[test]
fn same_volume_move_renames_and_memory_drops_the_source() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"moving");
    let mut tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Move).unwrap();

    let report = execute_transfer(plan, Collision::Skip, &Control::new());

    assert!(report.is_complete(), "{}", report.summary());
    assert!(!root.join("src/a.txt").exists());
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"moving");
    assert_eq!(report.destinations, vec![dest.join("a.txt")]);
    apply_moves(&mut tree, &report);
    assert!(find(&tree, &root, "src/a.txt").is_none());
}

#[test]
fn move_with_overwrite_merges_a_folder_and_removes_the_source_after_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/pack/x.txt"), b"x-new");
    write(root.join("src/pack/y.txt"), b"y");
    write(root.join("dst/pack/x.txt"), b"x-old");
    let mut tree = scan(&root);
    let src = find_required(&tree, &root, "src/pack");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Move).unwrap();

    let report = execute_transfer(plan, Collision::Overwrite, &Control::new());

    assert!(report.is_complete(), "{}", report.summary());
    assert_eq!(fs::read(dest.join("pack/x.txt")).unwrap(), b"x-new");
    assert_eq!(fs::read(dest.join("pack/y.txt")).unwrap(), b"y");
    assert!(
        !root.join("src/pack").exists(),
        "source removed after a clean copy"
    );
    apply_moves(&mut tree, &report);
    assert!(find(&tree, &root, "src/pack").is_none());
}

#[test]
fn copying_a_folder_into_itself_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("outer/inner/x.txt"), b"x");
    let tree = scan(&root);
    let outer = find_required(&tree, &root, "outer");
    let inside = root.join("outer/inner");
    let error =
        plan_transfer(&tree, &[outer], &base(&root), &inside, TransferKind::Copy).unwrap_err();
    assert!(matches!(error, PlanError::IntoItself { .. }), "{error}");
}

#[test]
fn move_onto_its_own_location_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"same");
    let mut tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let plan = plan_transfer(
        &tree,
        &[src],
        &base(&root),
        &root.join("src"),
        TransferKind::Move,
    )
    .unwrap();

    let report = execute_transfer(plan, Collision::Overwrite, &Control::new());

    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].reason, SkipReason::SameLocation);
    assert_eq!(
        fs::read(root.join("src/a.txt")).unwrap(),
        b"same",
        "source is never deleted"
    );
    apply_moves(&mut tree, &report);
    assert!(find(&tree, &root, "src/a.txt").is_some());
}

#[test]
fn cancelled_copy_copies_nothing_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/a.txt"), b"a");
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/a.txt");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();
    let control = Control::new();
    control.cancel();

    let report = execute_transfer(plan, Collision::KeepBoth, &control);

    assert!(report.cancelled);
    assert_eq!(report.not_attempted, 1);
    assert!(!report.is_complete());
    assert!(!dest.join("a.txt").exists());
}

#[cfg(unix)]
#[test]
fn copy_skips_links_and_reports_them() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/pack/real.txt"), b"real");
    std::os::unix::fs::symlink(root.join("src/pack/real.txt"), root.join("src/pack/link")).unwrap();
    let tree = scan(&root);
    let src = find_required(&tree, &root, "src/pack");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Copy).unwrap();

    let report = execute_transfer(plan, Collision::Skip, &Control::new());

    assert!(!report.is_complete(), "a link was left behind");
    assert!(
        report
            .skipped
            .iter()
            .any(|s| s.reason == SkipReason::LinkNotCopied)
    );
    assert!(
        dest.join("pack/real.txt").exists(),
        "the real file is copied"
    );
    assert!(
        fs::symlink_metadata(dest.join("pack/link")).is_err(),
        "the link is not recreated"
    );
    assert!(
        root.join("src/pack/link").exists(),
        "the source is untouched"
    );
}

/// A move on one volume renames the folder in one step, so a link inside travels with it
/// intact and nothing is left behind.
#[cfg(unix)]
#[test]
fn same_volume_move_carries_links_intact() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("src/pack/real.txt"), b"real");
    std::os::unix::fs::symlink("real.txt", root.join("src/pack/link")).unwrap();
    let mut tree = scan(&root);
    let src = find_required(&tree, &root, "src/pack");
    let dest = dest_of(&root);
    let plan = plan_transfer(&tree, &[src], &base(&root), &dest, TransferKind::Move).unwrap();

    let report = execute_transfer(plan, Collision::Skip, &Control::new());

    assert!(report.is_complete());
    assert!(!root.join("src/pack").exists());
    let moved = dest.join("pack/link");
    assert!(
        fs::symlink_metadata(&moved)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&moved).unwrap(), b"real");
    apply_moves(&mut tree, &report);
    assert!(find(&tree, &root, "src/pack").is_none());
}
