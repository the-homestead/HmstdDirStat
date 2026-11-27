//! How a drive is scanned: the NTFS file table (Fast) or a folder walk. `Auto` reads the file
//! table for spinning hard drives, where it is much faster than a walk, and walks everything
//! else. A drive can be pinned to one mode with its own override.

use serde::{Deserialize, Serialize};

/// The mode a user chose for one drive. `Auto` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanModeChoice {
    #[default]
    Auto,
    Fast,
    Walk,
}

/// The mode a scan actually runs with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanKind {
    Fast,
    Walk,
}

impl ScanKind {
    /// The name shown to the user and written to logs.
    pub fn label(self) -> &'static str {
        match self {
            Self::Fast => "file table (MFT)",
            Self::Walk => "folder walk",
        }
    }
}

/// Resolves a choice for one drive. `spinning_hdd` is whether the drive is a spinning disk, and
/// `fast_available` is whether the platform can read the file table at all. Fast is never chosen
/// when it is unavailable, whatever the choice says.
pub fn resolve(choice: ScanModeChoice, spinning_hdd: bool, fast_available: bool) -> ScanKind {
    match choice {
        ScanModeChoice::Walk => ScanKind::Walk,
        ScanModeChoice::Fast if fast_available => ScanKind::Fast,
        ScanModeChoice::Fast => ScanKind::Walk,
        ScanModeChoice::Auto if spinning_hdd && fast_available => ScanKind::Fast,
        ScanModeChoice::Auto => ScanKind::Walk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_uses_fast_only_for_spinning_disks_where_available() {
        assert_eq!(resolve(ScanModeChoice::Auto, true, true), ScanKind::Fast);
        assert_eq!(resolve(ScanModeChoice::Auto, false, true), ScanKind::Walk);
        assert_eq!(resolve(ScanModeChoice::Auto, true, false), ScanKind::Walk);
    }

    #[test]
    fn explicit_choices_override_the_drive_type() {
        assert_eq!(resolve(ScanModeChoice::Walk, true, true), ScanKind::Walk);
        assert_eq!(resolve(ScanModeChoice::Fast, false, true), ScanKind::Fast);
        assert_eq!(resolve(ScanModeChoice::Fast, false, false), ScanKind::Walk);
    }

    #[test]
    fn default_choice_is_auto() {
        assert_eq!(ScanModeChoice::default(), ScanModeChoice::Auto);
    }
}
