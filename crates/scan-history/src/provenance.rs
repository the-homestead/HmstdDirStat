use std::path::PathBuf;

/// The method used to perform the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMethod {
    /// Portable std::fs traversal.
    GenericTraversal,
    /// NTFS MFT based traversal (future extension).
    NtfsMft,
}

impl ScanMethod {
    pub fn code(self) -> u8 {
        match self {
            ScanMethod::GenericTraversal => 1,
            ScanMethod::NtfsMft => 2,
        }
    }
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(ScanMethod::GenericTraversal),
            2 => Some(ScanMethod::NtfsMft),
            _ => None,
        }
    }
}

/// Information about where the scan originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProvenance {
    pub method: ScanMethod,
    pub timestamp_unix_secs: i64,
    pub root: PathBuf,
}
