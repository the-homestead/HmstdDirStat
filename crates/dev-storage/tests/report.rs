mod common;

use common::{Fixture, analyse};
use timberstat_dev_storage::{
    Category, DevLocationSet, HistoryStatus, IssueKind, LocationRole, ReclaimableBasis,
    analyse_with_locations,
};
use timberstat_filesystem::NodeId;

#[test]
fn report_exposes_totals_largest_items_distribution_and_explicit_history_state() {
    let mut fixture = Fixture::new();
    for (name, size) in [("small-repo", 100), ("large-repo", 300)] {
        let repo = fixture.dir(NodeId::ROOT, name);
        fixture.dir(repo, ".git");
        fixture.file(repo, "source.rs", size);
    }
    let small = fixture.dirs(NodeId::ROOT, &["small-build", "debug"]);
    fixture.file(small, "small.rlib", 100);
    let large = fixture.dirs(NodeId::ROOT, &["large-build", "release"]);
    fixture.file(large, "large.rlib", 300);

    let (temp, tree) = fixture.finish();
    let report = analyse_with_locations(&tree, temp.path(), &DevLocationSet::new());
    assert_eq!(report.historical_growth, HistoryStatus::NotAvailable);
    assert_eq!(report.history_status(), "history not available");
    assert_eq!(
        report.category_totals[&Category::GitRepositories].logical_size,
        400
    );
    assert_eq!(
        report.category_totals[&Category::RustBuildArtifacts].logical_size,
        400
    );
    assert_eq!(
        report.category_totals[&Category::RustBuildArtifacts].item_count,
        2
    );

    let repositories = report.largest_repositories(2);
    assert_eq!(repositories.len(), 2);
    assert!(repositories[0].path.ends_with("large-repo"));
    assert!(repositories[1].path.ends_with("small-repo"));
    let artifacts = report.largest_build_artifacts(1);
    assert_eq!(artifacts.len(), 1);
    assert!(artifacts[0].path.ends_with("large-build/release"));
    assert!(
        report
            .cleanup_candidates()
            .iter()
            .all(|item| item.category != Category::GitRepositories)
    );

    assert_eq!(report.directory_distribution.len(), 4);
    assert!(
        report
            .directory_distribution
            .windows(2)
            .all(|pair| { pair[0].logical_size >= pair[1].logical_size })
    );
    assert!(
        report
            .directory_distribution
            .iter()
            .all(|entry| entry.path.exists())
    );
}

#[test]
fn unknown_known_locations_and_unreadable_nodes_are_explicit_issues() {
    let mut fixture = Fixture::new();
    let unreadable = fixture.dir(NodeId::ROOT, "unreadable");
    fixture.mark_unreadable(unreadable);
    let missing = fixture.root_path().join("missing-cache");
    let locations =
        DevLocationSet::new().add_directory(LocationRole::NpmCache, &missing, "missing test cache");
    let (temp, tree) = fixture.finish();
    let report = analyse_with_locations(&tree, temp.path(), &locations);

    assert!(report.issues.iter().any(|issue| {
        issue.kind == IssueKind::Unreadable && issue.path.ends_with("unreadable")
    }));
    assert!(report.issues.iter().any(|issue| {
        issue.kind == IssueKind::UnknownLocation
            && issue.path == missing
            && issue.message.contains("existence and size are unknown")
    }));
}

#[test]
fn unknown_allocation_is_not_silently_summed_as_a_complete_total() {
    let mut fixture = Fixture::new();
    let cache = fixture.dir(NodeId::ROOT, "npm-cache");
    fixture.file_with_physical(cache, "data.bin", 20, None);
    let (_temp, report) = analyse(fixture);

    let total = &report.category_totals[&Category::NpmCaches];
    assert_eq!(total.logical_size, 20);
    assert_eq!(total.allocated_size, None);
    let item = report
        .items
        .iter()
        .find(|item| item.category == Category::NpmCaches)
        .expect("npm item");
    assert_eq!(item.reclaimable_size, 20);
    assert_eq!(item.reclaimable_basis, ReclaimableBasis::LogicalUpperBound);
}
