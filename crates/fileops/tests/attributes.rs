//! Attribute changes: read-only and hidden, read back from the disk.
//! Everything runs under a temp folder. No user data is touched.

use std::fs;
use std::path::Path;

use timberstat_fileops::{AttributeChange, FailureReason, read_attributes, set_attributes};

fn file_with(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"content").unwrap();
    path
}

/// Clears the read-only bit so the temp folder can be deleted on Windows.
fn unlock(path: &Path) {
    let mut perms = fs::symlink_metadata(path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(path, perms).unwrap();
}

#[test]
fn readonly_is_set_and_cleared_and_read_back() {
    let tmp = tempfile::tempdir().unwrap();
    let file = file_with(tmp.path(), "a.txt");

    let set = set_attributes(
        &file,
        AttributeChange {
            readonly: Some(true),
            hidden: None,
        },
    )
    .expect("readonly on");
    assert!(set.readonly);
    assert!(read_attributes(&file).unwrap().readonly);

    let cleared = set_attributes(
        &file,
        AttributeChange {
            readonly: Some(false),
            hidden: None,
        },
    )
    .expect("readonly off");
    assert!(!cleared.readonly);
    assert!(!read_attributes(&file).unwrap().readonly);
}

#[test]
fn readonly_works_on_a_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("folder");
    fs::create_dir(&dir).unwrap();

    let set = set_attributes(
        &dir,
        AttributeChange {
            readonly: Some(true),
            hidden: None,
        },
    )
    .expect("readonly on folder");
    assert!(set.readonly);
    unlock(&dir);
}

#[test]
fn empty_change_only_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let file = file_with(tmp.path(), "a.txt");
    let before = read_attributes(&file).unwrap();
    let after = set_attributes(&file, AttributeChange::default()).unwrap();
    assert_eq!(before, after);
}

#[test]
fn relative_path_is_refused() {
    let err = set_attributes(
        Path::new("relative.txt"),
        AttributeChange {
            readonly: Some(true),
            hidden: None,
        },
    )
    .unwrap_err();
    assert_eq!(err.reason, FailureReason::Other);
}

#[test]
fn missing_entry_reports_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let err = set_attributes(
        &tmp.path().join("nope.txt"),
        AttributeChange {
            readonly: Some(true),
            hidden: None,
        },
    )
    .unwrap_err();
    assert_eq!(err.reason, FailureReason::NotFound);
}

#[cfg(not(windows))]
#[test]
fn hidden_is_refused_off_windows() {
    let tmp = tempfile::tempdir().unwrap();
    let file = file_with(tmp.path(), "a.txt");
    let err = set_attributes(
        &file,
        AttributeChange {
            readonly: None,
            hidden: Some(true),
        },
    )
    .unwrap_err();
    assert!(err.detail.contains("only on Windows"));
    assert!(!read_attributes(&file).unwrap().hidden);
}

#[cfg(windows)]
#[test]
fn hidden_is_set_and_cleared_on_windows() {
    let tmp = tempfile::tempdir().unwrap();
    let file = file_with(tmp.path(), "a.txt");
    let on = set_attributes(
        &file,
        AttributeChange {
            readonly: None,
            hidden: Some(true),
        },
    )
    .expect("hidden on");
    assert!(on.hidden);
    assert!(read_attributes(&file).unwrap().hidden);
    let off = set_attributes(
        &file,
        AttributeChange {
            readonly: None,
            hidden: Some(false),
        },
    )
    .expect("hidden off");
    assert!(!off.hidden);
    assert!(!read_attributes(&file).unwrap().hidden);
}

#[cfg(unix)]
#[test]
fn links_are_refused_and_their_target_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let target = file_with(tmp.path(), "target.txt");
    let link = tmp.path().join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let err = set_attributes(
        &link,
        AttributeChange {
            readonly: Some(true),
            hidden: None,
        },
    )
    .unwrap_err();
    assert_eq!(err.reason, FailureReason::Other);
    assert!(!read_attributes(&target).unwrap().readonly);
}
