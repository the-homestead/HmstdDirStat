//! Delete UI state and background-job orchestration. Dialog rendering is split by step;
//! pure wording and gates live in `view_model` and have unit tests.

mod common;
mod confirm;
mod preview;
mod progress;
mod rename;
mod rename_view;
mod result;
#[cfg(test)]
mod tests;
mod transfer;
#[cfg(test)]
mod transfer_tests;
mod transfer_view;
mod view_model;
pub use rename::RenameUi;
pub(in crate::shell) use transfer::TransferKindUi;
pub use transfer::TransferUi;

/// Creates the text inputs of whichever dialog is open. Inputs need a window, so this runs
/// during render.
pub(super) fn ensure_inputs(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    ensure_input(shell, window, cx);
    transfer::ensure_inputs(shell, window, cx);
    rename::ensure_inputs(shell, window, cx);
}

/// The open file-operation dialog, if any: delete, copy/move or rename.
pub(super) fn overlays(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    overlay(shell, p, handle.clone())
        .or_else(|| transfer_view::overlay(shell, p, handle.clone()))
        .or_else(|| rename_view::overlay(shell, p, handle))
}

use std::sync::Arc;

use gpui_kit::{
    component::{
        input::{InputEvent, InputState},
        tag::Tag,
        *,
    },
    *,
};
use timberstat_fileops::{
    Control, DeleteMode, DeletePlan, DeleteReport, apply_delete, build_delete_plan, execute_delete,
    tree_base,
};
use timberstat_filesystem::NodeId;

use super::{PROGRESS_REFRESH, Phase, Shell, widgets::Palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeleteStage {
    Closed,
    Planning,
    Preview,
    Confirm,
    Running,
    Result,
    Error,
}

/// All delete-specific view state. Its task is retained here for the whole operation.
pub(super) struct DeleteUi {
    stage: DeleteStage,
    mode: DeleteMode,
    plan: Option<DeletePlan>,
    control: Option<Arc<Control>>,
    report: Option<Arc<DeleteReport>>,
    error: Option<String>,
    typed: String,
    input: Option<Entity<InputState>>,
    _input_sub: Option<Subscription>,
    task: Option<Task<()>>,
    detached: usize,
    generation: u64,
}

impl Default for DeleteUi {
    fn default() -> Self {
        Self {
            stage: DeleteStage::Closed,
            mode: DeleteMode::RecycleBin,
            plan: None,
            control: None,
            report: None,
            error: None,
            typed: String::new(),
            input: None,
            _input_sub: None,
            task: None,
            detached: 0,
            generation: 0,
        }
    }
}

impl DeleteUi {
    pub(super) fn is_closed(&self) -> bool {
        self.stage == DeleteStage::Closed
    }

    fn begin(&mut self, mode: DeleteMode) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.stage = DeleteStage::Planning;
        self.mode = mode;
        self.plan = None;
        self.control = None;
        self.report = None;
        self.error = None;
        self.typed.clear();
        self.detached = 0;
        self.clear_input();
        self.task = None;
        self.generation
    }

    fn clear_input(&mut self) {
        self.input = None;
        self._input_sub = None;
    }
}

/// Creates the typed-confirmation input only when its step is visible.
fn ensure_input(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    if shell.delete.stage != DeleteStage::Confirm || shell.delete.input.is_some() {
        return;
    }
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type the confirmation text"));
    let subscription = cx.subscribe_in(
        &input,
        window,
        |shell: &mut Shell, input, event: &InputEvent, _window, cx| {
            if matches!(event, InputEvent::Change) {
                shell.delete.typed = input.read(cx).value().to_string();
                cx.notify();
            }
        },
    );
    shell.delete.input = Some(input);
    shell.delete._input_sub = Some(subscription);
}

