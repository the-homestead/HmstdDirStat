//! Delete plans and the confirmation gate.
//!
//! A [`DeletePlan`] is the preview. It can only become a [`ConfirmedDelete`]
//! through `confirm_recycle_bin` or `confirm_permanent`, and the executor only
//! accepts a `ConfirmedDelete`. A delete without a plan the user saw is
//! therefore impossible to express.

use std::ops::Range;
use std::path::Path;

use hmstd_filesystem::{FileTree, NodeId};

use crate::model::{PlanError, Preview, WorkItem};
use crate::preview::{DEFAULT_PREVIEW_CAP, collect_items, normalize_roots, summarize};

/// Where deleted items go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    /// Send to the Windows Recycle Bin (or the platform trash). The default.
    RecycleBin,
    /// Remove from disk immediately. Needs a typed confirmation.
    Permanent,
}

/// Selected entries, their preview totals, and the phrase a permanent delete
/// must be confirmed with.
#[derive(Debug, Clone)]
pub struct DeletePlan {
    items: Vec<WorkItem>,
    /// One range of `items` per selected root. The root is the range's last
    /// item, because subtrees are listed bottom-up.
    groups: Vec<Range<usize>>,
    preview: Preview,
    confirm_phrase: String,
}

impl DeletePlan {
    pub fn preview(&self) -> &Preview {
        &self.preview
    }

    pub fn items(&self) -> &[WorkItem] {
        &self.items
    }

    pub fn groups(&self) -> &[Range<usize>] {
        &self.groups
    }

    /// The text a permanent delete must be typed as: the item name for one
    /// root, or the number of roots for several.
    pub fn confirm_phrase(&self) -> &str {
        &self.confirm_phrase
    }

    /// The user confirmed this preview for the Recycle Bin.
    pub fn confirm_recycle_bin(self) -> ConfirmedDelete {
        ConfirmedDelete {
            mode: DeleteMode::RecycleBin,
            plan: self,
        }
    }

    /// The user typed the confirm phrase. Comparison is exact after trimming
    /// the ends, so a typo or a different name is refused.
    pub fn confirm_permanent(self, typed: &str) -> Result<ConfirmedDelete, PlanError> {
        if typed.trim() != self.confirm_phrase.trim() {
            return Err(PlanError::ConfirmationMismatch);
        }
        Ok(ConfirmedDelete {
            mode: DeleteMode::Permanent,
            plan: self,
        })
    }
}

/// A plan the user confirmed. Only the executor reads it.
#[derive(Debug, Clone)]
pub struct ConfirmedDelete {
    pub(crate) mode: DeleteMode,
    pub(crate) plan: DeletePlan,
}

impl ConfirmedDelete {
    pub fn mode(&self) -> DeleteMode {
        self.mode
    }

    pub fn plan(&self) -> &DeletePlan {
        &self.plan
    }
}

/// Builds the delete plan for the selected nodes, with the default preview cap.
/// `base` is the directory from [`crate::tree_base`].
pub fn build_delete_plan(
    tree: &FileTree,
    roots: &[NodeId],
    base: &Path,
) -> Result<DeletePlan, PlanError> {
    build_delete_plan_capped(tree, roots, base, DEFAULT_PREVIEW_CAP)
}

/// Like [`build_delete_plan`], with an explicit cap on listed paths.
pub fn build_delete_plan_capped(
    tree: &FileTree,
    roots: &[NodeId],
    base: &Path,
    cap: usize,
) -> Result<DeletePlan, PlanError> {
    let normalized = normalize_roots(tree, roots)?;
    let mut items = Vec::new();
    let mut groups = Vec::with_capacity(normalized.len());
    for root in &normalized {
        let start = items.len();
        items.extend(collect_items(tree, &[*root], base)?);
        groups.push(start..items.len());
    }
    let preview = summarize(tree, &items, cap);
    let confirm_phrase = match normalized.as_slice() {
        [single] => tree
            .node(*single)
            .map(|node| node.name().to_string_lossy().into_owned())
            .ok_or(PlanError::Detached(*single))?,
        many => many.len().to_string(),
    };
    Ok(DeletePlan {
        items,
        groups,
        preview,
        confirm_phrase,
    })
}
