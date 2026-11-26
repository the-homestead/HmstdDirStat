//! Scanner integration tests: real directories built in a temp dir.
//!
//! Cases that need an environment this machine may not have (symlink privilege,
//! non-root permission checks, hard-link support) print `NOT RUN` with the reason
//! and return. They never report a pass for a check that did not execute.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use timberstat_filesystem::{EntryKind, FileTree, NodeId};
use timberstat_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanOutput,
    ScanProgress,
};

fn write_bytes(path: &Path, len: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = fs::File::create(path).unwrap();
    file.write_all(&vec![b'x'; len]).unwrap();
}

fn scan(root: &Path) -> ScanOutput {
    GenericScanner
        .scan(
            root,
            &ScanOptions::default(),
            &CancelToken::new(),
            &mut |_| {},
        )
        .expect("scan should succeed")
}

fn child_named(tree: &FileTree, parent: NodeId, name: &str) -> Option<NodeId> {
    tree.children(parent)
        .find(|id| tree.node(*id).unwrap().name() == std::ffi::OsStr::new(name))
}

fn not_run(test: &str, reason: &str) {
    eprintln!("NOT RUN: {test}: {reason}");
}

#[test]
fn empty_directory_scans_to_root_only() {
    let dir = tempfile::tempdir().unwrap();
    let out = scan(dir.path());
    assert_eq!(out.tree.len(), 1);
    let totals = out.tree.node(NodeId::ROOT).unwrap().totals();
    assert_eq!(totals.logical, 0);
    assert_eq!(totals.files, 0);
    assert_eq!(totals.directories, 1);
    assert!(totals.complete);
    assert!(out.report.issues.is_empty());
}

#[test]
fn totals_equal_manually_summed_sizes() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("a.bin"), 10);
    write_bytes(&dir.path().join("sub/b.bin"), 200);
    write_bytes(&dir.path().join("sub/deeper/c.bin"), 0);
    let out = scan(dir.path());
    let totals = out.tree.node(NodeId::ROOT).unwrap().totals();
    assert_eq!(totals.logical, 210);
    assert_eq!(totals.files, 3);
    assert_eq!(totals.directories, 3, "root, sub, deeper");
    assert!(totals.complete);

    let sub = child_named(&out.tree, NodeId::ROOT, "sub").unwrap();
    assert_eq!(out.tree.node(sub).unwrap().totals().logical, 200);
}

#[test]
fn metadata_is_captured_for_files() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("m.bin"), 5);
    let out = scan(dir.path());
    let file = child_named(&out.tree, NodeId::ROOT, "m.bin").unwrap();
    let node = out.tree.node(file).unwrap();
    assert_eq!(node.kind(), EntryKind::File);
    assert!(
        node.metadata().modified_unix_secs.is_some(),
        "mtime should be set"
    );
    #[cfg(unix)]
    assert!(node.metadata().unix_mode.is_some());
    #[cfg(windows)]
    assert!(node.metadata().windows_attributes.is_some());
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    // Windows MAX_PATH limits how deep a path can be without long-path support.
    let depth: usize = if cfg!(windows) { 40 } else { 1000 };
    let dir = tempfile::tempdir().unwrap();
    let mut path = dir.path().to_path_buf();
    for _ in 0..depth {
        path.push("d");
    }
    fs::create_dir_all(&path).unwrap();
    write_bytes(&path.join("leaf.bin"), 42);

    let out = scan(dir.path());
    let totals = out.tree.node(NodeId::ROOT).unwrap().totals();
    assert_eq!(totals.logical, 42);
    assert_eq!(totals.directories, depth as u64 + 1);
}

#[test]
fn unicode_and_space_names_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let name = "naïve 日本 file.txt";
    write_bytes(&dir.path().join(name), 3);
    let out = scan(dir.path());
    let id = child_named(&out.tree, NodeId::ROOT, name).expect("unicode name should be found");
    assert_eq!(out.tree.node(id).unwrap().name().to_str(), Some(name));
}

