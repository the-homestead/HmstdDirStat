use std::{io, path::PathBuf};

use timberstat_filesystem::NodeId;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("export destination {0} already exists; pass overwrite = true to replace it")]
    AlreadyExists(PathBuf),
    #[error("could not {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("path cannot be represented as Unicode: {0:?}")]
    NonUnicodePath(PathBuf),
    #[error("tree node {0:?} is missing or detached")]
    MissingNode(NodeId),
    #[error("could not build the scan-history record: {0}")]
    ScanModel(#[source] io::Error),
    #[error("could not encode export JSON: {0}")]
    JsonEncode(#[source] serde_json::Error),
    #[error("could not parse export JSON: {0}")]
    JsonDecode(#[source] serde_json::Error),
    #[error("export schema version {found} is unsupported; this build supports {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("invalid exported scan: {0}")]
    InvalidScan(String),
}
