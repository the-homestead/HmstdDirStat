use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use timberstat_core::config;
use timberstat_engine::{self as shared, EngineError, ScanKind, ScanModeChoice, fast::FastPaths};
use timberstat_filesystem::FileTree;
use timberstat_scan_history::{ScanMethod as HistoryMethod, ScanProvenance};
use timberstat_scanner::{ScanMethod, ScanReport};

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

/// Scans `root` the way the GUI would: the drive's saved mode decides between the file table and
/// a folder walk, through the shared engine.
pub fn scan(root: &Path, cancel: &Cancellation) -> CliResult<Scanned> {
    cancel.check()?;
    let kind = shared::plan(root, drive_choice(root), None);
    eprintln!("scan: scanning {} ({})", root.display(), kind.label());
    let output = match kind {
        ScanKind::Walk => shared::scan_walk(root, cancel.scan_token(), &mut |progress| {
            eprintln!(
                "scan: {} entries, {} directories",
                progress.entries_visited, progress.directories_read
            );
        }),
        ScanKind::Fast => shared::scan_mft(root, &FastPaths::for_this_run()),
    }
    .map_err(|error| match error {
        EngineError::Cancelled => CliError::cancelled(),
        EngineError::Failed(message) => CliError::failure(message),
    })?;
    cancel.check()?;
    let method = match output.report.method {
        ScanMethod::GenericTraversal => HistoryMethod::GenericTraversal,
        ScanMethod::NtfsMft => HistoryMethod::NtfsMft,
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

/// The mode the user saved for this drive in config.json. Unreadable settings fall back to Auto.
fn drive_choice(root: &Path) -> ScanModeChoice {
    let Some(letter) = shared::drive_letter(root) else {
        return ScanModeChoice::Auto;
    };
    match config::default_config_path().and_then(|path| config::load_from(&path)) {
        Ok(settings) => shared::choice_for(&settings.drive_modes, letter),
        Err(error) => {
            eprintln!("scan: settings unreadable, using Auto: {error}");
            ScanModeChoice::Auto
        }
    }
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
