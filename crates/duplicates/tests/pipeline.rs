//! End-to-end tests for the duplicate pipeline. Every fixture lives in a temp
//! directory that the test creates and removes.

use std::fs;
use std::path::{Path, PathBuf};

use timberstat_duplicates::{Options, Progress, RunStatus, Stage, find_duplicates};
use timberstat_fileops::{FailureReason, WorkItem};
use timberstat_filesystem::{EntryKind, NodeId};

const KIB: usize = 1024;

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write fixture");
    path
}

/// Work items for on-disk files, sized from disk. Mirrors what
/// `collect_items` produces from a scanned tree.
fn items_for(paths: &[PathBuf]) -> Vec<WorkItem> {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| WorkItem {
            id: NodeId::from_index(index + 1).expect("node id"),
            parent: Some(NodeId::ROOT),
            path: path.clone(),
            kind: EntryKind::File,
            logical: fs::metadata(path).expect("metadata").len(),
            physical: None,
            identity: None,
        })
        .collect()
}

fn options() -> Options {
    Options {
        threads: 2,
        ..Options::default()
    }
}

fn run(paths: &[PathBuf]) -> timberstat_duplicates::DuplicateReport {
    let progress = Progress::new();
    let report = find_duplicates(&items_for(paths), &options(), &progress);
    assert_eq!(progress.stage(), Stage::Done, "run must finish in Done");
    report
}

fn names(group: &timberstat_duplicates::DuplicateGroup) -> Vec<String> {
    group
        .members
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
fn identical_files_form_one_group_with_reclaimable_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = vec![7u8; 200 * KIB];
    let paths = vec![
        write(dir.path(), "a.bin", &data),
        write(dir.path(), "b.bin", &data),
        write(dir.path(), "c.bin", &data),
    ];
    let report = run(&paths);
    assert_eq!(report.status, RunStatus::Complete);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.groups.len(), 1);
    let group = &report.groups[0];
    assert_eq!(names(group), ["a.bin", "b.bin", "c.bin"]);
    assert_eq!(group.size, data.len() as u64);
    // (count - 1) * size
    assert_eq!(group.reclaimable(), 2 * data.len() as u64);
    assert_eq!(report.reclaimable_total(), 2 * data.len() as u64);
    assert_eq!(report.duplicate_file_count(), 3);
}

#[test]
fn same_size_different_content_is_not_grouped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = vec![1u8; 10 * KIB];
    let mut b = a.clone();
    b[0] = 2;
    let paths = vec![
        write(dir.path(), "a.bin", &a),
        write(dir.path(), "b.bin", &b),
    ];
    let report = run(&paths);
    assert!(report.groups.is_empty(), "{:?}", report.groups);
    assert_eq!(report.files_considered, 2);
}

#[test]
fn difference_outside_sampled_windows_is_caught_by_full_hash() {
    // Same size, same first and last 64 KiB, different middle byte. The
    // sample stage cannot tell them apart; the full hash and byte verify must.
    let dir = tempfile::tempdir().expect("tempdir");
    let size = 300 * KIB;
    let base = vec![9u8; size];
    let mut middle = base.clone();
    middle[150_000] = 0;
    let paths = vec![
        write(dir.path(), "base.bin", &base),
        write(dir.path(), "middle.bin", &middle),
        write(dir.path(), "base-copy.bin", &base),
    ];
    let report = run(&paths);
    assert_eq!(report.groups.len(), 1, "{:?}", report.groups);
    assert_eq!(names(&report.groups[0]), ["base-copy.bin", "base.bin"]);
}

#[test]
fn hard_links_are_not_counted_as_copies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = vec![5u8; 50 * KIB];
    let original = write(dir.path(), "original.bin", &data);
    let link = dir.path().join("link.bin");
    fs::hard_link(&original, &link).expect("hard link on the temp volume");
    let copy = write(dir.path(), "copy.bin", &data);

    let report = run(&[original.clone(), link.clone(), copy]);
    assert_eq!(report.hardlinks_skipped, 1, "{report:?}");
    assert_eq!(report.groups.len(), 1, "{:?}", report.groups);
    let group = &report.groups[0];
    // One link and the separate copy: two real objects, one copy to reclaim.
    assert_eq!(group.members.len(), 2);
    assert_eq!(group.reclaimable(), data.len() as u64);
}

