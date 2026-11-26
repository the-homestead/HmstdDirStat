//! Saved-workspace state and Shell actions. The domain operations live in timberstat-core; this
//! module applies successful operations to the visible layout and persists them off the UI thread.

use std::path::PathBuf;

use gpui_kit::{component::input::InputState, *};
use timberstat_core::workspace::{
    WorkspaceLayout,
    saved::{WorkspaceDocument, WorkspaceStoreError, default_workspaces_path, load_from, save_to},
};

use super::Shell;

pub(super) struct WorkspaceUi {
    document: WorkspaceDocument,
    path: Option<PathBuf>,
    /// A rejected existing file makes this manager read-only for the run. In particular, a later
    /// layout drag cannot overwrite the rejected file through the automatic current-layout save.
    load_failed: bool,
    error: Option<String>,
    notice: Option<String>,
    name_input: Option<Entity<InputState>>,
}

impl WorkspaceUi {
    /// Restores the saved document, falling back to the layout from config.json only when no
    /// workspaces.json exists. No file is written by startup.
    pub(super) fn restore(fallback: WorkspaceLayout) -> (Self, WorkspaceLayout) {
        let mut state = Self {
            document: WorkspaceDocument::new(fallback.clone()),
            path: None,
            load_failed: false,
            error: None,
            notice: None,
            name_input: None,
        };
        match default_workspaces_path() {
            Ok(path) => {
                state.path = Some(path.clone());
                match load_from(&path, fallback) {
                    Ok(document) => state.document = document,
                    Err(error) => {
                        tracing::error!(%error, "saved workspaces rejected; file left unchanged");
                        state.load_failed = true;
                        state.error = Some(format!(
                            "{error}. The file was left unchanged; workspace saving is disabled for this run."
                        ));
                    }
                }
            }
            Err(error) => {
                state.load_failed = true;
                state.error = Some(format!(
                    "No workspace configuration path is available: {error}. Workspace saving is disabled."
                ));
            }
        }
        let layout = state.document.layout.clone();
        (state, layout)
    }

    pub(super) fn document(&self) -> &WorkspaceDocument {
        &self.document
    }

    pub(super) fn name_input(&self) -> Option<&Entity<InputState>> {
        self.name_input.as_ref()
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub(super) fn can_write(&self) -> bool {
        !self.load_failed && self.path.is_some()
    }

    fn input_value(&self, cx: &App) -> String {
        self.name_input
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    fn current_changed(&mut self, layout: WorkspaceLayout) {
        self.document.layout = layout;
        self.notice = None;
    }

    fn edit_failed(&mut self, error: impl std::fmt::Display) {
        self.notice = None;
        self.error = Some(error.to_string());
    }
}

/// Creates the name field when the View menu first opens. The field is reused for Save as and
/// Rename; button handlers read its current value directly.
pub(super) fn ensure_input(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    if !shell.view_menu || shell.workspaces.name_input.is_some() {
        return;
    }
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Custom workspace name"));
    shell.workspaces.name_input = Some(input);
}

impl Shell {
    pub(super) fn switch_workspace(&mut self, name: &str, cx: &mut Context<Self>) {
        self.workspaces.error = None;
        match self.workspaces.document.switch(name) {
            Ok(()) => {
                self.layout = self.workspaces.document.layout.clone();
                self.persist_workspaces(cx, None);
            }
            Err(error) => self.workspaces.edit_failed(error),
        }
        cx.notify();
    }

    pub(super) fn restore_workspace(&mut self, cx: &mut Context<Self>) {
        self.workspaces.error = None;
        match self.workspaces.document.restore() {
            Ok(()) => {
                self.layout = self.workspaces.document.layout.clone();
                self.persist_workspaces(cx, Some("Restored the saved workspace layout.".into()));
            }
            Err(error) => self.workspaces.edit_failed(error),
        }
        cx.notify();
    }

    pub(super) fn save_workspace_as(&mut self, cx: &mut Context<Self>) {
        let name = self.workspaces.input_value(cx);
        self.workspaces.current_changed(self.layout.clone());
        self.workspaces.error = None;
        match self.workspaces.document.save_current_as(&name) {
            Ok(()) => {
                let notice = format!("Saved workspace '{}'.", self.workspaces.document.active);
                self.persist_workspaces(cx, Some(notice));
            }
            Err(error) => self.workspaces.edit_failed(error),
        }
        cx.notify();
    }

    pub(super) fn update_workspace(&mut self, cx: &mut Context<Self>) {
        self.workspaces.current_changed(self.layout.clone());
        self.workspaces.error = None;
        match self.workspaces.document.update_active() {
            Ok(()) => {
                let notice = format!("Updated workspace '{}'.", self.workspaces.document.active);
                self.persist_workspaces(cx, Some(notice));
            }
            Err(error) => self.workspaces.edit_failed(error),
        }
        cx.notify();
    }

    pub(super) fn rename_workspace(&mut self, cx: &mut Context<Self>) {
        let name = self.workspaces.input_value(cx);
        self.workspaces.error = None;
        match self.workspaces.document.rename_active(&name) {
            Ok(()) => {
                let notice = format!("Renamed the active workspace to '{}'.", name.trim());
                self.persist_workspaces(cx, Some(notice));
            }
            Err(error) => self.workspaces.edit_failed(error),
        }
        cx.notify();
    }

    pub(super) fn delete_workspace(&mut self, cx: &mut Context<Self>) {
        self.workspaces.error = None;
        match self.workspaces.document.delete_active() {
            Ok(()) => {
                self.layout = self.workspaces.document.layout.clone();
                self.persist_workspaces(
                    cx,
                    Some("Deleted the custom workspace and restored Disk overview.".into()),
                );
            }
            Err(error) => self.workspaces.edit_failed(error),
        }
        cx.notify();
    }

    /// Records the active layout, then writes one immutable document snapshot on a worker.
    /// Replacing a queued save keeps the newest action; the storage layer validates the destination
    /// again immediately before every atomic replacement.
    pub(super) fn save_layout(&mut self, cx: &mut Context<Self>) {
        self.layout_dirty = false;
        self.workspaces.current_changed(self.layout.clone());
        self.persist_workspaces(cx, None);
    }

    fn persist_workspaces(&mut self, cx: &mut Context<Self>, notice: Option<String>) {
        if !self.workspaces.can_write() {
            return;
        }
        let Some(path) = self.workspaces.path.clone() else {
            return;
        };
        let document = self.workspaces.document.clone();
        let job = cx
            .background_executor()
            .spawn(async move { save_to(&path, &document) });
        self.layout_save = Some(cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |shell, cx| {
                match result {
                    Ok(()) => {
                        shell.workspaces.error = None;
                        if let Some(notice) = notice {
                            shell.workspaces.notice = Some(notice);
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "saved workspaces not written");
                        if matches!(
                            &error,
                            WorkspaceStoreError::Parse { .. }
                                | WorkspaceStoreError::UnsupportedVersion { .. }
                                | WorkspaceStoreError::Invalid { .. }
                        ) {
                            shell.workspaces.load_failed = true;
                        }
                        shell
                            .workspaces
                            .edit_failed(format!("Workspace changes were not saved: {error}"));
                    }
                }
                cx.notify();
            });
        }));
    }
}
