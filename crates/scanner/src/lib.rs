//! Filesystem scanning for TimberStat (Plan §4, Phase 2).
//!
//! Scanning reads metadata only, never file contents. It never follows symlinks
//! or junctions. Hard links are charged physically once per file identity.
//! Cancellation returns an error and no tree, so an interrupted scan can never
//! be mistaken for a complete one.

mod generic;
mod types;
mod volumes;
mod walk;

pub use generic::GenericScanner;
pub use types::{
    CancelToken, ScanError, ScanIssue, ScanMethod, ScanOptions, ScanOutput, ScanProgress,
    ScanProvenance, ScanReport,
};
pub use volumes::{SystemVolumes, VolumeEnumerator};

use std::path::Path;

/// A way of producing a [`FileTree`](timberstat_filesystem::FileTree) from a directory.
///
/// Phase 2 provides [`GenericScanner`]. The NTFS fast path (Phase 3) implements
/// the same trait and reports its own [`ScanMethod`].
pub trait FileSystemScanner {
    fn method(&self) -> ScanMethod;

    /// Scans `root`. `progress` is called periodically from the scanning thread.
    fn scan(
        &self,
        root: &Path,
        options: &ScanOptions,
        cancel: &CancelToken,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<ScanOutput, ScanError>;
}

pub use walk::is_skipped_name;
