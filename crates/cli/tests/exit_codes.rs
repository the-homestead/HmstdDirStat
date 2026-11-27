mod common;

use std::{ffi::OsStr, fs};

use common::{assert_code, fixture_root, run, stderr};

#[test]
fn usage_errors_exit_two_and_do_not_emit_data() {
    let cases: &[&[&OsStr]] = &[
        &[],
        &[OsStr::new("unknown")],
        &[OsStr::new("large"), OsStr::new(".")],
        &[
            OsStr::new("large"),
            OsStr::new("."),
            OsStr::new("--top"),
            OsStr::new("0"),
        ],
        &[
            OsStr::new("scan"),
            OsStr::new("."),
            OsStr::new("--format"),
            OsStr::new("xml"),
        ],
        &[
            OsStr::new("scan"),
            OsStr::new("."),
            OsStr::new("--overwrite"),
        ],
        &[OsStr::new("compare"), OsStr::new("one.json")],
        &[OsStr::new("profile"), OsStr::new("list")],
    ];
    for args in cases {
        let output = run(*args);
        assert_code(&output, 2);
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn operational_errors_exit_one() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let output = run([OsStr::new("hash"), missing.as_os_str()]);
    assert_code(&output, 1);
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("could not inspect"));
}

#[test]
fn manifest_enforces_shared_engines_and_no_gpui_dependency() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    for dependency in [
        "timberstat-scanner",
        "timberstat-filesystem",
        "timberstat-duplicates",
        "timberstat-export",
        "timberstat-scan-history",
        "timberstat-scan-comparison",
    ] {
        assert!(
            manifest.contains(&format!("{dependency} = {{ path =")),
            "missing shared dependency {dependency}"
        );
    }
    assert!(!manifest.contains("gpui"));
    assert!(!manifest.contains("clap"));
}

#[test]
fn help_and_version_are_successful_and_machine_data_stays_clean() {
    let help = run([OsStr::new("--help")]);
    assert_code(&help, 0);
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("profile run")
    );
    assert!(help.stderr.is_empty());

    let version = run([OsStr::new("--version")]);
    assert_code(&version, 0);
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .starts_with("timberstat-cli ")
    );
    assert!(version.stderr.is_empty());
}

#[test]
fn invalid_top_and_missing_paths_never_touch_real_user_data() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture_root(temp.path());
    let output = run([
        OsStr::new("large"),
        root.as_os_str(),
        OsStr::new("--top"),
        OsStr::new("not-a-number"),
    ]);
    assert_code(&output, 2);
    assert!(output.stdout.is_empty());
}