#[test]
fn only_hard_links_form_no_group() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = vec![3u8; 4 * KIB];
    let original = write(dir.path(), "one.bin", &data);
    let link = dir.path().join("two.bin");
    fs::hard_link(&original, &link).expect("hard link");
    let report = run(&[original, link]);
    assert!(report.groups.is_empty(), "{:?}", report.groups);
    assert_eq!(report.hardlinks_skipped, 1);
}

#[test]
fn zero_byte_files_ignored_by_default_and_reclaim_nothing_when_included() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = vec![
        write(dir.path(), "e1.bin", b""),
        write(dir.path(), "e2.bin", b""),
    ];
    let default_report = run(&paths);
    assert!(default_report.groups.is_empty());

    let progress = Progress::new();
    let with_empty = Options {
        min_size: 0,
        threads: 2,
    };
    let report = find_duplicates(&items_for(&paths), &with_empty, &progress);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].size, 0);
    assert_eq!(report.groups[0].reclaimable(), 0);
}

#[test]
fn groups_are_sorted_by_reclaimable_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Two 10 KiB copies reclaim 10 KiB; four 4 KiB copies reclaim 12 KiB.
    let big = vec![1u8; 10 * KIB];
    let small = vec![2u8; 4 * KIB];
    let mut paths = vec![
        write(dir.path(), "big1.bin", &big),
        write(dir.path(), "big2.bin", &big),
    ];
    for i in 0..4 {
        paths.push(write(dir.path(), &format!("small{i}.bin"), &small));
    }
    let report = run(&paths);
    assert_eq!(report.groups.len(), 2);
    assert_eq!(report.groups[0].members.len(), 4);
    assert_eq!(report.groups[0].reclaimable(), 3 * 4 * KIB as u64);
    assert_eq!(report.groups[1].reclaimable(), 10 * KIB as u64);
    assert_eq!(report.reclaimable_total(), 22 * KIB as u64);
}

#[test]
fn cancelled_run_returns_no_groups() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = vec![4u8; 8 * KIB];
    let paths = vec![
        write(dir.path(), "a.bin", &data),
        write(dir.path(), "b.bin", &data),
    ];
    let progress = Progress::new();
    progress.cancel();
    let report = find_duplicates(&items_for(&paths), &options(), &progress);
    assert_eq!(report.status, RunStatus::Cancelled);
    assert!(report.groups.is_empty());
    assert_eq!(progress.stage(), Stage::Done);
}

#[cfg(windows)]
#[test]
fn locked_file_is_reported_and_left_out_of_groups() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let data = vec![8u8; 16 * KIB];
    let a = write(dir.path(), "a.bin", &data);
    let b = write(dir.path(), "b.bin", &data);
    let locked_path = write(dir.path(), "locked.bin", &data);
    // share_mode(0): no other handle may open the file, so the pipeline's open fails.
    let _lock = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked_path)
        .expect("exclusive handle");

    let report = run(&[a, b, locked_path.clone()]);
    assert_eq!(report.groups.len(), 1, "{:?}", report.groups);
    assert_eq!(names(&report.groups[0]), ["a.bin", "b.bin"]);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, locked_path);
    assert_eq!(report.failures[0].reason, FailureReason::Locked);
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_reported_and_left_out_of_groups() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let data = vec![8u8; 16 * KIB];
    let a = write(dir.path(), "a.bin", &data);
    let b = write(dir.path(), "b.bin", &data);
    let locked_path = write(dir.path(), "locked.bin", &data);
    fs::set_permissions(&locked_path, fs::Permissions::from_mode(0o000)).expect("chmod");

    let report = run(&[a, b, locked_path.clone()]);
    fs::set_permissions(&locked_path, fs::Permissions::from_mode(0o644)).expect("restore");
    assert_eq!(report.groups.len(), 1, "{:?}", report.groups);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].reason, FailureReason::AccessDenied);
}

#[test]
fn different_sizes_are_never_grouped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = vec![
        write(dir.path(), "x.bin", &[1u8; 100]),
        write(dir.path(), "y.bin", &[1u8; 101]),
    ];
    let report = run(&paths);
    assert!(report.groups.is_empty());
    // Singleton sizes are dropped before the count, so nothing was considered.
    assert_eq!(report.files_considered, 0);
}
