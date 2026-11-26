use std::path::{Path, PathBuf};

use timberstat_filesystem::{EntryKind, FileIdentity};
use timberstat_scan_history::ScanEntry;

/// Exact subtree sizes. `u128` avoids saturation when many `u64` sizes are summed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SizeTotals {
    pub logical: u128,
    /// `None` means at least one entry in the subtree has no physical size.
    pub physical: Option<u128>,
}

impl SizeTotals {
    pub(crate) const ZERO: Self = Self {
        logical: 0,
        physical: Some(0),
    };
}

/// Signed change between two exact size totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeDelta {
    pub before: SizeTotals,
    pub after: SizeTotals,
    pub logical: i128,
    /// `None` when either side's physical total is unknown.
    pub physical: Option<i128>,
}

impl SizeDelta {
    pub(crate) fn between(before: SizeTotals, after: SizeTotals) -> Self {
        let logical = signed_difference(before.logical, after.logical);
        let physical = match (before.physical, after.physical) {
            (Some(before), Some(after)) => Some(signed_difference(before, after)),
            _ => None,
        };
        Self {
            before,
            after,
            logical,
            physical,
        }
    }
}

fn signed_difference(before: u128, after: u128) -> i128 {
    if after >= before {
        i128::try_from(after - before).expect("scan size difference fits i128")
    } else {
        -i128::try_from(before - after).expect("scan size difference fits i128")
    }
}

/// Subtree size change for one directory path. Nested directory deltas overlap by design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryDelta {
    pub path: PathBuf,
    pub delta: SizeDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeCategory {
    New,
    Deleted,
    Modified,
}

/// Metadata captured on one side of a path comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySnapshot {
    pub kind: EntryKind,
    pub logical_size: u64,
    pub physical_size: Option<u64>,
    pub modified_unix_secs: Option<i64>,
    pub identity: Option<FileIdentity>,
    pub hardlink_duplicate: bool,
}

impl EntrySnapshot {
    pub(crate) fn from_entry(entry: &ScanEntry) -> Self {
        Self {
            kind: entry.kind,
            logical_size: entry.logical_size,
            physical_size: entry.physical_size,
            modified_unix_secs: entry.modified_unix_secs,
            identity: entry.identity,
            hardlink_duplicate: entry.hardlink_duplicate,
        }
    }
}

/// Why two entries at the same path are classified as modified.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModificationReasons {
    pub logical_size: bool,
    pub physical_size: bool,
    pub modified_time: bool,
    pub kind: bool,
    pub identity: bool,
}

impl ModificationReasons {
    pub(crate) fn between(before: &ScanEntry, after: &ScanEntry) -> Self {
        Self {
            logical_size: before.logical_size != after.logical_size,
            physical_size: matches!(
                (before.physical_size, after.physical_size),
                (Some(before), Some(after)) if before != after
            ),
            modified_time: matches!(
                (before.modified_unix_secs, after.modified_unix_secs),
                (Some(before), Some(after)) if before != after
            ),
            kind: before.kind != after.kind,
            identity: matches!(
                (before.identity, after.identity),
                (Some(before), Some(after)) if before != after
            ),
        }
    }

    pub(crate) fn any(self) -> bool {
        self.logical_size || self.physical_size || self.modified_time || self.kind || self.identity
    }
}

/// A path-based change suitable for tree highlighting and detail views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryChange {
    pub path: PathBuf,
    pub category: ChangeCategory,
    pub before: Option<EntrySnapshot>,
    pub after: Option<EntrySnapshot>,
    pub reasons: ModificationReasons,
}

impl EntryChange {
    /// Change in this entry's own logical size, excluding descendants.
    pub fn logical_delta(&self) -> i128 {
        let before = self.before.as_ref().map_or(0, |entry| entry.logical_size);
        let after = self.after.as_ref().map_or(0, |entry| entry.logical_size);
        i128::from(after) - i128::from(before)
    }

    /// Change in this entry's own physical charge, or `None` if either side is unknown.
    pub fn physical_delta(&self) -> Option<i128> {
        let before = match &self.before {
            Some(entry) => entry.physical_size,
            None => Some(0),
        }?;
        let after = match &self.after {
            Some(entry) => entry.physical_size,
            None => Some(0),
        }?;
        Some(i128::from(after) - i128::from(before))
    }
}

/// Complete, deterministic diff of two scans with the same root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanComparison {
    pub root: PathBuf,
    pub before_timestamp_unix_secs: i64,
    pub after_timestamp_unix_secs: i64,
    pub changes: Vec<EntryChange>,
    pub directories: Vec<DirectoryDelta>,
    pub total: SizeDelta,
}

impl ScanComparison {
    pub fn change_at(&self, path: &Path) -> Option<&EntryChange> {
        self.changes
            .binary_search_by(|change| change.path.as_path().cmp(path))
            .ok()
            .map(|index| &self.changes[index])
    }

    pub fn directory_at(&self, path: &Path) -> Option<&DirectoryDelta> {
        self.directories
            .binary_search_by(|directory| directory.path.as_path().cmp(path))
            .ok()
            .map(|index| &self.directories[index])
    }

    pub fn new_entries(&self) -> impl Iterator<Item = &EntryChange> {
        self.entries_in_category(ChangeCategory::New)
    }

    pub fn deleted_entries(&self) -> impl Iterator<Item = &EntryChange> {
        self.entries_in_category(ChangeCategory::Deleted)
    }

    pub fn modified_entries(&self) -> impl Iterator<Item = &EntryChange> {
        self.entries_in_category(ChangeCategory::Modified)
    }

    fn entries_in_category(&self, category: ChangeCategory) -> impl Iterator<Item = &EntryChange> {
        self.changes
            .iter()
            .filter(move |change| change.category == category)
    }

    /// Sum own-entry deltas and compare them with the root subtree difference.
    pub fn logical_delta_reconciles(&self) -> bool {
        self.changes
            .iter()
            .map(EntryChange::logical_delta)
            .sum::<i128>()
            == self.total.logical
    }

    /// Physical reconciliation is available only when every physical size is known.
    pub fn physical_delta_reconciles(&self) -> Option<bool> {
        let expected = self.total.physical?;
        let actual = self.changes.iter().try_fold(0_i128, |total, change| {
            Some(total + change.physical_delta()?)
        })?;
        Some(actual == expected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrendSnapshot {
    pub timestamp_unix_secs: i64,
    pub total: SizeTotals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrendInterval {
    pub from_timestamp_unix_secs: i64,
    pub to_timestamp_unix_secs: i64,
    pub delta: SizeDelta,
}

/// Root-size history in caller-supplied, oldest-to-newest order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrowthTrend {
    pub root: PathBuf,
    pub snapshots: Vec<TrendSnapshot>,
    pub intervals: Vec<TrendInterval>,
}
