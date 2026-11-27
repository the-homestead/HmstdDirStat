//! Safety and correctness checks for the benchmark support crate.
//!
//! Dataset tests write only under the temp directory, into folders named with
//! the `timberstat-bench-test-` prefix, and remove them afterwards.

use std::path::PathBuf;

use timberstat_perf::{
    datasets::{self, DatasetError, Kind},
    run::RunRecord,
    summary,
};

fn test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "timberstat-bench-test-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn kind_ids_round_trip() {
    for kind in Kind::ALL {
        assert_eq!(Kind::parse(kind.id()), Some(kind));
    }
    assert_eq!(Kind::parse("nope"), None);
}

#[test]
fn roots_outside_temp_are_refused() {
    let outside = std::env::temp_dir()
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("timberstat-bench-refused"));
    if let Some(outside) = outside {
        // Two levels above the temp dir is never the temp dir itself.
        match datasets::check_root(&outside) {
            Err(DatasetError::OutsideTemp(..)) => {}
            other => panic!("expected OutsideTemp, got {other:?}"),
        }
    }
}

#[test]
fn roots_without_prefix_are_refused() {
    let root = std::env::temp_dir().join("not-a-bench-folder");
    match datasets::check_root(&root) {
        Err(DatasetError::BadName(_)) => {}
        other => panic!("expected BadName, got {other:?}"),
    }
}

#[test]
fn small_dataset_matches_its_manifest_and_cleans_up() {
    let root = test_root("small");
    let _ = datasets::clean(&root);
    let manifest = datasets::generate(Kind::Small, &root).expect("generate");
    assert_eq!(manifest.files, 300);
    assert_eq!(manifest.directories, 1);
    assert_eq!(manifest.expected_tree_nodes, 302);
    assert_eq!(manifest.max_depth, 1);
    assert!(!manifest.root.starts_with(r"\\?\"));
    let read_back = datasets::read_manifest(&check(&root)).expect("manifest written");
    assert_eq!(read_back, manifest);
    // A second generate on a non-empty root is refused, not overwritten.
    match datasets::generate(Kind::Small, &root) {
        Err(DatasetError::NotEmpty(_)) => {}
        other => panic!("expected NotEmpty, got {other:?}"),
    }
    datasets::clean(&root).expect("clean");
    assert!(!root.exists());
}

fn check(root: &std::path::Path) -> PathBuf {
    datasets::check_root(root).expect("root permitted")
}

#[test]
fn deep_dataset_has_more_than_200_levels() {
    let root = test_root("deep");
    let _ = datasets::clean(&root);
    let manifest = datasets::generate(Kind::Deep, &root).expect("generate");
    assert!(manifest.max_depth > 200, "max depth {}", manifest.max_depth);
    assert_eq!(manifest.files, 2 * u64::from(datasets::DEEP_LEVELS));
    datasets::clean(&root).expect("clean");
}

fn record(dataset: &str, wall: f64, peak: u64, matches: Option<bool>) -> RunRecord {
    RunRecord {
        dataset: dataset.to_owned(),
        root: String::new(),
        run: 1,
        cache_label: "warm".to_owned(),
        threads: 4,
        method: "GenericTraversal".to_owned(),
        wall_ms: wall,
        entries: 10,
        files: 9,
        directories: 1,
        issues: 0,
        entries_per_sec: 10.0 / (wall / 1000.0),
        peak_working_set_bytes: peak,
        memory_sample_interval_ms: 2.0,
        expected_tree_nodes: Some(10),
        nodes_match_manifest: matches,
    }
}

#[test]
fn summary_uses_median_and_range_not_best_run() {
    let records = vec![
        record("small", 10.0, 100, Some(true)),
        record("small", 30.0, 300, Some(true)),
        record("small", 20.0, 200, Some(true)),
        record("deep", 5.0, 50, Some(true)),
    ];
    let out = summary::summarize(&records);
    assert_eq!(out.len(), 2);
    let small = out.iter().find(|s| s.dataset == "small").expect("small");
    assert_eq!(small.samples, 3);
    assert_eq!(small.wall_ms_median, 20.0);
    assert_eq!(small.wall_ms_min, 10.0);
    assert_eq!(small.wall_ms_max, 30.0);
    assert_eq!(small.peak_working_set_bytes_median, 200);
    assert_eq!(small.all_nodes_match_manifest, Some(true));
}

#[test]
fn summary_reports_unknown_manifest_match_as_none() {
    let records = vec![record("x", 1.0, 1, None), record("x", 2.0, 2, Some(true))];
    let out = summary::summarize(&records);
    assert_eq!(out[0].all_nodes_match_manifest, None);
}

#[test]
fn median_handles_even_and_odd_counts() {
    assert_eq!(summary::median(&mut [3.0, 1.0, 2.0]), 2.0);
    assert_eq!(summary::median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    assert_eq!(summary::median(&mut []), 0.0);
}
