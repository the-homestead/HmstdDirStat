mod common;

use std::{ffi::OsStr, fs};

use common::{assert_code, fixture_root, run, scan_to_file, stderr, stdout_json};

#[test]
fn compare_reports_new_deleted_modified_and_size_deltas() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("keep.txt"), b"keep").unwrap();
    fs::write(root.join("change.txt"), b"a").unwrap();
    fs::write(root.join("delete.txt"), b"yyy").unwrap();
    let before = temp.path().join("before.json");
    let after = temp.path().join("after.json");
    scan_to_file(&root, &before);

    fs::write(root.join("change.txt"), b"abcdef").unwrap();
    fs::remove_file(root.join("delete.txt")).unwrap();
    fs::write(root.join("new.txt"), b"xx").unwrap();
    scan_to_file(&root, &after);

    let output = run([OsStr::new("compare"), before.as_os_str(), after.as_os_str()]);

    assert_code(&output, 0);
    let json = stdout_json(&output);
    // The root folder's own modified time also changes when entries are added or removed, and
    // the scanner records it to the second, so whether it counts as modified depends on timing.
    // The file entries are what this test checks, so they are asserted exactly.
    let summary = &json["summary"];
    assert_eq!(summary["new"], 1);
    assert_eq!(summary["deleted"], 1);
    assert!(summary["modified"].as_u64().unwrap() >= 1);
    assert_eq!(json["total"]["logical_delta"], "4");
    let changes = json["changes"].as_array().unwrap();
    let change = changes
        .iter()
        .find(|entry| entry["path"].as_str().unwrap().ends_with("change.txt"))
        .unwrap();
    assert_eq!(change["category"], "modified");
    assert_eq!(change["logical_delta"], "5");
    assert_eq!(change["reasons"]["logical_size"], true);
    let deleted = changes
        .iter()
        .find(|entry| entry["path"].as_str().unwrap().ends_with("delete.txt"))
        .unwrap();
    assert_eq!(deleted["category"], "deleted");
    assert_eq!(deleted["logical_delta"], "-3");
    let new = changes
        .iter()
        .find(|entry| entry["path"].as_str().unwrap().ends_with("new.txt"))
        .unwrap();
    assert_eq!(new["category"], "new");
    assert_eq!(new["logical_delta"], "2");
}

#[test]
fn compare_rejects_different_scan_roots_without_a_diff() {
    let temp = tempfile::tempdir().unwrap();
    let root_a = fixture_root(temp.path());
    let other = temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let before = temp.path().join("before.json");
    let after = temp.path().join("after.json");
    scan_to_file(&root_a, &before);
    scan_to_file(&other, &after);

    let output = run([OsStr::new("compare"), before.as_os_str(), after.as_os_str()]);

    assert_code(&output, 1);
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("expected"));
}

#[test]
fn all_three_builtin_profiles_run_against_the_supplied_tree() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("a.bin"), b"duplicate").unwrap();
    fs::write(root.join("b.bin"), b"duplicate").unwrap();

    let inventory = run([
        OsStr::new("profile"),
        OsStr::new("run"),
        OsStr::new("inventory"),
        root.as_os_str(),
    ]);
    assert_code(&inventory, 0);
    assert_eq!(
        stdout_json(&inventory)["entries"].as_array().unwrap().len(),
        3
    );

    let large = run([
        OsStr::new("profile"),
        OsStr::new("run"),
        OsStr::new("large-files"),
        root.as_os_str(),
    ]);
    assert_code(&large, 0);
    assert_eq!(stdout_json(&large)["top"], 20);
    assert_eq!(stdout_json(&large)["files"].as_array().unwrap().len(), 2);

    let duplicates = run([
        OsStr::new("profile"),
        OsStr::new("run"),
        OsStr::new("duplicates"),
        root.as_os_str(),
    ]);
    assert_code(&duplicates, 0);
    assert_eq!(
        stdout_json(&duplicates)["groups"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn user_supplied_profile_names_are_not_loaded() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    let output = run([
        OsStr::new("profile"),
        OsStr::new("run"),
        OsStr::new("custom-profile.json"),
        root.as_os_str(),
    ]);

    assert_code(&output, 2);
    assert!(stderr(&output).contains("unknown built-in profile"));
}
