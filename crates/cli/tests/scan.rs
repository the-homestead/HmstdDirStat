mod common;

use std::{ffi::OsStr, fs};

use common::{assert_code, fixture_root, run, stderr, stdout_json};

#[test]
fn scan_json_writes_data_to_stdout_and_progress_to_stderr() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("alpha.txt"), "alpha").unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/beta.txt"), "beta").unwrap();

    let output = run([OsStr::new("scan"), root.as_os_str()]);

    assert_code(&output, 0);
    let json = stdout_json(&output);
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["provenance"]["root"], root.to_str().unwrap());
    assert_eq!(json["entries"].as_array().unwrap().len(), 4);
    assert!(stderr(&output).contains("scan: scanning"));
    assert!(stderr(&output).contains("scan: visited 3 entries"));
}

#[test]
fn scan_csv_out_refuses_overwrite_then_replaces_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("alpha.txt"), "alpha").unwrap();
    let destination = temp.path().join("scan.csv");
    fs::write(&destination, "previous complete export").unwrap();

    let refused = run([
        OsStr::new("scan"),
        root.as_os_str(),
        OsStr::new("--format"),
        OsStr::new("csv"),
        OsStr::new("--out"),
        destination.as_os_str(),
    ]);
    assert_code(&refused, 1);
    assert!(stderr(&refused).contains("--overwrite"));
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "previous complete export"
    );

    let replaced = run([
        OsStr::new("scan"),
        root.as_os_str(),
        OsStr::new("--format"),
        OsStr::new("csv"),
        OsStr::new("--out"),
        destination.as_os_str(),
        OsStr::new("--overwrite"),
    ]);
    assert_code(&replaced, 0);
    assert!(replaced.stdout.is_empty());
    let csv = fs::read_to_string(&destination).unwrap();
    assert!(csv.starts_with(
        "path,name,kind,logical_size,allocated_size,modified,depth,hard_link_count\r\n"
    ));
    assert!(csv.contains("alpha.txt"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}

#[test]
fn scan_csv_supports_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("comma,name.txt"), "x").unwrap();

    let output = run([
        OsStr::new("scan"),
        root.as_os_str(),
        OsStr::new("--format"),
        OsStr::new("csv"),
    ]);

    assert_code(&output, 0);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = std::path::Path::new("root").join("comma,name.txt");
    assert!(stdout.contains(&format!("\"{}\"", expected.display())));
}

#[test]
fn failed_scan_never_creates_requested_output_file() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let destination = temp.path().join("scan.json");

    let output = run([
        OsStr::new("scan"),
        missing.as_os_str(),
        OsStr::new("--out"),
        destination.as_os_str(),
    ]);

    assert_code(&output, 1);
    assert!(!destination.exists());
    assert!(stderr(&output).contains("unavailable"));
}
