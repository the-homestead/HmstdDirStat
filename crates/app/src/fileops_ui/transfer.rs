//! Copy and move: dialog state and background orchestration.
//!
//! Flow: edit -> plan (worker) -> preview -> [replace confirmation, only for Overwrite
//! over a taken name] -> run (worker, cancellable) -> result. The UI thread never touches
//! the disk. Moved sources leave the in-memory tree. Copies are not added to it until the
//! next scan.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::component::input::InputState;
use gpui_kit::*;
use hmstd_fileops::{
    Collision, Control, TransferKind, TransferPlan, TransferReport, apply_moves, execute_transfer,
    plan_transfer, tree_base, validate_name,
};
use hmstd_filesystem::NodeId;

use super::common::text_input;
use crate::shell::{PROGRESS_REFRESH, Shell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransferStage {
    Closed,
    Editing,
    Planning,
    Preview,
    OverwriteConfirm,
    Running,
    Result,
    Error,
}

/// Which operation the dialog performs. Crate-visible because the context menu opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::shell) enum TransferKindUi {
    Copy,
    Move,
}

impl TransferKindUi {
    pub(super) fn engine(self) -> TransferKind {
        match self {
            Self::Copy => TransferKind::Copy,
            Self::Move => TransferKind::Move,
        }
    }

    pub(super) fn verb(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
        }
    }
}

/// Copy/move dialog state. Fields are read by `transfer_view`.
pub struct TransferUi {
    pub(super) stage: TransferStage,
    pub(super) kind: TransferKindUi,
    pub(super) node: Option<NodeId>,
    pub(super) source: String,
    pub(super) dest_text: String,
    pub(super) dest_error: Option<String>,
    pub(super) dest_input: Option<Entity<InputState>>,
    _dest_sub: Option<Subscription>,
    pub(super) collision: Collision,
    pub(super) name_text: String,
    pub(super) name_error: Option<String>,
    pub(super) name_input: Option<Entity<InputState>>,
    _name_sub: Option<Subscription>,
    pub(super) plan: Option<TransferPlan>,
    /// True when the destination already holds an item with the target name.
    pub(super) target_taken: bool,
    pub(super) control: Option<Arc<Control>>,
    pub(super) report: Option<Arc<TransferReport>>,
    pub(super) error: Option<String>,
    pub(super) generation: u64,
    _task: Option<Task<()>>,
}

impl Default for TransferUi {
    fn default() -> Self {
        Self {
            stage: TransferStage::Closed,
            kind: TransferKindUi::Copy,
            node: None,
            source: String::new(),
            dest_text: String::new(),
            dest_error: None,
            dest_input: None,
            _dest_sub: None,
            collision: Collision::Skip,
            name_text: String::new(),
            name_error: None,
            name_input: None,
            _name_sub: None,
            plan: None,
            target_taken: false,
            control: None,
            report: None,
            error: None,
            generation: 0,
            _task: None,
        }
    }
}

impl TransferUi {
    pub(in crate::shell) fn is_closed(&self) -> bool {
        self.stage == TransferStage::Closed
    }

    /// Drops every value except the stage and generation. Dropping `_task` cancels any
    /// planning job still in flight.
    fn clear(&mut self) {
        self.dest_text.clear();
        self.dest_error = None;
        self.dest_input = None;
        self._dest_sub = None;
        self.collision = Collision::Skip;
        self.name_text.clear();
        self.name_error = None;
        self.name_input = None;
        self._name_sub = None;
        self.plan = None;
        self.target_taken = false;
        self.control = None;
        self.report = None;
        self.error = None;
        self._task = None;
    }

    fn begin(&mut self, kind: TransferKindUi, node: NodeId, source: String, name: String) {
        self.generation = self.generation.wrapping_add(1);
        self.clear();
        self.stage = TransferStage::Editing;
        self.kind = kind;
        self.node = Some(node);
        self.source = source;
        self.name_text = name;
    }

