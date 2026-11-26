use std::path::{Path, PathBuf};

use timberstat_filesystem::EntryKind;
use timberstat_scan_history::Scan;

use crate::{
    ComparisonError,
    index::{IndexedScan, ensure_compatible_schema},
    model::{
        ChangeCategory, DirectoryDelta, EntryChange, EntrySnapshot, ModificationReasons,
        ScanComparison, SizeDelta, SizeTotals,
    },
};

/// Compare two scans by full path. Renames are one deletion plus one new entry.
pub fn compare(before: &Scan, after: &Scan) -> Result<ScanComparison, ComparisonError> {
    compare_inner(
        before,
        after,
        before.schema_version(),
        after.schema_version(),
    )
}

#[cfg(test)]
fn compare_with_versions(
    before: &Scan,
    after: &Scan,
    before_version: u32,
    after_version: u32,
) -> Result<ScanComparison, ComparisonError> {
    compare_inner(before, after, before_version, after_version)
}

fn compare_inner(
    before: &Scan,
    after: &Scan,
    before_version: u32,
    after_version: u32,
) -> Result<ScanComparison, ComparisonError> {
    ensure_compatible_schema(before_version, 0)?;
    ensure_compatible_schema(after_version, 1)?;
    if before.root_path() != after.root_path() {
        return Err(ComparisonError::MismatchedRoots {
            snapshot: 1,
            expected: before.root_path().to_path_buf(),
            found: after.root_path().to_path_buf(),
        });
    }

    let root = before.root_path().to_path_buf();
    let before_timestamp = before.provenance.timestamp_unix_secs;
    let after_timestamp = after.provenance.timestamp_unix_secs;
    let before = IndexedScan::build(before, 0)?;
    let after = IndexedScan::build(after, 1)?;
    let mut changes = entry_changes(&before, &after);
    changes.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    let directories = directory_deltas(&before, &after);
    let total = SizeDelta::between(before.root_total(), after.root_total());

    Ok(ScanComparison {
        root,
        before_timestamp_unix_secs: before_timestamp,
        after_timestamp_unix_secs: after_timestamp,
        changes,
        directories,
        total,
    })
}

fn entry_changes(before: &IndexedScan<'_>, after: &IndexedScan<'_>) -> Vec<EntryChange> {
    let mut changes = Vec::new();
    for (path, before_entry) in before.iter() {
        let before_snapshot = EntrySnapshot::from_entry(before_entry);
        match after.entry(path) {
            None => changes.push(EntryChange {
                path: path.to_path_buf(),
                category: ChangeCategory::Deleted,
                before: Some(before_snapshot),
                after: None,
                reasons: ModificationReasons::default(),
            }),
            Some(after_entry) => {
                let reasons = ModificationReasons::between(before_entry, after_entry);
                if reasons.any() {
                    changes.push(EntryChange {
                        path: path.to_path_buf(),
                        category: ChangeCategory::Modified,
                        before: Some(before_snapshot),
                        after: Some(EntrySnapshot::from_entry(after_entry)),
                        reasons,
                    });
                }
            }
        }
    }
    for (path, after_entry) in after.iter() {
        if before.entry(path).is_none() {
            changes.push(EntryChange {
                path: path.to_path_buf(),
                category: ChangeCategory::New,
                before: None,
                after: Some(EntrySnapshot::from_entry(after_entry)),
                reasons: ModificationReasons::default(),
            });
        }
    }
    changes
}

fn directory_deltas(before: &IndexedScan<'_>, after: &IndexedScan<'_>) -> Vec<DirectoryDelta> {
    let mut paths: Vec<PathBuf> = before
        .iter()
        .chain(after.iter())
        .filter(|(_, entry)| entry.kind == EntryKind::Directory)
        .map(|(path, _)| path.to_path_buf())
        .collect();
    paths.sort_unstable();
    paths.dedup();

    paths
        .into_iter()
        .map(|path| {
            let before_total = directory_total(before, &path).unwrap_or(SizeTotals::ZERO);
            let after_total = directory_total(after, &path).unwrap_or(SizeTotals::ZERO);
            DirectoryDelta {
                path,
                delta: SizeDelta::between(before_total, after_total),
            }
        })
        .collect()
}

fn directory_total(index: &IndexedScan<'_>, path: &Path) -> Option<SizeTotals> {
    let entry = index.entry(path)?;
    (entry.kind == EntryKind::Directory)
        .then(|| index.total(path))
        .flatten()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use timberstat_filesystem::EntryKind;
    use timberstat_scan_history::{CURRENT_SCHEMA, ScanEntry, ScanMethod, ScanProvenance};

    use super::*;

    fn scan() -> Scan {
        Scan::new(
            ScanProvenance {
                method: ScanMethod::GenericTraversal,
                timestamp_unix_secs: 1,
                root: PathBuf::from("C:/root"),
            },
            vec![ScanEntry {
                parent: None,
                name: OsString::from("root"),
                kind: EntryKind::Directory,
                logical_size: 0,
                physical_size: Some(0),
                identity: None,
                hardlink_duplicate: false,
                unreadable: false,
                modified_unix_secs: Some(1),
                readonly: false,
                windows_attributes: None,
            }],
        )
    }

    #[test]
    fn incompatible_schema_returns_error_instead_of_partial_diff() {
        let before = scan();
        let after = scan();
        let error =
            compare_with_versions(&before, &after, CURRENT_SCHEMA, CURRENT_SCHEMA + 1).unwrap_err();
        assert_eq!(
            error,
            ComparisonError::IncompatibleSchema {
                snapshot: 1,
                found: CURRENT_SCHEMA + 1,
                supported: CURRENT_SCHEMA,
            }
        );
    }
}
