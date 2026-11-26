mod common;

use common::{FileSpec, Fixture, analyse};
use timberstat_filesystem::NodeId;
use timberstat_storage_insights::{Category, InsightAction, ReclaimableBasis};

#[test]
fn every_category_is_found_in_a_temp_dir_synthetic_tree() {
    let mut fixture = Fixture::new();
    let root = NodeId::ROOT;

    let temp = fixture.dir(root, "Temp");
    fixture.file(temp, FileSpec::new("work.tmp", 11).allocated(Some(16)));
    let app_cache = fixture.dir(root, "AppCache");
    fixture.file(app_cache, FileSpec::new("state.bin", 12));
    let browser = fixture.dirs(root, &["Chrome", "Default", "Cache"]);
    fixture.file(browser, FileSpec::new("http.bin", 13));
    let logs = fixture.dir(root, "Logs");
    fixture.file(logs, FileSpec::new("service.log.1", 14));
    let downloads = fixture.dir(root, "Downloads");
    fixture.file(downloads, FileSpec::new("document.bin", 15).age_days(31));
    fixture.file(downloads, FileSpec::new("setup.exe", 160).age_days(60));
    let archives = fixture.dir(root, "Archives");
    fixture.file(archives, FileSpec::new("backup.tar.gz", 17).age_days(31));
    let crashes = fixture.dir(root, "CrashDumps");
    fixture.file(crashes, FileSpec::new("application.dmp", 18));
    fixture.dir(root, "Empty");
    let virtual_machines = fixture.dir(root, "VirtualMachines");
    fixture.file(virtual_machines, FileSpec::new("disk.vhdx", 190));
    let media = fixture.dir(root, "Media");
    fixture.file(media, FileSpec::new("one.mp4", 150));
    fixture.file(media, FileSpec::new("two.mkv", 150));
    let common_dir = fixture.dirs(root, &["Games", "Steam", "steamapps", "common"]);
    let game = fixture.dir(common_dir, "ExampleGame");
    fixture.file(game, FileSpec::new("game.bin", 300));

    let (_temp, tree) = fixture.finish();
    let root_path = tree_root(&_temp);
    let report = analyse(&tree, root_path, &common::options());
    let expected = [
        Category::TempFiles,
        Category::AppCache,
        Category::BrowserCache,
        Category::LogFiles,
        Category::OldDownloads,
        Category::LargeInstallers,
        Category::OldArchives,
        Category::CrashDumps,
        Category::EmptyDirectories,
        Category::VmImages,
        Category::LargeMedia,
        Category::GameInstalls,
    ];

    assert_eq!(report.item_count(), expected.len(), "{report:#?}");
    for category in expected {
        let items = report.items_in(category);
        assert_eq!(items.len(), 1, "category {category:?}: {items:#?}");
        let item = items[0];
        assert!(!item.reason.is_empty());
        assert!(!item.risk.text.is_empty());
        assert_eq!(item.action, InsightAction::ReviewOnly);
        assert!(item.path.is_absolute());
        if category != Category::EmptyDirectories {
            assert!(item.logical_size > 0, "category {category:?}");
            assert!(item.allocated_size.is_some(), "category {category:?}");
        }
    }

    let game = report.items_in(Category::GameInstalls)[0];
    assert_eq!(game.reclaimable_size, 0);
    assert_eq!(game.reclaimable_basis, ReclaimableBasis::InformationalOnly);
    let archive = report.items_in(Category::OldArchives)[0];
    assert!(archive.reason.contains("30-day threshold"));
    let media = report.items_in(Category::LargeMedia)[0];
    assert_eq!(media.logical_size, 300);
}

fn tree_root(temp: &tempfile::TempDir) -> &std::path::Path {
    temp.path()
}

#[test]
fn plain_large_executable_is_not_assumed_to_be_an_installer() {
    let mut fixture = Fixture::new();
    let programs = fixture.dir(NodeId::ROOT, "Programs");
    fixture.file(programs, FileSpec::new("tool.exe", 500));
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());
    assert!(report.items.is_empty(), "{report:#?}");
}

#[test]
fn unreadable_directory_is_not_claimed_to_be_empty() {
    let mut fixture = Fixture::new();
    let unreadable = fixture.dir(NodeId::ROOT, "Unreadable");
    fixture.mark_unreadable(unreadable);
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());
    assert!(report.items_in(Category::EmptyDirectories).is_empty());
}

#[test]
fn game_detection_requires_a_direct_child_of_the_registered_library() {
    let mut fixture = Fixture::new();
    let common_dir = fixture.dirs(NodeId::ROOT, &["Games", "Steam", "steamapps", "common"]);
    let game = fixture.dir(common_dir, "Game");
    let nested = fixture.dir(game, "Nested");
    fixture.file(nested, FileSpec::new("asset.bin", 50));
    let unrelated = fixture.dirs(NodeId::ROOT, &["Other", "steamapps", "common", "Game"]);
    fixture.file(unrelated, FileSpec::new("other.bin", 50));
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    let games = report.items_in(Category::GameInstalls);
    assert_eq!(games.len(), 1);
    assert!(games[0].path.ends_with("Games/Steam/steamapps/common/Game"));
}
