#![allow(dead_code)]

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;

pub fn run<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_timberstat-cli"))
        .args(args)
        .output()
        .expect("CLI process must launch")
}

pub fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

pub fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not JSON: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn assert_code(output: &Output, expected: i32) {
    assert_eq!(
        code(output),
        expected,
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr(output)
    );
}

pub fn fixture_root(parent: &Path) -> PathBuf {
    let root = parent.join("root");
    std::fs::create_dir(&root).unwrap();
    root
}

pub fn scan_to_file(root: &Path, output: &Path) {
    let result = run([
        OsStr::new("scan"),
        root.as_os_str(),
        OsStr::new("--out"),
        output.as_os_str(),
    ]);
    assert_code(&result, 0);
    assert!(result.stdout.is_empty());
}
