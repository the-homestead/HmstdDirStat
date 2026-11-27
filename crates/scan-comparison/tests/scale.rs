//! Scale check for [`compare`] on two million-entry scans.
//!
//! Ignored by default because it builds about one million entries per scan. Run with:
//!
//! `cargo test -p timberstat-scan-comparison --release --test scale -- --ignored --nocapture`

mod common;

use std::time::Instant;

use common::{directory, file, root_entry, scan};
use timberstat_scan_comparison::{ChangeCategory, compare};
use timberstat_scan_history::{Scan, ScanEntry};

const DIRECTORIES: u32 = 1_000;
const FILES_PER_DIRECTORY: u32 = 1_000;
const ROOT: &str = "C:/scale";

/// `size_of_first_file` lets the second scan differ from the first by one known amount.
fn build(timestamp: i64, size_of_first_file: u64) -> Scan {
    let capacity = 1 + DIRECTORIES as usize * (1 + FILES_PER_DIRECTORY as usize);
    let mut entries: Vec<ScanEntry> = Vec::with_capacity(capacity);
    entries.push(root_entry());
    for d in 0..DIRECTORIES {
        entries.push(directory(&format!("d{d}"), Some(0)));
    }
    for d in 0..DIRECTORIES {
        let parent = 1 + d;
        for f in 0..FILES_PER_DIRECTORY {
            let size = if d == 0 && f == 0 {
                size_of_first_file
            } else {
                10
            };
            entries.push(file(&format!("f{f}.bin"), Some(parent), size, 1));
        }
    }
    scan(ROOT, timestamp, entries)
}

#[test]
#[ignore = "builds two scans of about one million entries; run in release with --ignored"]
fn scale_compare_one_million_entries() {
    let started = Instant::now();
    let before = build(1, 10);
    let after = build(2, 25);
    let built = started.elapsed();
    assert_eq!(before.len(), 1 + 1_000 + 1_000_000);

    let compared = Instant::now();
    let result = compare(&before, &after).expect("compare two valid scans");
    let compared_in = compared.elapsed();

    // Only one file changed, by 15 bytes; the directory and root deltas must agree.
    assert_eq!(result.total.logical, 15);
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].category, ChangeCategory::Modified);

    println!(
        "scale: entries_per_scan={} build_both={:?} compare={:?} changes={} dirs={}",
        before.len(),
        built,
        compared_in,
        result.changes.len(),
        result.directories.len()
    );
}
