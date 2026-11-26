//! File operations for HmstdDirStat: delete, copy, move and rename.
//!
//! Everything here is GPUI-independent and split into a plan step and an
//! execute step. Planning reads only the in-memory tree. Execution touches
//! the disk and is meant for a worker thread. Results are reports that list
//! every failed path. A partial run is never reported as success.
//!
//! No `unsafe`. The Recycle Bin call goes through the `trash` crate.

mod copy;
mod delete;
mod delete_plan;
mod disk;
mod model;
mod preview;
mod rename;
mod sync;
mod transfer;
mod transfer_plan;

pub use delete::{DeleteReport, execute_delete, execute_delete_with};
pub use delete_plan::{
    ConfirmedDelete, DeleteMode, DeletePlan, build_delete_plan, build_delete_plan_capped,
};
pub use model::{Control, Failure, FailureReason, PlanError, Preview, WorkItem, classify_io};
pub use preview::{DEFAULT_PREVIEW_CAP, tree_base};
pub use rename::{RenameOutcome, rename_path, validate_name};
pub use sync::{apply_delete, apply_moves, apply_rename};
pub use transfer::{SkipReason, Skipped, TransferReport, execute_transfer};
pub use transfer_plan::{Collision, TransferKind, TransferPlan, plan_transfer};
