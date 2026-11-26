//! Copy and move executor. Runs on a worker thread.
//!
//! Each selected top-level entry is handled on its own, so one failure never
//! stops the others. A move removes its source only after the copy finished
//! with no failures or skips. Same-volume moves use a rename.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use hmstd_filesystem::{EntryKind, NodeId};

use crate::copy::{CopyTally, copy_entry, remove_tree_on_disk};
use crate::disk::{free_name, is_dir_entry, is_link_like, same_location};
use crate::model::{Control, Failure, FailureReason, WorkItem};
use crate::transfer_plan::{Collision, TransferKind, TransferPlan};

/// Why an entry was left alone on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The collision policy said skip.
    CollisionSkipped,
    /// The destination is the source itself; nothing was done.
    SameLocation,
    /// A symlink or reparse point inside the tree. Not copied, so the copy is incomplete.
    LinkNotCopied,
    /// A device, pipe or socket. Not copied, so the copy is incomplete.
    NotRegularFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// Outcome of a copy or move. Partial results are never reported as success.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferReport {
    pub kind: Option<TransferKind>,
    pub top_level_total: u64,
    /// Top-level items fully copied or moved.
    pub done_top_level: u64,
    /// Final destination paths of the finished top-level items.
    pub destinations: Vec<PathBuf>,
    /// Source roots that left their old location. Feed to [`crate::apply_moves`].
    pub moved: Vec<NodeId>,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub skipped: Vec<Skipped>,
    pub failures: Vec<Failure>,
    /// Top-level items not attempted or left unfinished by cancellation.
    pub not_attempted: u64,
    pub cancelled: bool,
}

impl TransferReport {
    /// True when nothing failed, nothing was left unfinished, and no link or
    /// special entry was left behind. Policy skips (skip, same location) still count as complete.
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
            && self.not_attempted == 0
            && !self.skipped.iter().any(|skip| {
                matches!(
                    skip.reason,
                    SkipReason::LinkNotCopied | SkipReason::NotRegularFile
                )
            })
    }

    pub fn summary(&self) -> String {
        let verb = match self.kind {
            Some(TransferKind::Move) => "Moved",
            _ => "Copied",
        };
        if self.is_complete() {
            return format!(
                "{verb} {} of {} items.",
                self.done_top_level, self.top_level_total
            );
        }
        let cancelled = if self.cancelled { ", cancelled" } else { "" };
        format!(
            "Partial: {verb} {} of {} items; {} failed, {} skipped, {} not attempted{cancelled}.",
            self.done_top_level,
            self.top_level_total,
            self.failures.len(),
            self.skipped.len(),
            self.not_attempted,
        )
    }
}

/// Runs a planned copy or move with an explicit collision policy.
pub fn execute_transfer(
    plan: TransferPlan,
    collision: Collision,
    control: &Control,
) -> TransferReport {
    let (kind, dest_dir, items, names) = plan.into_parts();
    let mut report = TransferReport {
        kind: Some(kind),
        top_level_total: items.len() as u64,
        ..TransferReport::default()
    };
    control.set_total(items.len() as u64);
    for (index, item) in items.iter().enumerate() {
        if control.is_cancelled() {
            report.not_attempted += (items.len() - index) as u64;
            break;
        }
        let Some(source_name) = item.path.file_name() else {
            report.failures.push(Failure::new(
                &item.path,
                FailureReason::Other,
                "the entry has no file name",
            ));
            continue;
        };
        let name = names
            .get(index)
            .and_then(Clone::clone)
            .unwrap_or_else(|| source_name.to_os_string());
        transfer_one(
            kind,
            item,
            &dest_dir,
            &name,
            collision,
            control,
            &mut report,
        );
    }
    report.cancelled = control.is_cancelled();
    report
}

fn transfer_one(
    kind: TransferKind,
    item: &WorkItem,
    dest_dir: &Path,
    name: &OsString,
    collision: Collision,
    control: &Control,
    report: &mut TransferReport,
) {
    let is_dir = item.kind == EntryKind::Directory;
    let mut target = dest_dir.join(name);
    let mut overwrite = false;

    match fs::symlink_metadata(&target) {
        Ok(existing) => {
            if same_location(&item.path, &target) {
                report.skipped.push(Skipped {
                    path: target,
                    reason: SkipReason::SameLocation,
                });
                return;
            }
            let existing_is_dir = is_dir_entry(&existing);
            match collision {
                Collision::Skip => {
                    report.skipped.push(Skipped {
                        path: target,
                        reason: SkipReason::CollisionSkipped,
                    });
                    return;
                }
                Collision::Rename => {
                    report.failures.push(Failure::new(
                        &target,
                        FailureReason::Conflict,
                        "that name is taken; choose another name for this item",
                    ));
                    return;
                }
                Collision::KeepBoth => match free_name(dest_dir, name, is_dir) {
                    Some(free) => target = dest_dir.join(free),
                    None => {
                        report.failures.push(Failure::new(
                            &target,
                            FailureReason::Conflict,
                            "no free name found after 9999 tries",
                        ));
                        return;
                    }
                },
                Collision::Overwrite => {
                    let compatible = is_dir == existing_is_dir && !is_link_like(&existing);
                    if !compatible {
                        report.failures.push(Failure::new(
                            &target,
                            FailureReason::Conflict,
                            "a file and a folder cannot replace each other; not replaced",
                        ));
                        return;
                    }
                    overwrite = true;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            report.failures.push(Failure::from_io(&target, &error));
            return;
        }
    }

    // Same-volume move: a rename is atomic and moves nothing else.
    // A folder merge cannot be a rename, so it takes the copy path.
    if kind == TransferKind::Move && !(overwrite && is_dir) {
        match fs::rename(&item.path, &target) {
            Ok(()) => {
                report.moved.push(item.id);
                finish(report, target, control);
                return;
            }
            Err(error) if is_cross_volume(&error) => {}
            Err(error) => {
                report.failures.push(Failure::from_io(&item.path, &error));
                return;
            }
        }
    }

    let mut tally = CopyTally::default();
    let complete = copy_entry(&item.path, &target, overwrite, control, &mut tally);
    merge_tally(report, tally);
    if !complete {
        if control.is_cancelled() {
            report.not_attempted += 1;
        }
        return;
    }
    if kind == TransferKind::Move {
        let removal = remove_tree_on_disk(&item.path);
        if !removal.is_empty() {
            report.failures.extend(removal);
            return;
        }
        report.moved.push(item.id);
    }
    finish(report, target, control);
}

fn finish(report: &mut TransferReport, target: PathBuf, control: &Control) {
    report.destinations.push(target);
    report.done_top_level += 1;
    control.advance(1);
}

fn merge_tally(report: &mut TransferReport, tally: CopyTally) {
    report.files_copied += tally.files;
    report.bytes_copied = report.bytes_copied.saturating_add(tally.bytes);
    report.skipped.extend(tally.skipped);
    report.failures.extend(tally.failures);
}

fn is_cross_volume(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::CrossesDevices {
        return true;
    }
    // ERROR_NOT_SAME_DEVICE. Unix reports EXDEV, which CrossesDevices already covers.
    cfg!(windows) && error.raw_os_error() == Some(17)
}
