//! GUI-free text and gating for the delete dialog. Kept separate so the safety wording
//! and permanent-delete gate can be tested without creating a window.

use hmstd_fileops::{DeleteReport, Failure, Preview};

use crate::scan::{format_bytes, format_count};

/// Values shown in the preview's dense statistics sheet.
pub(super) struct PreviewText {
    pub files: String,
    pub directories: String,
    pub other: String,
    pub logical: String,
    pub physical: String,
    pub reclaimable: String,
}

pub(super) fn preview_text(preview: &Preview) -> PreviewText {
    let physical = if preview.physical_known {
        format_bytes(preview.physical)
    } else {
        format!("At least {} (incomplete)", format_bytes(preview.physical))
    };
    let reclaimable = if preview.physical_known {
        format_bytes(preview.reclaimable)
    } else {
        format!(
            "At least {} (incomplete)",
            format_bytes(preview.reclaimable)
        )
    };
    PreviewText {
        files: format_count(preview.files),
        directories: format_count(preview.directories),
        other: format_count(preview.links_and_other),
        logical: format_bytes(preview.logical),
        physical,
        reclaimable,
    }
}

/// Exact permanent-delete gate, matching `DeletePlan::confirm_permanent`.
pub(super) fn confirmation_matches(typed: &str, phrase: &str) -> bool {
    typed.trim() == phrase.trim()
}

pub(super) fn hidden_paths_text(hidden: usize) -> Option<String> {
    (hidden > 0).then(|| format!("{} more not shown", format_count(hidden as u64)))
}

/// A fraction for a determinate bar. Zero total means the total is not known yet.
pub(super) fn progress_fraction(done: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| (done as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

pub(super) fn progress_text(done: u64, total: u64) -> String {
    if total == 0 {
        format!("{} entries processed", format_count(done))
    } else {
        format!(
            "{} of {} entries processed",
            format_count(done),
            format_count(total)
        )
    }
}

/// The engine owns summary wording. In particular, a partial report already starts with
/// `Partial`; this function never turns it into a success message.
pub(super) fn result_headline(report: &DeleteReport) -> String {
    report.summary()
}

pub(super) fn failure_text(failure: &Failure) -> String {
    format!("{} — {}", failure.reason.label(), failure.detail)
}

#[cfg(test)]
mod tests {
    use hmstd_fileops::{DeleteMode, FailureReason};
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn permanent_confirmation_is_exact_after_trimming() {
        assert!(confirmation_matches(" folder ", "folder"));
        assert!(confirmation_matches("folder", " folder "));
        assert!(!confirmation_matches("other", "folder"));
        assert!(!confirmation_matches("", "folder"));
        assert!(!confirmation_matches("folder2", "folder"));
    }

    #[test]
    fn hidden_paths_are_explicit() {
        assert_eq!(hidden_paths_text(0), None);
        assert_eq!(hidden_paths_text(1).as_deref(), Some("1 more not shown"));
        assert_eq!(
            hidden_paths_text(1_234).as_deref(),
            Some("1,234 more not shown")
        );
    }

    #[test]
    fn unknown_physical_sizes_are_not_presented_as_complete() {
        let preview = Preview {
            physical: 4096,
            reclaimable: 2048,
            physical_known: false,
            ..Preview::default()
        };
        let text = preview_text(&preview);
        assert_eq!(text.physical, "At least 4.0 KiB (incomplete)");
        assert_eq!(text.reclaimable, "At least 2.0 KiB (incomplete)");
    }

    #[test]
    fn progress_is_determinate_only_with_a_total() {
        assert_eq!(progress_fraction(2, 4), Some(0.5));
        assert_eq!(progress_fraction(8, 4), Some(1.0));
        assert_eq!(progress_fraction(0, 0), None);
        assert_eq!(progress_text(2, 4), "2 of 4 entries processed");
        assert_eq!(progress_text(2, 0), "2 entries processed");
    }

    #[test]
    fn partial_result_headline_never_claims_success() {
        let report = DeleteReport {
            mode: Some(DeleteMode::Permanent),
            total_entries: 3,
            removed: vec![hmstd_filesystem::NodeId::ROOT],
            failures: vec![Failure::new(
                PathBuf::from("locked.tmp"),
                FailureReason::Locked,
                "sharing violation",
            )],
            not_attempted: 1,
            cancelled: true,
            ..DeleteReport::default()
        };
        let headline = result_headline(&report);
        assert_eq!(headline, report.summary());
        assert!(headline.starts_with("Partial:"));
        assert!(!headline.contains("Deleted 3 of 3"));
        assert_eq!(
            failure_text(&report.failures[0]),
            "locked — sharing violation"
        );
    }
}
