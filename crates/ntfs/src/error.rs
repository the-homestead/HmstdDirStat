use thiserror::Error;

#[derive(Debug, Error)]
pub enum NtfsError {
    #[error("read failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("volume is not NTFS")]
    NotNtfs,
    #[error("unusable boot sector: {0}")]
    BadBootSector(&'static str),
    #[error("record data is truncated: {0}")]
    Truncated(&'static str),
    #[error("corrupt metadata: {0}")]
    Corrupt(&'static str),
    #[error("record fixup does not match the update sequence number")]
    FixupMismatch,
    #[error("internal tree error: {0}")]
    Tree(#[from] hmstd_filesystem::TreeError),
}
