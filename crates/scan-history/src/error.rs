/// Errors that can occur when working with persisted scans.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not {operation} scan file {path}: {source}")]
    Io {
        operation: &'static str,
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("scan file {path} is corrupt: {reason}")]
    Corrupt {
        path: std::path::PathBuf,
        reason: String,
    },
    #[error("scan file {path} uses schema version {found}; this build supports {supported}")]
    UnsupportedVersion {
        path: std::path::PathBuf,
        found: u32,
        supported: u32,
    },
    #[error("scan file {path} is invalid: {reason}")]
    Invalid {
        path: std::path::PathBuf,
        reason: String,
    },
    #[error("could not serialize scan for {path}: {reason}")]
    Encode {
        path: std::path::PathBuf,
        reason: String,
    },
    #[error(
        "scan file {path} uses unsupported schema version {found}; supported versions are 1..={supported}"
    )]
    UnsupportedMigration {
        path: std::path::PathBuf,
        found: u32,
        supported: u32,
    },
}
