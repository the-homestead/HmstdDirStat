mod common;

use common::{FileSpec, Fixture, analyse};
use timberstat_filesystem::NodeId;
use timberstat_storage_insights::InsightAction;

#[test]
fn no_action_ever_deletes_fixture_paths() {
    let mut fixture = Fixture::new();
    let temp = fixture.dir(NodeId::ROOT, "Temp");
    fixture.file(temp, FileSpec::new("important.tmp", 64));
    fixture.dir(NodeId::ROOT, "Empty");
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    assert_eq!(report.item_count(), 2);
    for item in &report.items {
        assert_eq!(item.action, InsightAction::ReviewOnly);
        assert_eq!(item.action.label(), "review only");
        assert!(
            item.path.exists(),
            "analysis changed or removed {}",
            item.path.display()
        );
    }
    assert!(_temp.path().exists());
}
