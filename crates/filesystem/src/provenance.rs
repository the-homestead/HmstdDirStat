use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Which technique produced a filesystem tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMethod {
    /// Portable `std::fs` traversal.
    GenericTraversal,
    /// Read from the NTFS master file table (elevated, Windows).
    NtfsMft,
}

impl ScanMethod {
    /// Stable on-disk code used by scan-history. These values are a persistence contract.
    pub fn code(self) -> u8 {
        match self {
            Self::GenericTraversal => 1,
            Self::NtfsMft => 2,
        }
    }

    /// Resolves a stable on-disk code used by scan-history.
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::GenericTraversal),
            2 => Some(Self::NtfsMft),
            _ => None,
        }
    }
}

/// Information about how and where a filesystem scan originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProvenance {
    pub method: ScanMethod,
    pub timestamp_unix_secs: i64,
    pub root: PathBuf,
}

impl ScanProvenance {
    /// Captures provenance at the current system time.
    pub fn now(method: ScanMethod, root: impl Into<PathBuf>) -> Self {
        let timestamp_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_secs()).ok())
            .unwrap_or(0);
        Self {
            method,
            timestamp_unix_secs,
            root: root.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_codes_are_stable() {
        assert_eq!(ScanMethod::GenericTraversal.code(), 1);
        assert_eq!(ScanMethod::NtfsMft.code(), 2);
        assert_eq!(ScanMethod::from_code(1), Some(ScanMethod::GenericTraversal));
        assert_eq!(ScanMethod::from_code(2), Some(ScanMethod::NtfsMft));
        assert_eq!(ScanMethod::from_code(0), None);
        assert_eq!(ScanMethod::from_code(3), None);
    }
}