    fn close(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.clear();
        self.stage = TransferStage::Closed;
        self.node = None;
        self.source.clear();
    }
}

/// True when the destination already holds an item with the name the operation would use.
pub(super) fn target_taken(plan: &TransferPlan, rename: Option<&str>) -> bool {
    let Some(item) = plan.items().first() else {
        return false;
    };
    let name = match rename {
        Some(name) => OsString::from(name),
        None => match item.path.file_name() {
            Some(name) => name.to_os_string(),
            None => return false,
        },
    };
    fs::symlink_metadata(plan.dest_dir().join(name)).is_ok()
}

/// Creates the destination and name inputs when the edit step opens. Inputs are created
/// during render because they need a window.
pub(super) fn ensure_inputs(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    if shell.transfer.stage != TransferStage::Editing || shell.transfer.dest_input.is_some() {
        return;
    }
    let (dest, dest_sub) = text_input(
        window,
        cx,
        "Full path of the destination folder",
        String::new(),
        |s, text| {
            s.transfer.dest_text = text;
            s.transfer.dest_error = None;
        },
    );
    let initial = shell.transfer.name_text.clone();
    let (name, name_sub) = text_input(window, cx, "New name", initial, |s, text| {
        s.transfer.name_text = text;
        s.transfer.name_error = None;
    });
    shell.transfer.dest_input = Some(dest);
    shell.transfer._dest_sub = Some(dest_sub);
    shell.transfer.name_input = Some(name);
    shell.transfer._name_sub = Some(name_sub);
}

impl Shell {
    /// Opens the copy or move dialog for a node from the context menu.
    pub(in crate::shell) fn begin_transfer(
        &mut self,
        node: NodeId,
        kind: TransferKindUi,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() || node == NodeId::ROOT {
            return;
        }
        let Some(name) = self
            .tree
            .as_deref()
            .and_then(|files| files.node(node))
            .map(|entry| entry.name().to_string_lossy().into_owned())
        else {
            return;
        };
        let Some(source) = self.path_of_node(node) else {
            return;
        };
        self.menu = None;
        self.transfer
            .begin(kind, node, source.display().to_string(), name);
        cx.notify();
    }

