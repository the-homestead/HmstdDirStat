//! Rename one entry in place: dialog state and background orchestration.
//!
//! Overwrite replaces an existing file only after the user confirms. Folders are never
//! replaced. The rename itself runs on a worker, and the in-memory tree is updated from
//! the result, with no rescan.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::component::input::InputState;
use gpui_kit::*;
use timberstat_fileops::{
    Collision, Failure, RenameOutcome, apply_rename, rename_path, validate_name,
};
use timberstat_filesystem::NodeId;

use super::common::text_input;
use crate::shell::Shell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenameStage {
    Closed,
    Editing,
    Checking,
    OverwriteConfirm,
    Running,
    Result,
}

/// What one rename job produced: a request to confirm a replace, or the final outcome.
pub(super) enum RenameStep {
    Confirm(PathBuf),
    Done(Result<RenameOutcome, Failure>),
}

/// Worker-side rename step. With Overwrite and a taken name, it asks for confirmation
/// first and changes nothing. `confirmed` is true only after the user pressed Replace.
pub(super) fn rename_step(
    path: &Path,
    new_name: &str,
    old_name: &str,
    collision: Collision,
    confirmed: bool,
) -> RenameStep {
    if !confirmed
        && collision == Collision::Overwrite
        && new_name != old_name
        && let Some(target) = path.parent().map(|parent| parent.join(new_name))
        && fs::symlink_metadata(&target).is_ok()
    {
        return RenameStep::Confirm(target);
    }
    RenameStep::Done(rename_path(path, new_name, collision))
}

/// Rename dialog state. Fields are read by `rename_view`.
pub struct RenameUi {
    pub(super) stage: RenameStage,
    pub(super) node: Option<NodeId>,
    pub(super) path: Option<PathBuf>,
    pub(super) old_name: String,
    pub(super) name_text: String,
    pub(super) name_error: Option<String>,
    pub(super) name_input: Option<Entity<InputState>>,
    _name_sub: Option<Subscription>,
    pub(super) collision: Collision,
    pub(super) target: Option<PathBuf>,
    pub(super) outcome: Option<Result<RenameOutcome, Failure>>,
    pub(super) generation: u64,
    _task: Option<Task<()>>,
}

impl Default for RenameUi {
    fn default() -> Self {
        Self {
            stage: RenameStage::Closed,
            node: None,
            path: None,
            old_name: String::new(),
            name_text: String::new(),
            name_error: None,
            name_input: None,
            _name_sub: None,
            collision: Collision::Skip,
            target: None,
            outcome: None,
            generation: 0,
            _task: None,
        }
    }
}

impl RenameUi {
    pub(in crate::shell) fn is_closed(&self) -> bool {
        self.stage == RenameStage::Closed
    }

    /// Drops every value except stage and generation. Dropping `_task` cancels a pending
    /// check; a running rename is never cancelled, because close is blocked while running.
    fn clear(&mut self) {
        self.node = None;
        self.path = None;
        self.old_name.clear();
        self.name_text.clear();
        self.name_error = None;
        self.name_input = None;
        self._name_sub = None;
        self.collision = Collision::Skip;
        self.target = None;
        self.outcome = None;
        self._task = None;
    }
}

/// Creates the name input when the edit step opens, during render (it needs a window).
pub(super) fn ensure_inputs(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    if shell.rename.stage != RenameStage::Editing || shell.rename.name_input.is_some() {
        return;
    }
    let initial = shell.rename.name_text.clone();
    let (input, subscription) = text_input(window, cx, "New name", initial, |s, text| {
        s.rename.name_text = text;
        s.rename.name_error = None;
    });
    shell.rename.name_input = Some(input);
    shell.rename._name_sub = Some(subscription);
}

