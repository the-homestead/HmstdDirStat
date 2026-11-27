//! The shared engine. Which mode a drive gets, the folder walk, the NTFS file-table read and the
//! duplicate search are decided here. The CLI and the GUI call these functions and only format
//! what they return; neither keeps its own copy of the rules.

pub mod fast;
pub mod probe;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use timberstat_core::scan_mode::resolve;
pub use timberstat_core::scan_mode::{ScanKind, ScanModeChoice};
use timberstat_duplicates::{DuplicateReport, Options, Progress, file_items, find_duplicates};
use timberstat_fileops::tree_base;
use timberstat_filesystem::{FileTree, ScanMethod, ScanProvenance};
use timberstat_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanOutput,
    ScanProgress, ScanReport,
};

/// True where the NTFS file table can be read at all (Windows only).
pub const FAST_NTFS_AVAILABLE: bool = cfg!(windows);

/// Why a scan did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    Cancelled,
    Failed(String),
}

impl From<ScanError> for EngineError {
    fn from(error: ScanError) -> Self {
        match error {
            ScanError::Cancelled => Self::Cancelled,
            other => Self::Failed(other.to_string()),
        }
    }
}

/// True for a drive root such as `W:`, `W:\` or `W:/`. Only a drive root can be read from the
/// file table, because the table covers the whole volume.
pub fn is_drive_root(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['/', '\\']);
    let bytes = trimmed.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// The drive letter of a path, uppercase. `None` when the path has no drive letter.
pub fn drive_letter(path: &Path) -> Option<char> {
    let first = path.to_string_lossy().chars().next()?;
    first
        .is_ascii_alphabetic()
        .then(|| first.to_ascii_uppercase())
}

/// The mode a drive letter is set to. Letters without an override use `Auto`.
pub fn choice_for(modes: &BTreeMap<String, ScanModeChoice>, letter: char) -> ScanModeChoice {
    modes
        .get(&letter.to_ascii_uppercase().to_string())
        .copied()
        .unwrap_or_default()
}

/// The mode a scan of `root` runs with. This is the one place that decides it.
///
/// `known_hdd` is a drive-type answer the caller already has (the GUI caches it for the session).
/// When it is `None` and the choice is `Auto`, the drive is probed now. Only drive roots can use
/// the file table; anything else is walked, whatever the choice says.
pub fn plan(root: &Path, choice: ScanModeChoice, known_hdd: Option<bool>) -> ScanKind {
    let available = FAST_NTFS_AVAILABLE && is_drive_root(root);
    let hdd = match (choice, available) {
        (ScanModeChoice::Auto, true) => known_hdd.unwrap_or_else(|| probe::is_spinning_hdd(root)),
        _ => false,
    };
    resolve(choice, hdd, available)
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

/// A finished scan, whichever mode produced it.
pub struct Scanned {
    pub tree: FileTree,
    pub report: ScanReport,
    pub kind: ScanKind,
    pub root: PathBuf,
}

/// Walks a folder tree with the shared skip rules. `progress` gets a snapshot as the walk runs.
pub fn scan_walk(
    root: &Path,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<ScanOutput, EngineError> {
    Ok(GenericScanner.scan(root, &ScanOptions::default(), cancel, progress)?)
}

/// Reads a drive's file table through the elevated helper and returns it as a scan. `root`
/// must be a drive root (see [`is_drive_root`]). Blocks until the helper exits.
pub fn scan_mft(root: &Path, paths: &fast::FastPaths) -> Result<ScanOutput, EngineError> {
    let volume = fast::raw_volume_name(root)
        .ok_or_else(|| EngineError::Failed("the file table needs a drive letter".to_string()))?;
    let (_summary, mut tree) = fast::run_elevated(&volume, paths).map_err(EngineError::Failed)?;
    tree.set_provenance(ScanProvenance::now(ScanMethod::NtfsMft, root));
    let mut report = ScanReport::new();
    report.entries_visited = u64::try_from(tree.attached_ids().count()).unwrap_or(u64::MAX);
    Ok(ScanOutput { tree, report })
}

/// Finds byte-verified duplicates among the files of a scanned tree. `root` is the scanned
/// folder, which the file paths are built from.
pub fn find_duplicates_in(tree: &FileTree, root: &Path, progress: &Progress) -> DuplicateReport {
    let items = file_items(tree, &tree_base(root));
    find_duplicates(&items, &Options::default(), progress)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn only_drive_roots_count_as_drive_roots() {
        assert!(is_drive_root(Path::new("W:")));
        assert!(is_drive_root(Path::new("W:\\")));
        assert!(is_drive_root(Path::new("w:/")));
        assert!(!is_drive_root(Path::new("W:/folder")));
        assert!(!is_drive_root(Path::new("/home/user")));
        assert!(!is_drive_root(Path::new("1:")));
    }

    #[test]
    fn drive_letter_is_uppercase_and_absent_for_plain_paths() {
        assert_eq!(drive_letter(Path::new("w:/x")), Some('W'));
        assert_eq!(drive_letter(Path::new("/x")), None);
    }

    #[test]
    fn choice_lookup_ignores_case_and_defaults_to_auto() {
        let mut modes = BTreeMap::new();
        modes.insert("W".to_string(), ScanModeChoice::Walk);
        assert_eq!(choice_for(&modes, 'w'), ScanModeChoice::Walk);
        assert_eq!(choice_for(&modes, 'C'), ScanModeChoice::Auto);
    }

    #[test]
    fn non_drive_paths_are_always_walked() {
        // Even an explicit Fast choice cannot read a folder from the file table.
        assert_eq!(
            plan(Path::new("/home/user"), ScanModeChoice::Fast, Some(true)),
            ScanKind::Walk
        );
    }

    #[test]
    fn auto_uses_the_file_table_only_for_spinning_drive_roots_where_available() {
        let expected = if FAST_NTFS_AVAILABLE {
            ScanKind::Fast
        } else {
            ScanKind::Walk
        };
        assert_eq!(
            plan(Path::new("W:/"), ScanModeChoice::Auto, Some(true)),
            expected
        );
        assert_eq!(
            plan(Path::new("W:/"), ScanModeChoice::Auto, Some(false)),
            ScanKind::Walk
        );
    }

    #[test]
    fn walk_choice_always_walks() {
        assert_eq!(
            plan(Path::new("W:/"), ScanModeChoice::Walk, Some(true)),
            ScanKind::Walk
        );
    }
}
