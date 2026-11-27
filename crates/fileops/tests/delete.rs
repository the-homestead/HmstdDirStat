mod common;

use std::fs;
use std::path::Path;

use common::{base, find, find_required, scan, write};
use timberstat_fileops::{
    Control, DeleteMode, FailureReason, apply_delete, build_delete_plan, execute_delete_with,
};

/// True when this process is root (Linux `/proc/self/status`, effective uid 0). Root bypasses
/// permission bits, so chmod-based failure tests cannot fail there. Always false elsewhere.
fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status").is_ok_and(|text| {
        text.lines()
            .find(|line| line.starts_with("Uid:"))
            .and_then(|line| line.split_whitespace().nth(2))
            == Some("0")
    })
}

/// A Recycle Bin stand-in that must never run. Used for permanent-mode tests:
/// if the executor tried the bin in permanent mode the test panics.
fn no_bin(path: &Path) -> Result<(), trash::Error> {
    panic!(
        "permanent mode must not call the Recycle Bin: {}",
        path.display()
    );
}

#[test]
fn permanent_delete_removes_tree_and_updates_memory_without_rescan() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("keep/a.txt"), b"aaaa");
    write(root.join("gone/sub/b.bin"), &[0u8; 100]);
    write(root.join("gone/c.txt"), b"cc");
    let mut tree = scan(&root);
    let gone = find_required(&tree, &root, "gone");
    let plan = build_delete_plan(&tree, &[gone], &base(&root)).unwrap();
    let confirmed = plan.confirm_permanent("gone").unwrap();

    let report = execute_delete_with(confirmed, &Control::new(), &no_bin);

    assert!(report.is_complete(), "summary: {}", report.summary());
    assert_eq!(report.mode, Some(DeleteMode::Permanent));
    assert_eq!(report.removed.len(), 4);
    assert_eq!(report.removed_files, 2);
    assert_eq!(report.removed_directories, 2);
    assert!(!root.join("gone").exists());
    assert!(root.join("keep/a.txt").exists());

    let detached = apply_delete(&mut tree, &report);
    assert_eq!(detached, 4);
    assert!(find(&tree, &root, "gone").is_none());
    assert!(find(&tree, &root, "keep/a.txt").is_some());
    let totals = tree.node(tree.root()).unwrap().totals();
    assert_eq!(totals.logical, 4, "memory matches disk after the delete");
    assert_eq!(totals.files, 1);
}

#[test]
fn partial_failure_is_reported_per_path_and_keeps_the_failed_entry() {
    if running_as_root() {
        eprintln!("skipped: root ignores the permission bits this test depends on");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("dir/ok.txt"), b"ok");
    write(root.join("dir/locked.txt"), b"locked");
    let mut tree = scan(&root);
    let dir = find_required(&tree, &root, "dir");

    // A file held open without FILE_SHARE_DELETE cannot be removed (sharing violation,
    // reported as Locked). Rust's remove_file clears read-only flags, so read-only
    // alone does not fail on Windows. On Unix a read-only folder blocks removal of
    // everything inside it. Each setup gives one real partial failure.
    let locked = root.join("dir/locked.txt");
    #[cfg(windows)]
    let lock_handle = {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE: deliberately without FILE_SHARE_DELETE.
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&locked)
            .unwrap()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("dir"), fs::Permissions::from_mode(0o555)).unwrap();
    }

    let plan = build_delete_plan(&tree, &[dir], &base(&root)).unwrap();
    let report = execute_delete_with(
        plan.confirm_permanent("dir").unwrap(),
        &Control::new(),
        &no_bin,
    );

    // Release the lock and restore permissions so the temp dir can be cleaned up.
    #[cfg(windows)]
    drop(lock_handle);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("dir"), fs::Permissions::from_mode(0o755)).unwrap();
    }

    assert!(!report.is_complete(), "a partial run must not be complete");
    assert!(
        report.summary().starts_with("Partial"),
        "{}",
        report.summary()
    );
    assert!(
        report.failures.iter().any(|f| f.path == locked),
        "the failed file is listed by path: {:?}",
        report.failures
    );
    assert!(
        report
            .failures
            .iter()
            .any(|f| f.reason == FailureReason::Blocked && f.path == root.join("dir")),
        "the folder that holds it is kept and listed"
    );
    assert!(locked.exists(), "the failed file is still on disk");
    apply_delete(&mut tree, &report);
    assert!(
        find(&tree, &root, "dir/locked.txt").is_some(),
        "memory keeps what is still on disk"
    );
    assert!(find(&tree, &root, "dir").is_some());
}

