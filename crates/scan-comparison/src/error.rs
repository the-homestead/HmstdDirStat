/// A comparison was refused rather than returning a partial diff.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComparisonError {
    #[error("growth trend needs at least {minimum} snapshots; received {actual}")]
    TooFewSnapshots { actual: usize, minimum: usize },

    #[error("snapshot {snapshot} has root {found:?}; expected {expected:?}")]
    MismatchedRoots {
        snapshot: usize,
        expected: std::path::PathBuf,
        found: std::path::PathBuf,
    },

    #[error("snapshot {snapshot} uses incompatible schema version {found}; expected {supported}")]
    IncompatibleSchema {
        snapshot: usize,
        found: u32,
        supported: u32,
    },

    #[error("snapshot {snapshot} is invalid: {reason}")]
    InvalidScan { snapshot: usize, reason: String },
}