impl Shell {
    /// Opens the rename dialog for a node. The scan root cannot be renamed here.
    pub(in crate::shell) fn begin_rename(&mut self, node: NodeId, cx: &mut Context<Self>) {
        if self.is_busy() || node == NodeId::ROOT {
            return;
        }
        let Some(old) = self
            .tree
            .as_deref()
            .and_then(|files| files.node(node))
            .map(|entry| entry.name().to_string_lossy().into_owned())
        else {
            return;
        };
        let Some(path) = self.path_of_node(node) else {
            return;
        };
        self.menu = None;
        self.rename.generation = self.rename.generation.wrapping_add(1);
        self.rename.clear();
        self.rename.stage = RenameStage::Editing;
        self.rename.node = Some(node);
        self.rename.path = Some(path);
        self.rename.name_text = old.clone();
        self.rename.old_name = old;
        cx.notify();
    }

    /// Validates the new name, then runs the rename on a worker (or asks to replace).
    pub(super) fn submit_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename.stage != RenameStage::Editing {
            return;
        }
        let name = self.rename.name_text.trim().to_owned();
        if let Err(error) = validate_name(&name) {
            self.rename.name_error = Some(error.to_string());
            cx.notify();
            return;
        }
        let Some(path) = self.rename.path.clone() else {
            return;
        };
        let old = self.rename.old_name.clone();
        let collision = self.rename.collision;
        self.rename.name_error = None;
        self.rename.stage = RenameStage::Checking;
        let generation = self.rename.generation;
        let job = cx
            .background_executor()
            .spawn(async move { rename_step(&path, &name, &old, collision, false) });
        self.await_rename_step(generation, job, cx);
        cx.notify();
    }

    /// The user pressed Replace on the confirmation. Only Overwrite reaches this step.
    pub(super) fn replace_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename.stage != RenameStage::OverwriteConfirm {
            return;
        }
        let (Some(path), name, old) = (
            self.rename.path.clone(),
            self.rename.name_text.trim().to_owned(),
            self.rename.old_name.clone(),
        ) else {
            return;
        };
        self.rename.stage = RenameStage::Running;
        let generation = self.rename.generation;
        let job = cx
            .background_executor()
            .spawn(async move { rename_step(&path, &name, &old, Collision::Overwrite, true) });
        self.await_rename_step(generation, job, cx);
        cx.notify();
    }

    fn await_rename_step(
        &mut self,
        generation: u64,
        job: Task<RenameStep>,
        cx: &mut Context<Self>,
    ) {
        self.rename._task = Some(cx.spawn(async move |this, cx| {
            let step = job.await;
            let _ = this.update(cx, |shell, cx| {
                if shell.rename.generation != generation {
                    return;
                }
                match step {
                    RenameStep::Confirm(target) => {
                        shell.rename.target = Some(target);
                        shell.rename.stage = RenameStage::OverwriteConfirm;
                    }
                    RenameStep::Done(result) => shell.finish_rename(result, cx),
                }
                cx.notify();
            });
        }));
    }

    /// Updates memory only: the node takes the new name when the file was renamed.
    fn finish_rename(&mut self, result: Result<RenameOutcome, Failure>, cx: &mut Context<Self>) {
        if let Ok(RenameOutcome::Renamed { to }) = &result
            && let Some(new_name) = to.file_name()
            && let Some(node) = self.rename.node
            && let Some(files) = self.tree.as_mut()
        {
            apply_rename(Arc::make_mut(files), node, new_name);
        }
        self.refresh_after_tree_edit(cx);
        self.rename.target = None;
        self.rename.outcome = Some(result);
        self.rename.stage = RenameStage::Result;
        cx.notify();
    }

    /// Closing is blocked while a check or rename is in flight.
    pub(super) fn close_rename(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.rename.stage,
            RenameStage::Checking | RenameStage::Running
        ) {
            return;
        }
        self.rename.generation = self.rename.generation.wrapping_add(1);
        self.rename.clear();
        self.rename.stage = RenameStage::Closed;
        cx.notify();
    }

    pub(super) fn back_to_rename_edit(&mut self, cx: &mut Context<Self>) {
        if self.rename.stage == RenameStage::OverwriteConfirm {
            self.rename.stage = RenameStage::Editing;
            self.rename.target = None;
            cx.notify();
        }
    }

    pub(super) fn set_rename_collision(&mut self, policy: Collision, cx: &mut Context<Self>) {
        if self.rename.stage == RenameStage::Editing {
            self.rename.collision = policy;
            cx.notify();
        }
    }
}
