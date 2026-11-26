mod common;

use common::{FileSpec, Fixture, analyse};
use timberstat_filesystem::NodeId;
use timberstat_storage_insights::{Category, DAY_SECS};

#[test]
fn download_and_archive_age_thresholds_are_inclusive() {
    let mut fixture = Fixture::new();
    let downloads = fixture.dir(NodeId::ROOT, "Downloads");
    fixture.file(downloads, FileSpec::new("new.bin", 10).age_days(29));
    fixture.file(downloads, FileSpec::new("exact.bin", 10).age_days(30));
    fixture.file(downloads, FileSpec::new("new.zip", 10).age_days(29));
    fixture.file(downloads, FileSpec::new("exact.tar.gz", 10).age_days(30));
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    let downloads = report.items_in(Category::OldDownloads);
    assert_eq!(downloads.len(), 1);
    assert!(downloads[0].path.ends_with("exact.bin"));
    let archives = report.items_in(Category::OldArchives);
    assert_eq!(archives.len(), 1);
    assert!(archives[0].path.ends_with("exact.tar.gz"));
}

#[test]
fn unknown_age_is_counted_and_never_treated_as_old() {
    let mut fixture = Fixture::new();
    let downloads = fixture.dir(NodeId::ROOT, "Downloads");
    fixture.file(downloads, FileSpec::new("unknown.bin", 10).modified(None));
    fixture.file(downloads, FileSpec::new("unknown.zip", 10).modified(None));
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    assert!(report.items.is_empty(), "{report:#?}");
    assert_eq!(report.unknown_age_files, 2);
}

#[test]
fn temp_and_log_age_thresholds_can_be_changed() {
    let mut fixture = Fixture::new();
    let temp = fixture.dir(NodeId::ROOT, "Temp");
    fixture.file(temp, FileSpec::new("recent.tmp", 10).age_days(9));
    let logs = fixture.dir(NodeId::ROOT, "Logs");
    fixture.file(logs, FileSpec::new("old.log", 10).age_days(11));
    let (_temp, tree) = fixture.finish();

    let mut options = common::options();
    options.temp_min_age = Some(10 * DAY_SECS);
    options.logs_min_age = Some(10 * DAY_SECS);
    let report = analyse(&tree, _temp.path(), &options);
    assert!(report.items_in(Category::TempFiles).is_empty());
    assert_eq!(report.items_in(Category::LogFiles).len(), 1);

    options.temp_min_age = None;
    options.logs_min_age = None;
    let report = analyse(&tree, _temp.path(), &options);
    assert_eq!(report.items_in(Category::TempFiles).len(), 1);
    assert_eq!(report.items_in(Category::LogFiles).len(), 1);
}

#[test]
fn installer_and_vm_size_thresholds_are_inclusive() {
    let mut fixture = Fixture::new();
    let files = fixture.dir(NodeId::ROOT, "Files");
    fixture.file(files, FileSpec::new("small-setup.exe", 99));
    fixture.file(files, FileSpec::new("setup.exe", 100));
    fixture.file(files, FileSpec::new("small.vhdx", 99));
    fixture.file(files, FileSpec::new("disk.vhdx", 100));
    let (_temp, tree) = fixture.finish();
    let report = analyse(&tree, _temp.path(), &common::options());

    let installers = report.items_in(Category::LargeInstallers);
    assert_eq!(installers.len(), 1);
    assert!(installers[0].path.ends_with("setup.exe"));
    let images = report.items_in(Category::VmImages);
    assert_eq!(images.len(), 1);
    assert!(images[0].path.ends_with("disk.vhdx"));
}

#[test]
fn media_requires_both_file_count_and_media_byte_thresholds() {
    let mut fixture = Fixture::new();
    let media = fixture.dir(NodeId::ROOT, "Media");
    fixture.file(media, FileSpec::new("one.mp4", 100));
    fixture.file(media, FileSpec::new("two.mkv", 100));
    fixture.file(media, FileSpec::new("notes.txt", 1_000));
    let (_temp, tree) = fixture.finish();

    let mut options = common::options();
    options.media_min_size = 201;
    assert!(
        analyse(&tree, _temp.path(), &options)
            .items_in(Category::LargeMedia)
            .is_empty()
    );

    options.media_min_size = 200;
    let report = analyse(&tree, _temp.path(), &options);
    let collections = report.items_in(Category::LargeMedia);
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].logical_size, 1_200);

    options.media_min_files = 3;
    assert!(
        analyse(&tree, _temp.path(), &options)
            .items_in(Category::LargeMedia)
            .is_empty()
    );
}
