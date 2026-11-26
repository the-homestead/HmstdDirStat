mod common;

use std::path::{Path, PathBuf};

use common::{FileState, ROOT, directory, file_state, root_entry, scan};
use proptest::prelude::*;
use timberstat_scan_comparison::{ChangeCategory, EntryChange, compare};
use timberstat_scan_history::Scan;

type FlatCase = (Option<FileState>, Option<FileState>);
type NestedCase = (u8, Option<FileState>, Option<FileState>);

fn state() -> impl Strategy<Value = FileState> {
    (any::<u64>(), any::<i64>()).prop_map(|(logical, modified)| FileState {
        logical,
        physical: Some(logical),
        modified: Some(modified),
        identity: None,
        hardlink_duplicate: false,
    })
}

fn flat_cases() -> impl Strategy<Value = Vec<FlatCase>> {
    prop::collection::vec(
        (prop::option::of(state()), prop::option::of(state())),
        0..24,
    )
}

fn nested_cases() -> impl Strategy<Value = Vec<NestedCase>> {
    prop::collection::vec(
        (
            0_u8..=2,
            prop::option::of(state()),
            prop::option::of(state()),
        ),
        0..24,
    )
}

fn flat_scan(timestamp: i64, cases: &[FlatCase], before: bool) -> Scan {
    let mut entries = vec![root_entry()];
    for (index, case) in cases.iter().enumerate() {
        let state = if before { case.0 } else { case.1 };
        if let Some(state) = state {
            entries.push(file_state(&format!("f{index}"), Some(0), state));
        }
    }
    scan(ROOT, timestamp, entries)
}

fn nested_scan(timestamp: i64, cases: &[NestedCase], before: bool) -> Scan {
    let mut entries = vec![
        root_entry(),
        directory("a", Some(0)),
        directory("b", Some(0)),
    ];
    for (index, case) in cases.iter().enumerate() {
        let state = if before { case.1 } else { case.2 };
        if let Some(state) = state {
            entries.push(file_state(
                &format!("f{index}"),
                Some(u32::from(case.0)),
                state,
            ));
        }
    }
    scan(ROOT, timestamp, entries)
}

fn expected_category(
    before: Option<FileState>,
    after: Option<FileState>,
) -> Option<ChangeCategory> {
    match (before, after) {
        (None, Some(_)) => Some(ChangeCategory::New),
        (Some(_), None) => Some(ChangeCategory::Deleted),
        (Some(before), Some(after))
            if before.logical != after.logical || before.modified != after.modified =>
        {
            Some(ChangeCategory::Modified)
        }
        _ => None,
    }
}

fn nested_total(cases: &[NestedCase], directory: Option<u8>, before: bool) -> u128 {
    cases
        .iter()
        .filter(|case| match directory {
            None | Some(0) => true, // the root subtree includes both child directories
            Some(directory) => case.0 == directory,
        })
        .filter_map(|case| if before { case.1 } else { case.2 })
        .fold(0_u128, |total, state| total + u128::from(state.logical))
}

fn nested_path(directory: u8) -> PathBuf {
    match directory {
        0 => PathBuf::from(ROOT),
        1 => Path::new(ROOT).join("a"),
        2 => Path::new(ROOT).join("b"),
        _ => unreachable!(),
    }
}

proptest! {
    #[test]
    fn diff_categories_follow_path_membership_and_metadata(cases in flat_cases()) {
        let before = flat_scan(10, &cases, true);
        let after = flat_scan(20, &cases, false);
        let comparison = compare(&before, &after).unwrap();

        let expected_count = cases
            .iter()
            .filter(|case| expected_category(case.0, case.1).is_some())
            .count();
        prop_assert_eq!(comparison.changes.len(), expected_count);
        for (index, case) in cases.iter().enumerate() {
            let actual = comparison
                .change_at(&Path::new(ROOT).join(format!("f{index}")))
                .map(|change| change.category);
            prop_assert_eq!(actual, expected_category(case.0, case.1));
        }
        prop_assert!(comparison.logical_delta_reconciles());
        prop_assert_eq!(comparison.physical_delta_reconciles(), Some(true));
    }

    #[test]
    fn nested_entry_and_directory_deltas_reconcile(cases in nested_cases()) {
        let before = nested_scan(10, &cases, true);
        let after = nested_scan(20, &cases, false);
        let comparison = compare(&before, &after).unwrap();

        for directory in 0..=2 {
            let expected_before = nested_total(&cases, Some(directory), true);
            let expected_after = nested_total(&cases, Some(directory), false);
            let actual = comparison.directory_at(&nested_path(directory)).unwrap();
            prop_assert_eq!(actual.delta.before.logical, expected_before);
            prop_assert_eq!(actual.delta.after.logical, expected_after);
            prop_assert_eq!(
                actual.delta.logical,
                i128::try_from(expected_after).unwrap() - i128::try_from(expected_before).unwrap()
            );
        }

        let expected_before = nested_total(&cases, None, true);
        let expected_after = nested_total(&cases, None, false);
        prop_assert_eq!(comparison.total.before.logical, expected_before);
        prop_assert_eq!(comparison.total.after.logical, expected_after);
        prop_assert_eq!(
            comparison.changes.iter().map(EntryChange::logical_delta).sum::<i128>(),
            comparison.total.logical
        );
        prop_assert!(comparison.logical_delta_reconciles());
        prop_assert_eq!(comparison.physical_delta_reconciles(), Some(true));
    }
}
