mod common;

use std::path::{Path, PathBuf};

use common::{FileState, ROOT, directory, file, file_state, root_entry, scan};
use timberstat_filesystem::FileIdentity;
use timberstat_scan_comparison::{ChangeCategory, ComparisonError, compare};

fn path(suffix: &str) -> PathBuf {
    Path::new(ROOT).join(suffix)
}

fn identity(file_id: u64) -> Option<FileIdentity> {
    Some(FileIdentity { volume: 7, file_id })
}

#[test]
fn categorizes_new_deleted_modified_and_unchanged_paths() {
    let before = scan(
        ROOT,
        10,
        vec![
            root_entry(),
            directory("src", Some(0)),
            file("gone", Some(1), 7, 1),
            file("changed", Some(1), 10, 1),
            file("same", Some(1), 5, 1),
            file("stamped", Some(1), 5, 1),
        ],
    );
    let after = scan(
        ROOT,
        20,
        vec![
            root_entry(),
            directory("src", Some(0)),
            file("new", Some(1), 9, 1),
            file("changed", Some(1), 12, 1),
            file("same", Some(1), 5, 1),
            file("stamped", Some(1), 5, 2),
        ],
    );

    let comparison = compare(&before, &after).unwrap();
    assert_eq!(comparison.new_entries().count(), 1);
    assert_eq!(comparison.deleted_entries().count(), 1);
    assert_eq!(comparison.modified_entries().count(), 2);
    assert_eq!(
        comparison.change_at(&path("src/new")).unwrap().category,
        ChangeCategory::New
    );
    assert_eq!(
        comparison.change_at(&path("src/gone")).unwrap().category,
        ChangeCategory::Deleted
    );
    assert!(
        comparison
            .change_at(&path("src/changed"))
            .unwrap()
            .reasons
            .logical_size
    );
    assert!(
        comparison
            .change_at(&path("src/stamped"))
            .unwrap()
            .reasons
            .modified_time
    );
    assert!(comparison.change_at(&path("src/same")).is_none());
    assert_eq!(comparison.before_timestamp_unix_secs, 10);
    assert_eq!(comparison.after_timestamp_unix_secs, 20);
}

#[test]
fn signed_directory_deltas_reconcile_with_root_total() {
    let before = scan(
        ROOT,
        10,
        vec![
            root_entry(),
            directory("a", Some(0)),
            directory("b", Some(0)),
            file("f", Some(1), 100, 1),
            file("f", Some(2), 30, 1),
        ],
    );
    let after = scan(
        ROOT,
        20,
        vec![
            root_entry(),
            directory("a", Some(0)),
            file("f", Some(1), 130, 1),
            file("c", Some(0), 5, 1),
        ],
    );

    let comparison = compare(&before, &after).unwrap();
    assert_eq!(
        comparison.directory_at(&path("a")).unwrap().delta.logical,
        30
    );
    assert_eq!(
        comparison.directory_at(&path("b")).unwrap().delta.logical,
        -30
    );
    assert_eq!(
        comparison
            .directory_at(Path::new(ROOT))
            .unwrap()
            .delta
            .logical,
        5
    );
    assert_eq!(comparison.total.before.logical, 130);
    assert_eq!(comparison.total.after.logical, 135);
    assert_eq!(comparison.total.logical, 5);
    assert!(comparison.logical_delta_reconciles());
    assert_eq!(comparison.physical_delta_reconciles(), Some(true));
}

#[test]
fn mtime_and_recorded_identity_changes_mark_entries_modified() {
    let before = scan(
        ROOT,
        10,
        vec![
            root_entry(),
            file_state(
                "time",
                Some(0),
                FileState {
                    identity: identity(1),
                    ..FileState::new(4, 1)
                },
            ),
            file_state(
                "identity",
                Some(0),
                FileState {
                    identity: identity(1),
                    ..FileState::new(8, 1)
                },
            ),
        ],
    );
    let after = scan(
        ROOT,
        20,
        vec![
            root_entry(),
            file_state(
                "time",
                Some(0),
                FileState {
                    identity: identity(1),
                    ..FileState::new(4, 2)
                },
            ),
            file_state(
                "identity",
                Some(0),
                FileState {
                    identity: identity(2),
                    ..FileState::new(8, 1)
                },
            ),
        ],
    );

    let comparison = compare(&before, &after).unwrap();
    let time = comparison.change_at(&path("time")).unwrap();
    assert!(time.reasons.modified_time && !time.reasons.identity);
    let identity = comparison.change_at(&path("identity")).unwrap();
    assert!(identity.reasons.identity && !identity.reasons.modified_time);
}

#[test]
fn missing_metadata_is_unknown_not_a_modification() {
    let before = scan(
        ROOT,
        10,
        vec![
            root_entry(),
            file_state(
                "f",
                Some(0),
                FileState {
                    identity: identity(1),
                    ..FileState::new(4, 1)
                },
            ),
        ],
    );
    let after = scan(
        ROOT,
        20,
        vec![
            root_entry(),
            file_state(
                "f",
                Some(0),
                FileState {
                    logical: 4,
                    physical: None,
                    modified: None,
                    identity: None,
                    hardlink_duplicate: false,
                },
            ),
        ],
    );

    let comparison = compare(&before, &after).unwrap();
    assert!(comparison.changes.is_empty());
    assert_eq!(comparison.total.physical, None);
    assert_eq!(comparison.physical_delta_reconciles(), None);
}

#[test]
fn same_identity_at_a_new_path_is_delete_plus_new() {
    let before = scan(
        ROOT,
        10,
        vec![
            root_entry(),
            file_state(
                "old",
                Some(0),
                FileState {
                    identity: identity(9),
                    ..FileState::new(4, 1)
                },
            ),
        ],
    );
    let after = scan(
        ROOT,
        20,
        vec![
            root_entry(),
            file_state(
                "new",
                Some(0),
                FileState {
                    identity: identity(9),
                    ..FileState::new(4, 1)
                },
            ),
        ],
    );

    let comparison = compare(&before, &after).unwrap();
    assert_eq!(
        comparison.change_at(&path("old")).unwrap().category,
        ChangeCategory::Deleted
    );
    assert_eq!(
        comparison.change_at(&path("new")).unwrap().category,
        ChangeCategory::New
    );
}

#[test]
fn hard_link_charge_reallocation_reconciles_physical_deltas() {
    let mut a = FileState::new(10, 1);
    a.physical = Some(4096);
    a.identity = identity(3);
    let mut b = a;
    b.physical = Some(0);
    b.hardlink_duplicate = true;
    let before = scan(
        ROOT,
        10,
        vec![
            root_entry(),
            file_state("a", Some(0), a),
            file_state("b", Some(0), b),
        ],
    );
    let after = scan(
        ROOT,
        20,
        vec![
            root_entry(),
            file_state("a", Some(0), b),
            file_state("b", Some(0), a),
        ],
    );

    let comparison = compare(&before, &after).unwrap();
    assert_eq!(comparison.total.physical, Some(0));
    assert_eq!(comparison.modified_entries().count(), 2);
    assert!(comparison.logical_delta_reconciles());
    assert_eq!(comparison.physical_delta_reconciles(), Some(true));
}

#[test]
fn mismatched_roots_are_refused() {
    let before = scan(ROOT, 10, vec![root_entry()]);
    let after = scan("D:/other", 20, vec![root_entry()]);
    assert_eq!(
        compare(&before, &after).unwrap_err(),
        ComparisonError::MismatchedRoots {
            snapshot: 1,
            expected: PathBuf::from(ROOT),
            found: PathBuf::from("D:/other"),
        }
    );
}