#[test]
fn cancel_before_start_removes_nothing_and_counts_every_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("gone/a.txt"), b"a");
    write(root.join("gone/b.txt"), b"b");
    let tree = scan(&root);
    let gone = find_required(&tree, &root, "gone");
    let plan = build_delete_plan(&tree, &[gone], &base(&root)).unwrap();
    let control = Control::new();
    control.cancel();

    let report = execute_delete_with(plan.confirm_permanent("gone").unwrap(), &control, &no_bin);

    assert!(report.cancelled);
    assert!(report.removed.is_empty());
    assert_eq!(report.not_attempted, 3, "two files and the folder");
    assert!(!report.is_complete());
    assert!(report.summary().contains("cancelled"));
    assert!(root.join("gone/a.txt").exists() && root.join("gone/b.txt").exists());
}

#[test]
fn recycle_mode_sends_each_root_once_and_updates_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    let bin = tmp.path().join("fake_bin");
    fs::create_dir_all(&bin).unwrap();
    write(root.join("one/a.txt"), b"aa");
    write(root.join("two/b.txt"), b"bbb");
    let mut tree = scan(&root);
    let one = find_required(&tree, &root, "one");
    let two = find_required(&tree, &root, "two");
    let plan = build_delete_plan(&tree, &[one, two], &base(&root)).unwrap();

    // The stand-in moves each item into a bin folder, as the real bin would.
    let to_bin = |path: &Path| -> Result<(), trash::Error> {
        let name = path.file_name().unwrap();
        fs::rename(path, bin.join(name)).map_err(|e| trash::Error::Unknown {
            description: e.to_string(),
        })
    };
    let report = execute_delete_with(plan.confirm_recycle_bin(), &Control::new(), &to_bin);

    assert!(report.is_complete(), "{}", report.summary());
    assert_eq!(report.mode, Some(DeleteMode::RecycleBin));
    assert!(bin.join("one").is_dir());
    assert!(bin.join("two").is_dir());
    apply_delete(&mut tree, &report);
    assert_eq!(tree.node(tree.root()).unwrap().totals().files, 0);
}

#[test]
fn recycle_failure_never_falls_back_to_permanent_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("stubborn.txt"), b"keep me");
    let tree = scan(&root);
    let stubborn = find_required(&tree, &root, "stubborn.txt");
    let plan = build_delete_plan(&tree, &[stubborn], &base(&root)).unwrap();
    let refuse = |_: &Path| -> Result<(), trash::Error> {
        Err(trash::Error::Unknown {
            description: "no Recycle Bin on this volume".into(),
        })
    };

    let report = execute_delete_with(plan.confirm_recycle_bin(), &Control::new(), &refuse);

    assert!(!report.is_complete());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].reason, FailureReason::RecycleBin);
    assert!(
        root.join("stubborn.txt").exists(),
        "the file was not deleted"
    );
    assert!(report.removed.is_empty());
}

#[test]
fn cancel_between_recycle_roots_leaves_the_rest_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    let bin = tmp.path().join("fake_bin");
    fs::create_dir_all(&bin).unwrap();
    write(root.join("first.txt"), b"1");
    write(root.join("second.txt"), b"2");
    let tree = scan(&root);
    let first = find_required(&tree, &root, "first.txt");
    let second = find_required(&tree, &root, "second.txt");
    let plan = build_delete_plan(&tree, &[first, second], &base(&root)).unwrap();
    let control = Control::new();
    let stop_after_first = |path: &Path| -> Result<(), trash::Error> {
        let name = path.file_name().unwrap();
        fs::rename(path, bin.join(name)).map_err(|e| trash::Error::Unknown {
            description: e.to_string(),
        })?;
        control.cancel();
        Ok(())
    };

    let report = execute_delete_with(plan.confirm_recycle_bin(), &control, &stop_after_first);

    assert!(report.cancelled);
    assert_eq!(report.removed.len(), 1);
    assert_eq!(report.not_attempted, 1);
    assert!(!report.is_complete());
    assert!(
        root.join("second.txt").exists(),
        "cancelled before its turn"
    );
}

#[cfg(unix)]
#[test]
fn symlink_is_removed_as_a_link_and_its_target_is_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("scan");
    write(root.join("target/data.txt"), b"precious");
    write(root.join("holder/x.txt"), b"x");
    std::os::unix::fs::symlink(root.join("target"), root.join("holder/link")).unwrap();
    let mut tree = scan(&root);
    let holder = find_required(&tree, &root, "holder");
    let plan = build_delete_plan(&tree, &[holder], &base(&root)).unwrap();
    let report = execute_delete_with(
        plan.confirm_permanent("holder").unwrap(),
        &Control::new(),
        &no_bin,
    );
    assert!(report.is_complete(), "{}", report.summary());
    assert!(
        root.join("target/data.txt").exists(),
        "the link target survives"
    );
    apply_delete(&mut tree, &report);
}
