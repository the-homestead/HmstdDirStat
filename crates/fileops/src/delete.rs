//! Delete executor. Runs on a worker thread and never touches the UI.
//!
//! Recycle Bin mode sends each selected root to the bin as one call. Permanent
//! mode removes entries bottom-up, so a directory is only removed after its
//! children. A failed child keeps its parents, and each failure is reported
//! with its path. The tree is updated afterwards by [`crate::sync`].

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use hmstd_filesystem::{EntryKind, NodeId};

use crate::delete_plan::{ConfirmedDelete, DeleteMode};
use crate::disk::{is_dir_entry, is_link_to_dir};
use crate::model::{Control, Failure, FailureReason, WorkItem, classify_io};

/// Outcome of one delete run. A report with failures or unattempted entries
/// is partial and must never be shown as a success.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteReport {
    pub mode: Option<DeleteMode>,
    /// Ids of every node whose path is gone from disk (including already missing).
    pub removed: Vec<NodeId>,
    pub removed_files: u64,
    pub removed_directories: u64,
    pub removed_other: u64,
    pub removed_logical: u64,
    pub removed_physical: u64,
    /// Entries that were already missing on disk; counted as removed.
    pub already_missing: u64,
    pub failures: Vec<Failure>,
    /// Entries skipped because cancellation arrived first.
    pub not_attempted: u64,
    pub cancelled: bool,
    pub total_entries: u64,
}

impl DeleteReport {
    /// True only when nothing failed and nothing was left unattempted.
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty() && self.not_attempted == 0
    }

    /// One line for the status bar. Partial runs say so explicitly.
    pub fn summary(&self) -> String {
        let removed = self.removed.len();
        if self.is_complete() {
            return format!("Deleted {removed} of {} entries.", self.total_entries);
        }
        let cancelled = if self.cancelled { ", cancelled" } else { "" };
        format!(
            "Partial: {removed} of {} entries removed, {} failed, {} not attempted{cancelled}.",
            self.total_entries,
            self.failures.len(),
            self.not_attempted,
        )
    }
}

/// Runs a confirmed delete with the platform Recycle Bin.
pub fn execute_delete(confirmed: ConfirmedDelete, control: &Control) -> DeleteReport {
    execute_delete_with(confirmed, control, &|path| trash::delete(path))
}

/// Runs a confirmed delete with an injected Recycle Bin call. Tests use this to
/// simulate bin failures without touching the real bin.
pub fn execute_delete_with(
    confirmed: ConfirmedDelete,
    control: &Control,
    recycle: &dyn Fn(&Path) -> Result<(), trash::Error>,
) -> DeleteReport {
    let ConfirmedDelete { mode, plan } = confirmed;
    let items = plan.items();
    let mut report = DeleteReport {
        mode: Some(mode),
        total_entries: items.len() as u64,
        ..DeleteReport::default()
    };
    control.set_total(items.len() as u64);
    match mode {
        DeleteMode::RecycleBin => {
            for group in plan.groups() {
                let entries = &items[group.clone()];
                let Some(root) = entries.last() else {
                    continue;
                };
                if control.is_cancelled() {
                    report.not_attempted += entries.len() as u64;
                    continue;
                }
                match recycle(&root.path) {
                    Ok(()) => {
                        for entry in entries {
                            record_removed(&mut report, entry);
                        }
                        control.advance(entries.len() as u64);
                    }
                    Err(error) => {
                        report.failures.push(Failure::new(
                            &root.path,
                            recycle_reason(&error),
                            error.to_string(),
                        ));
                        control.advance(entries.len() as u64);
                    }
                }
            }
        }
        DeleteMode::Permanent => remove_permanently(items, control, &mut report),
    }
    report.cancelled = control.is_cancelled();
    report
}

fn remove_permanently(items: &[WorkItem], control: &Control, report: &mut DeleteReport) {
    // Parents whose child failed. Their own removal would fail or lose the child's status.
    let mut blocked: HashSet<NodeId> = HashSet::new();
    for (index, item) in items.iter().enumerate() {
        if control.is_cancelled() {
            report.not_attempted += (items.len() - index) as u64;
            return;
        }
        if blocked.contains(&item.id) {
            report.failures.push(Failure::new(
                &item.path,
                FailureReason::Blocked,
                "a child entry was not removed, so this folder was kept",
            ));
            if let Some(parent) = item.parent {
                blocked.insert(parent);
            }
            control.advance(1);
            continue;
        }
        match remove_entry(item) {
            Ok(already_missing) => {
                record_removed(report, item);
                if already_missing {
                    report.already_missing += 1;
                }
            }
            Err(failure) => {
                report.failures.push(failure);
                if let Some(parent) = item.parent {
                    blocked.insert(parent);
                }
            }
        }
        control.advance(1);
    }
}

/// Removes one entry. Returns `Ok(true)` when it was already missing.
/// Symlinks and junctions are removed as links; their targets are never touched.
fn remove_entry(item: &WorkItem) -> Result<bool, Failure> {
    let meta = match fs::symlink_metadata(&item.path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(Failure::from_io(&item.path, &error)),
    };
    let disk_is_real_dir = is_dir_entry(&meta);
    if item.kind == EntryKind::Directory && !disk_is_real_dir {
        return Err(Failure::new(
            &item.path,
            FailureReason::Conflict,
            "the entry changed type since the scan; not removed",
        ));
    }
    if item.kind != EntryKind::Directory && disk_is_real_dir {
        return Err(Failure::new(
            &item.path,
            FailureReason::Conflict,
            "the entry changed type since the scan; not removed",
        ));
    }
    let result = if disk_is_real_dir || is_link_to_dir(&meta) {
        fs::remove_dir(&item.path)
    } else {
        fs::remove_file(&item.path)
    };
    match result {
        Ok(()) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(Failure::from_io(&item.path, &error)),
    }
}

fn record_removed(report: &mut DeleteReport, item: &WorkItem) {
    report.removed.push(item.id);
    match item.kind {
        EntryKind::File => report.removed_files += 1,
        EntryKind::Directory => report.removed_directories += 1,
        EntryKind::Symlink | EntryKind::Other => report.removed_other += 1,
    }
    report.removed_logical = report.removed_logical.saturating_add(item.logical);
    if let Some(physical) = item.physical {
        report.removed_physical = report.removed_physical.saturating_add(physical);
    }
}

/// Maps a Recycle Bin error. Anything the bin itself rejects is `RecycleBin`,
/// so the caller can say so. The executor never falls back to permanent delete.
fn recycle_reason(error: &trash::Error) -> FailureReason {
    match error {
        trash::Error::CouldNotAccess { .. } => FailureReason::AccessDenied,
        trash::Error::Os { code, .. } => {
            match classify_io(&std::io::Error::from_raw_os_error(*code)) {
                FailureReason::Other => FailureReason::RecycleBin,
                reason => reason,
            }
        }
        _ => FailureReason::RecycleBin,
    }
}
