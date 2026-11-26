//! Copy and move plans: sources, destination, and per-item name overrides.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use timberstat_filesystem::{FileTree, NodeId};

use crate::model::{PlanError, Preview, WorkItem};
use crate::preview::{DEFAULT_PREVIEW_CAP, collect_items, normalize_roots, summarize};
use crate::rename::validate_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    Copy,
    Move,
}

/// What to do when the destination name is already taken. There is no silent
/// default: the caller must pass one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// Leave the existing destination and the source as they are.
    Skip,
    /// Replace the existing file, or merge into an existing folder.
    /// Folder-versus-file clashes are never replaced.
    Overwrite,
    /// Use the name the user typed for that item (see [`TransferPlan::set_name`]).
    /// The item fails if that name is taken too.
    Rename,
    /// Pick the first free `name (2)` style name.
    KeepBoth,
}

/// The selected top-level entries and where they go.
#[derive(Debug, Clone)]
pub struct TransferPlan {
    kind: TransferKind,
    dest_dir: PathBuf,
    items: Vec<WorkItem>,
    names: Vec<Option<OsString>>,
    preview: Preview,
}

impl TransferPlan {
    pub fn kind(&self) -> TransferKind {
        self.kind
    }

    pub fn dest_dir(&self) -> &Path {
        &self.dest_dir
    }

    /// The selected top-level entries, in selection order.
    pub fn items(&self) -> &[WorkItem] {
        &self.items
    }

    pub fn preview(&self) -> &Preview {
        &self.preview
    }

    /// Sets the destination name for one item, used by [`Collision::Rename`].
    pub fn set_name(&mut self, index: usize, name: &str) -> Result<(), PlanError> {
        validate_name(name)?;
        let slot = self
            .names
            .get_mut(index)
            .ok_or_else(|| PlanError::BadName(name.to_owned(), "no such item"))?;
        *slot = Some(OsString::from(name));
        Ok(())
    }

    pub(crate) fn into_parts(
        self,
    ) -> (TransferKind, PathBuf, Vec<WorkItem>, Vec<Option<OsString>>) {
        (self.kind, self.dest_dir, self.items, self.names)
    }
}

/// Plans a copy or move of the selected nodes into `dest_dir`.
///
/// `base` is the directory from [`crate::tree_base`]. Refuses a destination
/// inside a source, which would copy a folder into itself.
pub fn plan_transfer(
    tree: &FileTree,
    roots: &[NodeId],
    base: &Path,
    dest_dir: &Path,
    kind: TransferKind,
) -> Result<TransferPlan, PlanError> {
    let normalized = normalize_roots(tree, roots)?;
    let dest_meta = std::fs::metadata(dest_dir)
        .map_err(|error| PlanError::Resolve(dest_dir.to_path_buf(), error.to_string()))?;
    if !dest_meta.is_dir() {
        return Err(PlanError::DestinationNotDirectory(dest_dir.to_path_buf()));
    }
    let dest_canonical = std::fs::canonicalize(dest_dir)
        .map_err(|error| PlanError::Resolve(dest_dir.to_path_buf(), error.to_string()))?;

    let mut items = Vec::with_capacity(normalized.len());
    for root in &normalized {
        let top = collect_items(tree, &[*root], base)?;
        let Some(item) = top.into_iter().last() else {
            return Err(PlanError::Detached(*root));
        };
        let source_canonical = std::fs::canonicalize(&item.path)
            .map_err(|error| PlanError::Resolve(item.path.clone(), error.to_string()))?;
        if dest_canonical.starts_with(&source_canonical) {
            return Err(PlanError::IntoItself {
                source_path: item.path.clone(),
                dest: dest_dir.to_path_buf(),
            });
        }
        items.push(item);
    }

    let everything = collect_items(tree, &normalized, base)?;
    let preview = summarize(tree, &everything, DEFAULT_PREVIEW_CAP);
    let names = vec![None; items.len()];
    Ok(TransferPlan {
        kind,
        dest_dir: dest_dir.to_path_buf(),
        items,
        names,
        preview,
    })
}
