use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use timberstat_filesystem::FileTree;
use timberstat_scan_history::{ScanMethod as HistoryMethod, ScanProvenance};
use timberstat_scanner::{
    FileSystemScanner, GenericScanner, ScanError, ScanMethod, ScanOptions, ScanReport,
};

use crate::{
    cancel::Cancellation,
    error::{CliError, CliResult, EXIT_PARTIAL, EXIT_SUCCESS},
};

pub struct Scanned {
    pub tree: FileTree,
    pub report: ScanReport,
    pub provenance: ScanProvenance,
    pub root: PathBuf,
}

impl Scanned {
    pub fn is_partial(&self) -> bool {
        self.report.issue_count() > 0
            || self
                .tree
                .node(self.tree.root())
                .is_none_or(|root| !root.totals().complete)
    }

    pub fn exit_code(&self) -> u8 {
        if self.is_partial() {
            EXIT_PARTIAL
        } else {
            EXIT_SUCCESS
        }
    }
}

pub fn scan(root: &Path, cancel: &Cancellation) -> CliResult<Scanned> {
    cancel.check()?;
    eprintln!("scan: scanning {}", root.display());
    let output = GenericScanner
        .scan(
            root,
            &ScanOptions::default(),
            cancel.scan_token(),
            &mut |progress| {
                eprintln!(
                    "scan: {} entries, {} directories",
                    progress.entries_visited, progress.directories_read
                );
            },
        )
        .map_err(|error| match error {
            ScanError::Cancelled => CliError::cancelled(),
            other => CliError::failure(other.to_string()),
        })?;
    cancel.check()?;
    let method = match output.report.method {
        ScanMethod::GenericTraversal => HistoryMethod::GenericTraversal,
    };
    let scanned = Scanned {
        tree: output.tree,
        report: output.report,
        provenance: ScanProvenance {
            method,
            timestamp_unix_secs: unix_timestamp(),
            root: root.to_path_buf(),
        },
        root: root.to_path_buf(),
    };
    eprintln!(
        "scan: visited {} entries; {} issues",
        scanned.report.entries_visited,
        scanned.report.issue_count()
    );
    report_issues(&scanned);
    Ok(scanned)
}

fn report_issues(scanned: &Scanned) {
    if !scanned.is_partial() {
        return;
    }
    eprintln!("scan: result is partial; exit code will be {EXIT_PARTIAL}");
    for issue in scanned.report.issues.iter().take(5) {
        eprintln!("scan: issue {}: {}", issue.path.display(), issue.message);
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0)
}
