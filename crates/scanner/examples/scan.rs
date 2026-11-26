//! Headless scan of one directory: `cargo run -p timberstat-scanner --example scan -- <path>`.
//!
//! Development aid for Phase 2. The CLI (Phase 7) will supersede it.

use std::{path::PathBuf, time::Instant};

use timberstat_filesystem::NodeId;
use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

fn main() {
    let root: PathBuf = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let started = Instant::now();
    let out = GenericScanner
        .scan(
            &root,
            &ScanOptions::default(),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap_or_else(|error| {
            eprintln!("scan failed: {error}");
            std::process::exit(1);
        });
    let elapsed = started.elapsed();

    let totals = out.tree.node(NodeId::ROOT).expect("root exists").totals();
    println!("root:        {}", root.display());
    println!("method:      {:?}", out.report.method);
    println!("entries:     {}", out.report.entries_visited);
    println!("files:       {}", totals.files);
    println!("directories: {}", totals.directories);
    println!("logical:     {} bytes", totals.logical);
    if totals.physical_known {
        println!("physical:    {} bytes", totals.physical);
    } else {
        println!("physical:    unknown on this platform (partial)");
    }
    println!("complete:    {}", totals.complete);
    println!(
        "hard links:  {} deduplicated",
        out.report.hardlink_duplicates
    );
    println!("skipped mounts: {}", out.report.mount_points_skipped);
    println!("issues:      {}", out.report.issue_count());
    for issue in out.report.issues.iter().take(20) {
        println!("  - {}: {}", issue.path.display(), issue.message);
    }
    println!("elapsed:     {elapsed:?}");
}
