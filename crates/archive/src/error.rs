use std::path::PathBuf;

/// Result type for archive operations.
pub type ArchiveResult<T> = Result<T, ArchiveError>;

/// Failures encountered during archive work.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ArchiveError {
    /// The archive format is unsupported by any installed handler.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    /// No handler is available for this format (or any format in the message).
    #[error("no handler: {0}")]
    NoHandler(String),

    /// The configured preferred handler is not available on this host.
    #[error("the preferred handler {0} is not available on this host")]
    PreferredHandlerMissing(String),

    /// A handler invocation failed: `code` is None when the process could not
    /// be started; `stderr` carries the handler's diagnostic.
    #[error("handler {handler} failed (exit code {code:?}): {stderr}")]
    HandlerFailed {
        handler: String,
        code: Option<i32>,
        stderr: String,
    },

    /// The handler did not finish within the allowed time.
    #[error("handler {0} timed out")]
    Timeout(String),

    /// Cancellation was requested before the operation finished.
    #[error("cancelled")]
    Cancelled,

    /// The handler reported that the archive is password protected or the
    /// supplied password was wrong.
    #[error("the archive needs a password or the password is wrong: {0}")]
    PasswordRequired(String),

    /// Multi‑volume archives are not supported by the selected handler.
    #[error("multi‑volume archives are not supported: {0}")]
    MultiVolume(String),

    /// An entry inside the archive would escape the destination (path traversal).
    #[error("unsafe archive entry: {0}")]
    UnsafeEntry(String),

    /// Generic I/O error around a path.
    #[error("i/o error at {path}: {detail}")]
    Io { path: PathBuf, detail: String },

    /// No archive format matched the file name.
    #[error("unknown archive format for {0}")]
    UnknownFormat(PathBuf),

    /// The archive has no extractable entries.
    #[error("the archive has no entries")]
    EmptyArchive,

    /// The requested operation is not supported for this format/handler.
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

impl ArchiveError {
    pub fn io_error(path: impl Into<PathBuf>, error: std::io::Error) -> Self {
        ArchiveError::Io {
            path: path.into(),
            detail: error.to_string(),
        }
    }

    pub fn handler_failed(
        handler: impl Into<String>,
        code: Option<i32>,
        stderr: impl Into<String>,
    ) -> Self {
        ArchiveError::HandlerFailed {
            handler: handler.into(),
            code,
            stderr: stderr.into(),
        }
    }
}
