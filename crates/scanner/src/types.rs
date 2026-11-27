use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use std::ffi::OsString;
use timberstat_filesystem::FileTree;

/// Maximum issues kept in a report. Further issues are counted, not stored,
/// so a broken tree cannot exhaust memory.
pub const MAX_RECORDED_ISSUES: usize = 10_000;

/// What the scan does and what it was asked to skip.
/// Windows system folder the scan never enters. It holds volume metadata, not user files.
pub const SKIPPED_SYSTEM_FOLDER: &str = "System Volume Information";

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Paths to skip entirely, including everything beneath them.
    /// Matching is by whole path components and is case-sensitive.
    pub exclude: Vec<PathBuf>,
    /// Folder names skipped wherever they appear, ignoring ASCII case as NTFS does.
    pub skip_names: Vec<OsString>,
    /// Unix only: descend into directories on other filesystems. Off by default.
    pub cross_mount_points: bool,
    /// Directory-listing threads. `0` picks a default from the CPU count.
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            exclude: Vec::new(),
            skip_names: vec![OsString::from(SKIPPED_SYSTEM_FOLDER)],
            cross_mount_points: false,
            threads: 0,
        }
    }
}

impl ScanOptions {
    /// Number of listing threads to use for this scan.
    ///
    /// Directory listing dominates the cost on large volumes and each call blocks
    /// on the file system, so more threads than cores helps. The default is a
    /// starting point; measure before changing it.
    pub fn worker_count(&self) -> usize {
        if self.threads > 0 {
            return self.threads;
        }
        std::thread::available_parallelism()
            .map_or(8, |cores| cores.get() * 2)
            .clamp(4, 32)
    }
}

/// Cooperative cancellation. Clone freely; all clones share one flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

/// Live counters, reported every few thousand entries and once at the end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanProgress {
    pub entries_visited: u64,
    pub directories_read: u64,
}

/// Which technique produced a tree. Stored with the result so views can say how it was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMethod {
    /// Portable `std::fs` traversal.
    GenericTraversal,
    /// Read from the NTFS master file table (elevated, Windows).
    NtfsMft,
}

/// One problem the scan worked around without stopping.
#[derive(Debug)]
pub struct ScanIssue {
    pub path: PathBuf,
    pub message: String,
}

/// Everything a scan reports besides the tree itself.
#[derive(Debug)]
pub struct ScanReport {
    pub method: ScanMethod,
    pub entries_visited: u64,
    pub issues: Vec<ScanIssue>,
    /// Issues beyond [`MAX_RECORDED_ISSUES`], counted but not stored.
    pub dropped_issues: u64,
    /// Later hard links whose physical charge went to an earlier path.
    pub hardlink_duplicates: u64,
    /// Directories on other filesystems that were not entered.
    pub mount_points_skipped: u64,
}

impl ScanReport {
    pub fn new(method: ScanMethod) -> Self {
        Self {
            method,
            entries_visited: 0,
            issues: Vec::new(),
            dropped_issues: 0,
            hardlink_duplicates: 0,
            mount_points_skipped: 0,
        }
    }

    pub(crate) fn push_issue(&mut self, path: &Path, source: io::Error) {
        if self.issues.len() < MAX_RECORDED_ISSUES {
            self.issues.push(ScanIssue {
                path: path.to_path_buf(),
                message: source.to_string(),
            });
        } else {
            self.dropped_issues += 1;
        }
    }

    /// Total number of issues, stored and dropped.
    pub fn issue_count(&self) -> u64 {
        self.issues.len() as u64 + self.dropped_issues
    }
}

/// A completed scan: the tree plus its report.
#[derive(Debug)]
pub struct ScanOutput {
    pub tree: FileTree,
    pub report: ScanReport,
}

/// Why a scan produced no tree.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// Cancellation was requested. No partial tree is returned.
    #[error("scan was cancelled; no result was produced")]
    Cancelled,
    #[error("scan root {path} is unavailable: {source}")]
    RootUnavailable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("scan root {0} is not a directory")]
    RootNotDirectory(PathBuf),
    #[error("internal tree error: {0}")]
    Tree(#[from] timberstat_filesystem::TreeError),
    /// The listing workers exited before the walk finished. The result is unusable.
    #[error("directory workers stopped before the walk finished")]
    WorkersStopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_token_clones_share_state() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn issues_past_the_cap_are_counted_not_stored() {
        let mut report = ScanReport::new(ScanMethod::GenericTraversal);
        for _ in 0..(MAX_RECORDED_ISSUES + 5) {
            report.push_issue(Path::new("x"), io::Error::other("boom"));
        }
        assert_eq!(report.issues.len(), MAX_RECORDED_ISSUES);
        assert_eq!(report.dropped_issues, 5);
        assert_eq!(report.issue_count(), (MAX_RECORDED_ISSUES + 5) as u64);
    }
}
