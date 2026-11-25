//! Text shown under the volume buttons for each scan phase.

use std::sync::atomic::Ordering;

use crate::{
    scan::{ScanSummary, format_bytes, format_count},
    shell::Phase,
};

/// The text lines shown under the volume buttons for the current phase.
pub fn status_lines(phase: &Phase) -> Vec<String> {
    match phase {
        Phase::Idle => vec!["Choose a volume to scan.".to_string()],
        Phase::Scanning { root, entries, .. } => vec![format!(
            "Scanning {} … {} entries so far",
            root.display(),
            format_count(entries.load(Ordering::Relaxed))
        )],
        Phase::Cancelled(root) => vec![format!(
            "Scan of {} was cancelled. No partial result is shown.",
            root.display()
        )],
        Phase::Failed { root, message } => {
            vec![format!("Scan of {} failed: {message}", root.display())]
        }
        Phase::Finished(summary) => summary_lines(summary),
    }
}

fn summary_lines(summary: &ScanSummary) -> Vec<String> {
    let physical = match summary.physical_bytes {
        Some(bytes) => format!("Physical size: {}", format_bytes(bytes)),
        None => "Physical size: not available on this platform yet".to_string(),
    };
    let completeness = if summary.complete {
        "Totals: complete".to_string()
    } else {
        "Totals: PARTIAL, some folders could not be read".to_string()
    };
    let mut lines = vec![
        format!("Root: {}", summary.root.display()),
        format!(
            "Files: {}   Folders: {}",
            format_count(summary.files),
            format_count(summary.directories)
        ),
        format!("Logical size: {}", format_bytes(summary.logical_bytes)),
        physical,
        completeness,
        format!("Issues: {}", format_count(summary.issues)),
    ];
    // Name the unreadable paths so skipped folders are visible, not just counted.
    lines.extend(summary.issue_paths.iter().map(|issue| format!("  {issue}")));
    lines.extend([
        format!(
            "Hard links counted once: {}",
            format_count(summary.hardlink_duplicates)
        ),
        format!(
            "Scan time: {:.2} s via {}",
            summary.elapsed.as_secs_f64(),
            summary.method
        ),
    ]);
    lines
}
