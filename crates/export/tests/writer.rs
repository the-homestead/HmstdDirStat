use std::fs;

use timberstat_export::{ExportError, write_atomic};

#[test]
fn existing_file_is_refused_without_explicit_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scan.csv");
    fs::write(&path, "old complete export").unwrap();

    let error = write_atomic(&path, "replacement", false).unwrap_err();
    assert!(matches!(error, ExportError::AlreadyExists(_)));
    assert_eq!(fs::read_to_string(&path).unwrap(), "old complete export");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn atomic_write_creates_then_replaces_complete_file_without_temp_leftovers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scan.json");
    let first = "{\"schema_version\":1,\"value\":\"first\"}\n";
    let second = "{\"schema_version\":1,\"value\":\"second\"}\n";

    write_atomic(&path, first, false).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), first);
    write_atomic(&path, second, true).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), second);
    let entries: Vec<_> = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, [path.file_name().unwrap()]);
}

#[test]
fn import_file_reads_the_same_public_json_contract() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.json");
    let error = timberstat_export::import_scan_json_file(&path).unwrap_err();
    assert!(matches!(error, ExportError::Io { .. }));
}
