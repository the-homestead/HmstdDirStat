//! Saved scans: writing the open tree to the scan store and reading one back, plus the
//! labels the history list shows. No GPUI types here, so the logic tests without a window.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use timberstat_filesystem::FileTree;
use timberstat_scan_history::{
    Scan, ScanMethod as HistoryMethod, ScanProvenance, ScanStore, ScanSummary as SavedScan,
};
use timberstat_scanner::ScanMethod as ScannerMethod;

use crate::scan::ScanSummary;

/// Where saved scans live for this user.
pub fn store_dir() -> Result<PathBuf, String> {
    timberstat_core::config::default_scans_dir().map_err(|error| error.to_string())
}

pub fn method_from_scanner(method: ScannerMethod) -> HistoryMethod {
    match method {
        ScannerMethod::GenericTraversal => HistoryMethod::GenericTraversal,
        ScannerMethod::NtfsMft => HistoryMethod::NtfsMft,
    }
}

/// The method name as the result panels show it. Matches [`crate::scan::summarize`].
pub fn method_label(method: HistoryMethod) -> &'static str {
    match method {
        HistoryMethod::GenericTraversal => "generic traversal",
        HistoryMethod::NtfsMft => "file table (MFT)",
    }
}

/// Provenance for a scan that finished now.
pub fn new_provenance(method: HistoryMethod, root: &Path) -> ScanProvenance {
    let timestamp_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0);
    ScanProvenance {
        method,
        timestamp_unix_secs,
        root: root.to_path_buf(),
    }
}

/// Writes the tree into the store. The store renames a complete file into place, so an
/// interrupted save leaves the previously saved scans intact.
pub fn save_scan(dir: &Path, tree: &FileTree, provenance: &ScanProvenance) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    let scan = Scan::from_tree(provenance.clone(), tree).map_err(|error| error.to_string())?;
    ScanStore::new(dir)
        .save(&scan)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Every saved scan, oldest name first. A folder that does not exist yet has no scans.
pub fn list_scans(dir: &Path) -> Result<Vec<SavedScan>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    ScanStore::new(dir)
        .list()
        .map_err(|error| error.to_string())
}

/// Loads one saved scan and rebuilds its tree.
pub fn open_scan(dir: &Path, id: &str) -> Result<(FileTree, ScanProvenance), String> {
    let scan = ScanStore::new(dir)
        .load(id)
        .map_err(|error| error.to_string())?;
    let tree = scan.to_tree().map_err(|error| error.to_string())?;
    Ok((tree, scan.provenance))
}

/// The result-panel summary of a tree, computed the same way a live scan's is.
pub fn summary_for_tree(tree: &FileTree, provenance: &ScanProvenance) -> ScanSummary {
    let totals = tree
        .node(tree.root())
        .map(|node| node.totals())
        .unwrap_or_default();
    let mut entries = 0u64;
    let mut hardlink_duplicates = 0u64;
    let mut unreadable = 0u64;
    for id in tree.attached_ids() {
        entries += 1;
        if let Some(node) = tree.node(id) {
            hardlink_duplicates += u64::from(node.is_hardlink_duplicate());
            unreadable += u64::from(node.is_unreadable());
        }
    }
    ScanSummary {
        root: provenance.root.clone(),
        files: totals.files,
        directories: totals.directories,
        logical_bytes: totals.logical,
        physical_bytes: totals.physical_known.then_some(totals.physical),
        complete: totals.complete,
        issues: unreadable,
        issue_paths: Vec::new(),
        hardlink_duplicates,
        entries,
        elapsed: Duration::ZERO,
        method: method_label(provenance.method),
    }
}

/// A Unix timestamp as `YYYY-MM-DD HH:MM UTC`. Std has no local time zone, so the label says UTC.
pub fn format_utc(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let seconds_of_day = unix_secs.rem_euclid(86_400);
    let (hour, minute) = (seconds_of_day / 3600, seconds_of_day % 3600 / 60);
    // Civil date from a day count since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

#[cfg(test)]
mod tests {
    use super::*;
    use timberstat_filesystem::{EntryKind, Metadata, NewEntry};

    fn sample_tree() -> FileTree {
        let mut tree = FileTree::with_root(NewEntry::directory("data".into(), Metadata::default()));
        let root = tree.root();
        tree.add_child(
            root,
            NewEntry {
                name: "a.bin".into(),
                kind: EntryKind::File,
                logical_size: 300,
                physical_size: Some(4096),
                identity: None,
                hardlink_duplicate: false,
                unreadable: false,
                metadata: Metadata::default(),
            },
        )
        .unwrap();
        let locked = tree
            .add_child(
                root,
                NewEntry::directory("locked".into(), Metadata::default()),
            )
            .unwrap();
        tree.mark_unreadable(locked).unwrap();
        tree.roll_up();
        tree
    }

    fn provenance(root: &Path) -> ScanProvenance {
        ScanProvenance {
            method: HistoryMethod::GenericTraversal,
            timestamp_unix_secs: 1_700_000_000,
            root: root.to_path_buf(),
        }
    }

    #[test]
    fn utc_labels_match_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13 UTC");
        // A leap day: 2024-02-29 12:00 UTC.
        assert_eq!(format_utc(1_709_208_000), "2024-02-29 12:00 UTC");
        // Before 1970 still formats.
        assert_eq!(format_utc(-86_400), "1969-12-31 00:00 UTC");
    }

    #[test]
    fn summary_matches_the_tree_it_describes() {
        let tree = sample_tree();
        let summary = summary_for_tree(&tree, &provenance(Path::new("C:/data")));
        assert_eq!(summary.files, 1);
        assert_eq!(summary.directories, 2);
        assert_eq!(summary.logical_bytes, 300);
        // Directories carry no physical size here, so the total is unknown, as Totals reports.
        assert_eq!(summary.physical_bytes, None);
        assert!(
            !summary.complete,
            "the unreadable folder makes the scan partial"
        );
        assert_eq!(summary.issues, 1);
        assert_eq!(summary.entries, 3);
        assert_eq!(summary.method, "generic traversal");
        assert_eq!(summary.elapsed, Duration::ZERO);
    }

    #[test]
    fn saved_scan_lists_and_reopens_from_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("scans");
        assert!(list_scans(&store_path).unwrap().is_empty());

        let tree = sample_tree();
        let root = Path::new("C:/data");
        save_scan(&store_path, &tree, &provenance(root)).unwrap();

        let listed = list_scans(&store_path).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].error.is_none());
        assert_eq!(listed[0].provenance.root, root);

        let (reopened, saved) = open_scan(&store_path, &listed[0].id).unwrap();
        assert_eq!(saved, provenance(root));
        assert_eq!(
            summary_for_tree(&reopened, &saved),
            summary_for_tree(&tree, &provenance(root))
        );
    }
}