impl Shell {
    /// Starts a memory-only plan on a worker. No file operation can start before the preview.
    pub(super) fn begin_delete(&mut self, node: NodeId, mode: DeleteMode, cx: &mut Context<Self>) {
        if !self.delete.is_closed() || self.is_scanning() {
            return;
        }
        let (Some(files), Some(scan_root)) = (self.tree.clone(), self.scan_root.clone()) else {
            return;
        };
        self.menu = None;
        let generation = self.delete.begin(mode);
        let job = cx.background_executor().spawn(async move {
            let base = tree_base(&scan_root);
            build_delete_plan(&files, &[node], &base)
        });
        self.delete.task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |shell, cx| {
                if shell.delete.generation != generation
                    || shell.delete.stage != DeleteStage::Planning
                {
                    return;
                }
                match outcome {
                    Ok(plan) => {
                        shell.delete.plan = Some(plan);
                        shell.delete.stage = DeleteStage::Preview;
                    }
                    Err(error) => {
                        shell.delete.error = Some(error.to_string());
                        shell.delete.stage = DeleteStage::Error;
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Confirms the preview. Recycle Bin runs now; permanent delete advances to typing.
    pub(super) fn review_delete(&mut self, cx: &mut Context<Self>) {
        if self.delete.stage != DeleteStage::Preview {
            return;
        }
        if self.delete.mode == DeleteMode::Permanent {
            self.delete.stage = DeleteStage::Confirm;
            self.delete.typed.clear();
            self.delete.error = None;
            self.delete.clear_input();
            cx.notify();
        } else {
            self.confirm_delete(cx);
        }
    }

    pub(super) fn back_to_delete_preview(&mut self, cx: &mut Context<Self>) {
        if self.delete.stage == DeleteStage::Confirm {
            self.delete.stage = DeleteStage::Preview;
            self.delete.typed.clear();
            self.delete.clear_input();
            cx.notify();
        }
    }

    /// Converts the visible plan into the engine's confirmed type, then executes off-thread.
    pub(super) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let expected = if self.delete.mode == DeleteMode::Permanent {
            DeleteStage::Confirm
        } else {
            DeleteStage::Preview
        };
        if self.delete.stage != expected {
            return;
        }
        if self.delete.mode == DeleteMode::Permanent
            && self.delete.plan.as_ref().is_none_or(|plan| {
                !view_model::confirmation_matches(&self.delete.typed, plan.confirm_phrase())
            })
        {
            return;
        }
        let Some(plan) = self.delete.plan.take() else {
            return;
        };
        let confirmed = match self.delete.mode {
            DeleteMode::RecycleBin => plan.confirm_recycle_bin(),
            DeleteMode::Permanent => match plan.confirm_permanent(&self.delete.typed) {
                Ok(confirmed) => confirmed,
                Err(error) => {
                    self.delete.error = Some(error.to_string());
                    self.delete.stage = DeleteStage::Error;
                    cx.notify();
                    return;
                }
            },
        };

        let control = Control::new();
        control.set_total(confirmed.plan().preview().entry_count());
        let worker_control = control.clone();
        self.delete.control = Some(control);
        self.delete.stage = DeleteStage::Running;
        self.delete.error = None;
        self.delete.typed.clear();
        self.delete.clear_input();
        let generation = self.delete.generation;
        let job = cx
            .background_executor()
            .spawn(async move { execute_delete(confirmed, worker_control.as_ref()) });
        self.delete.task = Some(cx.spawn(async move |this, cx| {
            let report = job.await;
            let _ = this.update(cx, |shell, cx| {
                if shell.delete.generation == generation
                    && shell.delete.stage == DeleteStage::Running
                {
                    shell.finish_delete(report, cx);
                }
            });
        }));
        self.spawn_delete_progress(generation, cx);
        cx.notify();
    }

    fn spawn_delete_progress(&self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REFRESH).await;
                let running = this
                    .update(cx, |shell, cx| {
                        let running = shell.delete.generation == generation
                            && shell.delete.stage == DeleteStage::Running;
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

    /// Applies only the worker's report to memory, then rebuilds view state. No rescan or I/O.
    fn finish_delete(&mut self, report: DeleteReport, cx: &mut Context<Self>) {
        let mut detached = 0;
        if let Some(files) = self.tree.as_mut() {
            detached = apply_delete(Arc::make_mut(files), &report);
        }
        if self
            .tree
            .as_deref()
            .is_some_and(|files| !files.is_attached(self.focus))
        {
            self.focus = NodeId::ROOT;
        }
        if self.selected.is_some_and(|selected| {
            self.tree
                .as_deref()
                .is_some_and(|files| !files.is_attached(selected))
        }) {
            self.selected = None;
        }
        self.query = self.requery(self.query.text());
        self.refresh_tree_state(cx);
        self.refresh_extensions();
        self.sunburst_hover = None;
        self.account_for_delete(&report);
        self.delete.detached = detached;
        self.delete.report = Some(Arc::new(report));
        self.delete.control = None;
        self.delete.stage = DeleteStage::Result;
        cx.notify();
    }

    /// Keeps the existing scan panels in step with the edited tree without another scan.
    fn account_for_delete(&mut self, report: &DeleteReport) {
        let removed = report.removed.len() as u64;
        match &mut self.phase {
            Phase::Walked(summary) | Phase::Reopened(summary, _) => {
                summary.files = summary.files.saturating_sub(report.removed_files);
                summary.directories = summary
                    .directories
                    .saturating_sub(report.removed_directories);
                summary.logical_bytes =
                    summary.logical_bytes.saturating_sub(report.removed_logical);
                summary.physical_bytes = summary
                    .physical_bytes
                    .map(|bytes| bytes.saturating_sub(report.removed_physical));
                summary.entries = summary.entries.saturating_sub(removed);
            }
            Phase::FastDone(summary) => {
                summary.files = summary.files.saturating_sub(report.removed_files);
                summary.directories = summary
                    .directories
                    .saturating_sub(report.removed_directories);
                summary.logical = summary.logical.saturating_sub(report.removed_logical);
                summary.physical = summary.physical.saturating_sub(report.removed_physical);
            }
            _ => {}
        }
    }

    pub(super) fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        if self.delete.stage == DeleteStage::Running
            && let Some(control) = self.delete.control.as_ref()
        {
            control.cancel();
        }
        cx.notify();
    }

    /// Closing is blocked during execution; the running panel offers safe cancellation instead.
    pub(super) fn close_delete(&mut self, cx: &mut Context<Self>) {
        if self.delete.stage == DeleteStage::Running {
            return;
        }
        self.delete.generation = self.delete.generation.wrapping_add(1);
        self.delete.stage = DeleteStage::Closed;
        self.delete.plan = None;
        self.delete.control = None;
        self.delete.report = None;
        self.delete.error = None;
        self.delete.typed.clear();
        self.delete.clear_input();
        self.delete.task = None;
        cx.notify();
    }
}

fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    if shell.delete.is_closed() {
        return None;
    }
    let (title, tag, body) = match shell.delete.stage {
        DeleteStage::Closed => return None,
        DeleteStage::Planning => (
            "Delete preview",
            Tag::secondary().child("Planning"),
            progress::planning(p, handle),
        ),
        DeleteStage::Preview => (
            "Delete preview",
            mode_tag(shell.delete.mode),
            preview::body(shell, p, handle),
        ),
        DeleteStage::Confirm => (
            "Confirm permanent delete",
            Tag::danger().child("Permanent"),
            confirm::body(shell, p, handle),
        ),
        DeleteStage::Running => (
            "Delete in progress",
            mode_tag(shell.delete.mode),
            progress::running(shell, p, handle),
        ),
        DeleteStage::Result => {
            let report = shell.delete.report.as_ref();
            let tag = if report.is_some_and(|report| report.cancelled) {
                Tag::warning().child("Cancelled")
            } else if report.is_some_and(|report| report.is_complete()) {
                Tag::success().child("Complete")
            } else {
                Tag::warning().child("Partial")
            };
            ("Delete result", tag, result::body(shell, p, handle))
        }
        DeleteStage::Error => (
            "Delete cannot start",
            Tag::danger().child("Stopped"),
            result::plan_error(shell, p, handle),
        ),
    };
    Some(dialog_frame(title, tag, body, p))
}

fn mode_tag(mode: DeleteMode) -> Tag {
    if mode == DeleteMode::Permanent {
        Tag::danger().child("Permanent")
    } else {
        Tag::primary().child(crate::platform::TRASH_NAME)
    }
}

/// A modal surface that never dismisses on an outside click. Destructive steps need an
/// explicit button, and a running delete can only be cancelled through its Cancel button.
pub(super) fn dialog_frame(title: &str, tag: Tag, body: Div, p: Palette) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(hsla(0.0, 0.0, 0.0, 0.5))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .v_flex()
                .w(px(720.))
                .max_h(px(760.))
                .overflow_hidden()
                .p_4()
                .gap_3()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .text_color(p.foreground)
                .text_size(px(13.))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title.to_string()),
                        )
                        .child(tag),
                )
                .child(body),
        )
}