    /// Validates the typed destination and name, then plans on a worker. Nothing is
    /// changed by planning; it only reads the tree and checks the destination.
    pub(super) fn preview_transfer(&mut self, cx: &mut Context<Self>) {
        if self.transfer.stage != TransferStage::Editing {
            return;
        }
        let dest = self.transfer.dest_text.trim().to_owned();
        if dest.is_empty() {
            self.transfer.dest_error = Some("Choose a destination folder.".into());
            cx.notify();
            return;
        }
        if !Path::new(&dest).is_absolute() {
            self.transfer.dest_error =
                Some("Enter a full path, such as C:\\Folders\\Target.".into());
            cx.notify();
            return;
        }
        let collision = self.transfer.collision;
        let rename =
            (collision == Collision::Rename).then(|| self.transfer.name_text.trim().to_owned());
        if let Some(name) = rename.as_deref()
            && let Err(error) = validate_name(name)
        {
            self.transfer.name_error = Some(error.to_string());
            cx.notify();
            return;
        }
        let (Some(files), Some(scan_root), Some(node)) = (
            self.tree.clone(),
            self.scan_root.clone(),
            self.transfer.node,
        ) else {
            return;
        };
        let kind = self.transfer.kind.engine();
        let generation = self.transfer.generation;
        self.transfer.dest_error = None;
        self.transfer.name_error = None;
        self.transfer.stage = TransferStage::Planning;
        let dest_path = PathBuf::from(&dest);
        let job = cx.background_executor().spawn(async move {
            let base = tree_base(&scan_root);
            let mut plan = plan_transfer(&files, &[node], &base, &dest_path, kind)
                .map_err(|error| error.to_string())?;
            if let Some(name) = rename.as_deref() {
                plan.set_name(0, name).map_err(|error| error.to_string())?;
            }
            let taken = target_taken(&plan, rename.as_deref());
            Ok::<(TransferPlan, bool), String>((plan, taken))
        });
        self.transfer._task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |shell, cx| {
                if shell.transfer.generation != generation
                    || shell.transfer.stage != TransferStage::Planning
                {
                    return;
                }
                match outcome {
                    Ok((plan, taken)) => {
                        shell.transfer.plan = Some(plan);
                        shell.transfer.target_taken = taken;
                        shell.transfer.stage = if collision == Collision::Overwrite && taken {
                            TransferStage::OverwriteConfirm
                        } else {
                            TransferStage::Preview
                        };
                    }
                    Err(error) => {
                        shell.transfer.error = Some(error);
                        shell.transfer.stage = TransferStage::Error;
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Runs the planned transfer on a worker. Allowed from the preview, and from the
    /// replace confirmation (which is the user's explicit approval to overwrite).
    pub(super) fn run_transfer(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.transfer.stage,
            TransferStage::Preview | TransferStage::OverwriteConfirm
        ) {
            return;
        }
        let Some(plan) = self.transfer.plan.take() else {
            return;
        };
        let collision = self.transfer.collision;
        let control = Control::new();
        let worker = control.clone();
        self.transfer.control = Some(control);
        self.transfer.stage = TransferStage::Running;
        self.transfer.error = None;
        let generation = self.transfer.generation;
        let job = cx
            .background_executor()
            .spawn(async move { execute_transfer(plan, collision, worker.as_ref()) });
        self.transfer._task = Some(cx.spawn(async move |this, cx| {
            let report = job.await;
            let _ = this.update(cx, |shell, cx| {
                if shell.transfer.generation == generation
                    && shell.transfer.stage == TransferStage::Running
                {
                    shell.finish_transfer(report, cx);
                }
            });
        }));
        self.spawn_transfer_progress(generation, cx);
        cx.notify();
    }

    fn spawn_transfer_progress(&self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REFRESH).await;
                let running = this
                    .update(cx, |shell, cx| {
                        let running = shell.transfer.generation == generation
                            && shell.transfer.stage == TransferStage::Running;
                        if running {
                            cx.notify();
                        }
                        running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
    }

    /// Applies only the report to memory: moved sources leave the tree. No rescan.
    fn finish_transfer(&mut self, report: TransferReport, cx: &mut Context<Self>) {
        if let Some(files) = self.tree.as_mut() {
            apply_moves(Arc::make_mut(files), &report);
        }
        self.refresh_after_tree_edit(cx);
        self.transfer.control = None;
        self.transfer.report = Some(Arc::new(report));
        self.transfer.stage = TransferStage::Result;
        cx.notify();
    }

    pub(super) fn cancel_transfer(&mut self, cx: &mut Context<Self>) {
        if self.transfer.stage == TransferStage::Running
            && let Some(control) = self.transfer.control.as_ref()
        {
            control.cancel();
        }
        cx.notify();
    }

    /// Closing is blocked while running; the progress step offers cancel instead.
    pub(super) fn close_transfer(&mut self, cx: &mut Context<Self>) {
        if self.transfer.stage == TransferStage::Running {
            return;
        }
        self.transfer.close();
        cx.notify();
    }

    /// Returns from the preview or replace confirmation to the edit step.
    pub(super) fn back_to_transfer_edit(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.transfer.stage,
            TransferStage::Preview | TransferStage::OverwriteConfirm
        ) {
            self.transfer.stage = TransferStage::Editing;
            self.transfer.plan = None;
            self.transfer.target_taken = false;
            cx.notify();
        }
    }

    pub(super) fn set_transfer_collision(&mut self, policy: Collision, cx: &mut Context<Self>) {
        if self.transfer.stage == TransferStage::Editing {
            self.transfer.collision = policy;
            self.transfer.name_error = None;
            cx.notify();
        }
    }
}
