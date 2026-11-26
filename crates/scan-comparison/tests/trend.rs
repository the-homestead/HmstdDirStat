mod common;

use std::path::PathBuf;

use common::{ROOT, root_entry, scan, scan_with_file};
use timberstat_scan_comparison::{ComparisonError, growth_trend};

#[test]
fn three_snapshots_produce_ordered_growth_intervals() {
    let first = scan_with_file(ROOT, 100, 10);
    let second = scan_with_file(ROOT, 200, 25);
    let third = scan_with_file(ROOT, 300, 15);

    let trend = growth_trend(&[&first, &second, &third]).unwrap();
    assert_eq!(trend.root, PathBuf::from(ROOT));
    assert_eq!(trend.snapshots.len(), 3);
    assert_eq!(trend.intervals.len(), 2);
    assert_eq!(
        trend
            .snapshots
            .iter()
            .map(|snapshot| snapshot.total.logical)
            .collect::<Vec<_>>(),
        vec![10, 25, 15]
    );
    assert_eq!(trend.intervals[0].delta.logical, 15);
    assert_eq!(trend.intervals[1].delta.logical, -10);
    assert_eq!(trend.intervals[0].from_timestamp_unix_secs, 100);
    assert_eq!(trend.intervals[0].to_timestamp_unix_secs, 200);
    assert_eq!(
        trend.intervals[1].delta.after.logical - trend.intervals[0].delta.before.logical,
        5
    );
}

#[test]
fn trend_rejects_fewer_than_three_snapshots() {
    let first = scan_with_file(ROOT, 100, 10);
    let second = scan_with_file(ROOT, 200, 20);
    assert_eq!(
        growth_trend(&[&first, &second]).unwrap_err(),
        ComparisonError::TooFewSnapshots {
            actual: 2,
            minimum: 3,
        }
    );
}

#[test]
fn trend_rejects_a_later_mismatched_root() {
    let first = scan_with_file(ROOT, 100, 10);
    let second = scan_with_file(ROOT, 200, 20);
    let third = scan("D:/other", 300, vec![root_entry()]);
    assert_eq!(
        growth_trend(&[&first, &second, &third]).unwrap_err(),
        ComparisonError::MismatchedRoots {
            snapshot: 2,
            expected: PathBuf::from(ROOT),
            found: PathBuf::from("D:/other"),
        }
    );
}
