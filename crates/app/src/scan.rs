//! Scan results as plain data, and display formatting. No GPUI types here,
//! so the logic is testable without a window.

use std::{path::PathBuf, time::Duration};

use hmstd_filesystem::NodeId;
use hmstd_scanner::{ScanMethod, ScanOutput};

/// What the user sees after a scan completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    pub root: PathBuf,
    pub files: u64,
    pub directories: u64,
    pub logical_bytes: u64,
    /// `None` when this platform cannot report physical sizes.
    pub physical_bytes: Option<u64>,
    /// False when any directory could not be fully read; totals are then partial.
    pub complete: bool,
    pub issues: u64,
    pub hardlink_duplicates: u64,
    pub entries: u64,
    pub elapsed: Duration,
    pub method: &'static str,
}

/// Reduces a finished scan to what the window displays.
pub fn summarize(root: PathBuf, output: &ScanOutput, elapsed: Duration) -> ScanSummary {
    let totals = output
        .tree
        .node(NodeId::ROOT)
        .map(|node| node.totals())
        .unwrap_or_default();
    ScanSummary {
        root,
        files: totals.files,
        directories: totals.directories,
        logical_bytes: totals.logical,
        physical_bytes: totals.physical_known.then_some(totals.physical),
        complete: totals.complete,
        issues: output.report.issue_count(),
        hardlink_duplicates: output.report.hardlink_duplicates,
        entries: output.report.entries_visited,
        elapsed,
        method: match output.report.method {
            ScanMethod::GenericTraversal => "generic traversal",
        },
    }
}

/// Binary (IEC) units with one decimal place above bytes, e.g. `1.5 GiB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Decimal count with comma separators, e.g. `1,234,567`.
pub fn format_count(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmstd_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(1 << 20), "1.0 MiB");
        assert_eq!(format_bytes(3 << 30), "3.0 GiB");
        assert_eq!(format_bytes(2 << 40), "2.0 TiB");
    }

    #[test]
    fn counts_get_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }

    #[test]
    fn summary_matches_the_scanned_tree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 300]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b.bin"), vec![0u8; 700]).unwrap();
        let output = GenericScanner
            .scan(
                dir.path(),
                &ScanOptions::default(),
                &CancelToken::new(),
                &mut |_| {},
            )
            .unwrap();

        let summary = summarize(dir.path().to_path_buf(), &output, Duration::from_millis(5));
        assert_eq!(summary.logical_bytes, 1000);
        assert_eq!(summary.files, 2);
        assert_eq!(summary.directories, 2);
        assert!(summary.complete);
        assert_eq!(summary.issues, 0);
        assert_eq!(summary.method, "generic traversal");
        assert_eq!(summary.entries, 3);
    }
}
