mod common;

use common::{FileSpec, Fixture, analyse};
use timberstat_filesystem::NodeId;
use timberstat_storage_insights::{Category, ReclaimableBasis};

#[test]
fn hard_links_are_itemised_but_allocated_reclaim_is_charged_once() {
    let mut fixture = Fixture::new();
    let temp = fixture.dir(NodeId::ROOT, "Temp");
    let first = fixture.file(
        temp,
        FileSpec::new("first.tmp", 1_000)
            .allocated(Some(4_096))
            .identity(7, 42),
    );
    let first_path = fixture.path(first).to_path_buf();
    fixture.hard_link(
        temp,
        FileSpec::new("second.tmp", 1_000)
            .allocated(Some(0))
            .identity(7, 42)
            .hardlink_duplicate(true),
        &first_path,
    );
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    let items = report.items_in(Category::TempFiles);
    assert_eq!(items.len(), 2);
    assert_eq!(report.reclaimable_total(), 4_096);
    assert_eq!(
        report.reclaimable_by_category()[&Category::TempFiles],
        4_096
    );
    assert_eq!(
        items
            .iter()
            .filter(|item| item.reclaimable_basis == ReclaimableBasis::SharedHardLink)
            .count(),
        1
    );
}

#[test]
fn a_later_charged_path_still_counts_when_duplicate_path_comes_first() {
    let mut fixture = Fixture::new();
    let temp = fixture.dir(NodeId::ROOT, "Temp");
    let original_path = fixture.path(temp).join("z-charged.tmp");
    std::fs::write(&original_path, b"fixture").expect("seed original link target");
    // The duplicate enters the synthetic arena before the path carrying charge.
    fixture.hard_link(
        temp,
        FileSpec::new("a-duplicate.tmp", 1_000)
            .allocated(Some(0))
            .identity(7, 42)
            .hardlink_duplicate(true),
        &original_path,
    );
    fixture.file(
        temp,
        FileSpec::new("z-charged.tmp", 1_000)
            .allocated(Some(4_096))
            .identity(7, 42),
    );
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    assert_eq!(report.items_in(Category::TempFiles).len(), 2);
    assert_eq!(report.reclaimable_total(), 4_096);
}

#[test]
fn unknown_allocation_uses_unique_logical_bytes_once() {
    let mut fixture = Fixture::new();
    let temp = fixture.dir(NodeId::ROOT, "Temp");
    let first = fixture.file(
        temp,
        FileSpec::new("first.tmp", 1_000)
            .allocated(None)
            .identity(7, 42),
    );
    let first_path = fixture.path(first).to_path_buf();
    fixture.hard_link(
        temp,
        FileSpec::new("second.tmp", 1_000)
            .allocated(None)
            .identity(7, 42),
        &first_path,
    );
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    assert_eq!(report.items_in(Category::TempFiles).len(), 2);
    assert_eq!(report.reclaimable_total(), 1_000);
    assert!(
        report
            .items
            .iter()
            .any(|item| { item.reclaimable_basis == ReclaimableBasis::LogicalUpperBound })
    );
    assert!(
        report
            .items
            .iter()
            .any(|item| { item.reclaimable_basis == ReclaimableBasis::SharedHardLink })
    );
}

#[test]
fn media_logical_fallback_does_not_double_count_hard_links() {
    let mut fixture = Fixture::new();
    let media = fixture.dir(NodeId::ROOT, "Media");
    let first = fixture.file(
        media,
        FileSpec::new("first.mp4", 150)
            .allocated(None)
            .identity(8, 90),
    );
    let first_path = fixture.path(first).to_path_buf();
    fixture.hard_link(
        media,
        FileSpec::new("second.mkv", 150)
            .allocated(None)
            .identity(8, 90),
        &first_path,
    );
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    let collections = report.items_in(Category::LargeMedia);
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].logical_size, 300);
    assert_eq!(collections[0].allocated_size, None);
    assert_eq!(collections[0].reclaimable_size, 150);
    assert_eq!(report.reclaimable_total(), 150);
}
