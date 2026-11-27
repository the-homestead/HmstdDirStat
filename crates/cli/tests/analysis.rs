mod common;

use std::{ffi::OsStr, fs};

use common::{assert_code, fixture_root, run, stderr, stdout_json};

#[test]
fn large_returns_only_the_requested_largest_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("small.bin"), vec![0_u8; 10]).unwrap();
    fs::write(root.join("large.bin"), vec![0_u8; 300]).unwrap();
    fs::write(root.join("medium.bin"), vec![0_u8; 200]).unwrap();

    let output = run([
        OsStr::new("large"),
        root.as_os_str(),
        OsStr::new("--top"),
        OsStr::new("2"),
    ]);

    assert_code(&output, 0);
    let json = stdout_json(&output);
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["logical_size"], 300);
    assert!(files[0]["path"].as_str().unwrap().ends_with("large.bin"));
    assert_eq!(files[1]["logical_size"], 200);
    assert!(files[1]["path"].as_str().unwrap().ends_with("medium.bin"));
    assert_eq!(json["complete"], true);
}

#[test]
fn hash_uses_the_official_blake3_empty_input_vector() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("empty.bin");
    fs::write(&file, []).unwrap();

    let output = run([OsStr::new("hash"), file.as_os_str()]);

    assert_code(&output, 0);
    let json = stdout_json(&output);
    assert_eq!(json["algorithm"], "blake3");
    assert_eq!(json["bytes"], 0);
    assert_eq!(
        json["digest"],
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
}

#[test]
fn hash_reports_every_other_algorithm_as_not_done() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("data.bin");
    fs::write(&file, b"data").unwrap();

    for algorithm in ["sha256", "sha512", "md5", "sha1", "crc32", "xxhash"] {
        let output = run([
            OsStr::new("hash"),
            file.as_os_str(),
            OsStr::new("--algorithm"),
            OsStr::new(algorithm),
        ]);
        assert_code(&output, 2);
        assert!(stderr(&output).contains("not implemented"));
        assert!(stderr(&output).contains("only BLAKE3 is done"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn dupes_exports_byte_verified_groups() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("first.bin"), b"same bytes").unwrap();
    fs::write(root.join("nested/second.bin"), b"same bytes").unwrap();
    fs::write(root.join("different.bin"), b"not same!").unwrap();

    let output = run([OsStr::new("dupes"), root.as_os_str()]);

    assert_code(&output, 0);
    let json = stdout_json(&output);
    let groups = json["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["size"], 10);
    assert_eq!(groups[0]["reclaimable_size"], 10);
    assert_eq!(groups[0]["members"].as_array().unwrap().len(), 2);
    assert_eq!(groups[0]["digest"].as_str().unwrap().len(), 64);
}

#[test]
fn dupes_can_export_csv_to_a_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    fs::write(root.join("a"), b"duplicate").unwrap();
    fs::write(root.join("b"), b"duplicate").unwrap();
    let destination = temp.path().join("dupes.csv");

    let output = run([
        OsStr::new("dupes"),
        root.as_os_str(),
        OsStr::new("--format"),
        OsStr::new("csv"),
        OsStr::new("--out"),
        destination.as_os_str(),
    ]);

    assert_code(&output, 0);
    assert!(output.stdout.is_empty());
    let csv = fs::read_to_string(destination).unwrap();
    assert!(csv.contains("group_id,digest\r\n"));
    assert_eq!(csv.lines().count(), 3);
}