#[test]
fn excluded_subtree_is_absent_and_not_counted() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("keep.bin"), 7);
    write_bytes(&dir.path().join("skip/hidden.bin"), 1000);
    let options = ScanOptions {
        exclude: vec![dir.path().join("skip")],
        ..ScanOptions::default()
    };
    let out = GenericScanner
        .scan(dir.path(), &options, &CancelToken::new(), &mut |_| {})
        .unwrap();
    assert!(child_named(&out.tree, NodeId::ROOT, "skip").is_none());
    assert_eq!(out.tree.node(NodeId::ROOT).unwrap().totals().logical, 7);
}

#[test]
fn cancel_before_start_returns_cancelled_and_no_tree() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("a.bin"), 1);
    let token = CancelToken::new();
    token.cancel();
    let result = GenericScanner.scan(dir.path(), &ScanOptions::default(), &token, &mut |_| {});
    assert!(matches!(result, Err(ScanError::Cancelled)));
}

#[test]
fn cancel_mid_scan_never_produces_a_complete_tree() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..5_000 {
        write_bytes(&dir.path().join(format!("f{i:05}.bin")), 1);
    }
    let token = CancelToken::new();
    let trigger = token.clone();
    let result = GenericScanner.scan(
        dir.path(),
        &ScanOptions::default(),
        &token,
        &mut |_progress: ScanProgress| trigger.cancel(),
    );
    assert!(
        matches!(result, Err(ScanError::Cancelled)),
        "a scan cancelled mid-way must return Cancelled, not a partial tree"
    );
}

#[test]
fn progress_fires_at_interval_and_at_the_end() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..5_000 {
        write_bytes(&dir.path().join(format!("p{i:05}.bin")), 1);
    }
    let mut seen: Vec<ScanProgress> = Vec::new();
    GenericScanner
        .scan(
            dir.path(),
            &ScanOptions::default(),
            &CancelToken::new(),
            &mut |p| seen.push(p),
        )
        .unwrap();
    assert!(
        seen.len() >= 2,
        "expected an interval and a final report, got {seen:?}"
    );
    assert_eq!(seen.last().unwrap().entries_visited, 5_000);
}

#[test]
fn missing_root_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    let result = GenericScanner.scan(
        &missing,
        &ScanOptions::default(),
        &CancelToken::new(),
        &mut |_| {},
    );
    assert!(matches!(result, Err(ScanError::RootUnavailable { .. })));
}

#[test]
fn file_as_root_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.bin");
    write_bytes(&file, 1);
    let result = GenericScanner.scan(
        &file,
        &ScanOptions::default(),
        &CancelToken::new(),
        &mut |_| {},
    );
    assert!(matches!(result, Err(ScanError::RootNotDirectory(_))));
}

#[cfg(unix)]
#[test]
fn unreadable_subdirectory_is_reported_and_marks_totals_incomplete() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("visible.bin"), 9);
    let locked = dir.path().join("locked");
    write_bytes(&locked.join("secret.bin"), 1000);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // Root can read a mode-000 directory; the check would not exercise anything.
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        not_run(
            "unreadable_subdirectory",
            "process can read mode-000 directories (likely root)",
        );
        return;
    }

    let out = scan(dir.path());
    let locked_id = child_named(&out.tree, NodeId::ROOT, "locked").unwrap();
    assert!(out.tree.node(locked_id).unwrap().is_unreadable());
    assert!(!out.tree.node(NodeId::ROOT).unwrap().totals().complete);
    assert_eq!(out.report.issue_count(), 1);
    assert_eq!(out.tree.node(NodeId::ROOT).unwrap().totals().logical, 9);

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
#[test]
fn unreadable_subdirectory_is_reported_and_marks_totals_incomplete() {
    not_run(
        "unreadable_subdirectory",
        "permission bits are not portable; ACL denial needs a Windows fixture",
    );
}

