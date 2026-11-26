//! Shared types for file operations: work items, previews, cancellation,
//! and classified failures. Nothing here touches the filesystem.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use timberstat_filesystem::{EntryKind, NodeId};

/// One tree entry an operation will act on, copied out of the tree so the
/// background worker never needs the tree itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub path: PathBuf,
    pub kind: EntryKind,
    pub logical: u64,
    pub physical: Option<u64>,
    /// Identity used to count hard links once. `None` when the platform gives none.
    pub identity: Option<timberstat_filesystem::FileIdentity>,
}

/// Shared progress and cancel flag. The UI keeps one `Arc<Control>` and hands
/// a clone to the worker; the worker checks `is_cancelled` before every entry.
#[derive(Debug, Default)]
pub struct Control {
    cancelled: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
}

impl Control {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Requests cancellation. Work already finished stays finished; nothing
    /// is rolled back.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::SeqCst);
    }

    pub fn advance(&self, by: u64) {
        self.done.fetch_add(by, Ordering::SeqCst);
    }

    pub fn done(&self) -> u64 {
        self.done.load(Ordering::SeqCst)
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::SeqCst)
    }
}

/// Why one path could not be processed. Kept coarse so the UI can name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    /// Another process holds the file open (sharing or lock violation).
    Locked,
    /// Permission denied, including read-only files on Windows.
    AccessDenied,
    /// The path is gone or was never there.
    NotFound,
    /// A directory still holds entries that were not removed.
    NotEmpty,
    /// A name clash the active collision policy will not resolve.
    Conflict,
    /// A child of this entry failed, so the entry was kept.
    Blocked,
    /// The Recycle Bin rejected the item, e.g. no bin on this volume.
    RecycleBin,
    /// The new name is not a valid file name.
    InvalidName,
    /// Anything else. The detail carries the OS or library message.
    Other,
}

impl FailureReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::AccessDenied => "access denied",
            Self::NotFound => "not found",
            Self::NotEmpty => "directory not empty",
            Self::Conflict => "name conflict",
            Self::Blocked => "kept: a child failed",
            Self::RecycleBin => "Recycle Bin rejected the item",
            Self::InvalidName => "invalid name",
            Self::Other => "failed",
        }
    }
}

/// One path that was not processed, with the reason and the raw detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub path: PathBuf,
    pub reason: FailureReason,
    pub detail: String,
}

impl Failure {
    pub fn new(path: impl Into<PathBuf>, reason: FailureReason, detail: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason,
            detail: detail.into(),
        }
    }

    pub fn from_io(path: impl Into<PathBuf>, error: &io::Error) -> Self {
        Self::new(path, classify_io(error), error.to_string())
    }
}

/// Maps an I/O error to a reason. Windows error codes are checked first
/// because `ErrorKind` does not separate sharing violations from access denial.
pub fn classify_io(error: &io::Error) -> FailureReason {
    #[cfg(windows)]
    {
        match error.raw_os_error() {
            // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
            Some(32 | 33) => return FailureReason::Locked,
            // ERROR_ACCESS_DENIED
            Some(5) => return FailureReason::AccessDenied,
            // ERROR_DIR_NOT_EMPTY
            Some(145) => return FailureReason::NotEmpty,
            _ => {}
        }
    }
    #[cfg(unix)]
    {
        match error.raw_os_error() {
            // EBUSY
            Some(16) => return FailureReason::Locked,
            // EACCES, EPERM
            Some(13 | 1) => return FailureReason::AccessDenied,
            // ENOTEMPTY
            Some(39) => return FailureReason::NotEmpty,
            _ => {}
        }
    }
    match error.kind() {
        io::ErrorKind::NotFound => FailureReason::NotFound,
        io::ErrorKind::PermissionDenied => FailureReason::AccessDenied,
        io::ErrorKind::ResourceBusy => FailureReason::Locked,
        io::ErrorKind::DirectoryNotEmpty => FailureReason::NotEmpty,
        _ => FailureReason::Other,
    }
}

/// Totals shown in the preview, computed from the tree before anything runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preview {
    pub files: u64,
    pub directories: u64,
    pub links_and_other: u64,
    /// Sum of apparent sizes. Every path counts its own size.
    pub logical: u64,
    /// Sum of on-disk sizes over the selection, each file identity once.
    pub physical: u64,
    /// False when any entry in the selection has no on-disk size.
    pub physical_known: bool,
    /// On-disk bytes this operation would actually free: `physical` minus
    /// hard-linked files that also have a link outside the selection.
    pub reclaimable: u64,
    /// The first paths in sorted order, at most the preview cap.
    pub paths_shown: Vec<PathBuf>,
    /// Paths omitted from `paths_shown`.
    pub paths_hidden: usize,
}

impl Preview {
    pub fn entry_count(&self) -> u64 {
        self.files + self.directories + self.links_and_other
    }
}

/// Why a plan could not be built. These are refusals, not partial results.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("nothing is selected")]
    NothingSelected,
    #[error("node {0:?} is not part of the tree any more")]
    Detached(NodeId),
    #[error("the scan root cannot be removed or moved from here")]
    ScanRoot,
    #[error("destination {dest} is inside the source {source_path}")]
    IntoItself { source_path: PathBuf, dest: PathBuf },
    #[error("destination {0} is not an existing directory")]
    DestinationNotDirectory(PathBuf),
    #[error("cannot resolve {0} on disk: {1}")]
    Resolve(PathBuf, String),
    #[error("name {0:?} is not allowed: {1}")]
    BadName(String, &'static str),
    #[error("the typed confirmation does not match; nothing was deleted")]
    ConfirmationMismatch,
}