#[cfg(unix)]
#[test]
fn hard_link_is_charged_physically_once() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("a.bin"), 10_000);
    fs::hard_link(dir.path().join("a.bin"), dir.path().join("b.bin")).unwrap();

    let out = scan(dir.path());
    let a = child_named(&out.tree, NodeId::ROOT, "a.bin").unwrap();
    let b = child_named(&out.tree, NodeId::ROOT, "b.bin").unwrap();
    let node_b = out.tree.node(b).unwrap();

    // Logical: each path counts its full size.
    assert_eq!(
        out.tree.node(NodeId::ROOT).unwrap().totals().logical,
        20_000
    );
    // Physical: exactly one of the two paths is charged; traversal order decides which.
    let node_a = out.tree.node(a).unwrap();
    let charged = [node_a, node_b]
        .iter()
        .filter(|node| !node.is_hardlink_duplicate())
        .count();
    assert_eq!(charged, 1, "exactly one path carries the physical charge");
    let duplicate = if node_b.is_hardlink_duplicate() {
        node_b
    } else {
        node_a
    };
    assert_eq!(duplicate.physical_size(), Some(0));
    assert_eq!(out.report.hardlink_duplicates, 1);
    let root = out.tree.node(NodeId::ROOT).unwrap();
    let physical_paths = node_a.physical_size().unwrap() + node_b.physical_size().unwrap();
    assert_eq!(
        root.totals().physical,
        root.physical_size().unwrap() + physical_paths,
        "shared file must be counted once"
    );
}

#[cfg(windows)]
#[test]
fn hard_link_is_counted_logically_per_path_and_physical_unknown() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("a.bin"), 10_000);
    fs::hard_link(dir.path().join("a.bin"), dir.path().join("b.bin")).unwrap();
    let out = scan(dir.path());
    let totals = out.tree.node(NodeId::ROOT).unwrap().totals();
    assert_eq!(totals.logical, 20_000);
    assert!(
        !totals.physical_known,
        "Windows file identity is Phase 3; physical must not be claimed"
    );
}

#[cfg(unix)]
#[test]
fn symlink_is_recorded_but_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("target/real.bin"), 1000);
    if let Err(error) =
        std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("link"))
    {
        not_run("symlink_not_followed", &error.to_string());
        return;
    }
    let out = scan(dir.path());
    let link = child_named(&out.tree, NodeId::ROOT, "link").unwrap();
    let node = out.tree.node(link).unwrap();
    assert_eq!(node.kind(), EntryKind::Symlink);
    assert_eq!(
        out.tree.children(link).count(),
        0,
        "symlink must not be entered"
    );
    // Only the real file counts once, through its real directory.
    assert_eq!(out.tree.node(NodeId::ROOT).unwrap().totals().logical, 1000);
}

#[cfg(windows)]
#[test]
fn symlink_is_recorded_but_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(&dir.path().join("target/real.bin"), 1000);
    if let Err(error) =
        std::os::windows::fs::symlink_dir(dir.path().join("target"), dir.path().join("link"))
    {
        not_run(
            "symlink_not_followed",
            &format!("symlink creation needs privilege or Developer Mode: {error}"),
        );
        return;
    }
    let out = scan(dir.path());
    let link = child_named(&out.tree, NodeId::ROOT, "link").unwrap();
    assert_eq!(out.tree.node(link).unwrap().kind(), EntryKind::Symlink);
    assert_eq!(out.tree.children(link).count(), 0);
    assert_eq!(out.tree.node(NodeId::ROOT).unwrap().totals().logical, 1000);
}

#[test]
fn long_paths_are_scanned() {
    let dir = tempfile::tempdir().unwrap();
    let mut path = dir.path().to_path_buf();
    for _ in 0..6 {
        path.push("a_directory_name_of_moderate_length");
    }
    fs::create_dir_all(&path).unwrap();
    write_bytes(&path.join("deep.bin"), 77);
    if path.as_os_str().len() <= 260 {
        eprintln!(
            "note: long-path fixture is only {} chars",
            path.as_os_str().len()
        );
    }
    let out = scan(dir.path());
    assert_eq!(out.tree.node(NodeId::ROOT).unwrap().totals().logical, 77);
}

#[test]
#[ignore = "scale check: run with --ignored in release mode and record the timing"]
fn hundred_thousand_files_scan() {
    let dir = tempfile::tempdir().unwrap();
    for shard in 0..100 {
        let sub: PathBuf = dir.path().join(format!("s{shard:03}"));
        fs::create_dir_all(&sub).unwrap();
        for i in 0..1_000 {
            write_bytes(&sub.join(format!("f{i:04}.bin")), 16);
        }
    }
    let started = std::time::Instant::now();
    let out = scan(dir.path());
    eprintln!("scanned 100000 files in {:?}", started.elapsed());
    assert_eq!(out.tree.node(NodeId::ROOT).unwrap().totals().files, 100_000);
}
