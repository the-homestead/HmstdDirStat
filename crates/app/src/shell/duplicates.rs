//! The duplicate search in the app: starting and cancelling a run, keeping its result, and
//! the click behaviour of a duplicate row. The pipeline lives in `hmstd-duplicates`; this
//! module only drives it on the background executor. Nothing here deletes a file. Deleting goes
//! through the context menu and the delete preview.

use std::collections::HashSet;
use std::sync::Arc;

use gpui_kit::*;
use hmstd_duplicates::{DuplicateReport, Options, Progress, file_items, find_duplicates};
use hmstd_fileops::tree_base;
use hmstd_filesystem::NodeId;

use super::{PROGRESS_REFRESH, Shell};
use crate::search::Query;

/// State of the duplicate panel. `task` is kept here because dropping a gpui task cancels it.
#[derive(Default)]
pub struct DupUi {
    /// Progress of the run in flight. `Some` only while a run is going.
    progress: Option<Arc<Progress>>,
    /// The last finished run, cancelled or complete.
    pub(super) report: Option<DuplicateReport>,
    /// Every file in the last finished run's groups. Feeds `dup:` in the search box.
    members: Option<HashSet<NodeId>>,
    task: Option<Task<()>>,
    /// Identifies the current run. A result from an older run is dropped.
    generation: u64,
}

impl DupUi {
    pub(super) fn is_running(&self) -> bool {
        self.progress.is_some()
    }

    pub(super) fn progress(&self) -> Option<&Progress> {
        self.progress.as_deref()
    }

    /// The members of the last finished run, `None` before any run has finished.
    pub(super) fn members(&self) -> Option<&HashSet<NodeId>> {
        self.members.as_ref()
    }

    /// Stops a run in flight and forgets the result. Called when the tree is replaced, since the
    /// result's node ids then point at a different tree.
    pub(super) fn reset(&mut self) {
        if let Some(progress) = self.progress.take() {
            progress.cancel();
        }
        self.report = None;
        self.members = None;
        self.task = None;
        self.generation += 1;
    }
}

/// Every node id in the report's groups.
fn member_ids(report: &DuplicateReport) -> HashSet<NodeId> {
    report
        .groups
        .iter()
        .flat_map(|group| group.members.iter().map(|member| member.id))
        .collect()
}

impl Shell {
    /// The search over the current tree, with the last duplicate result for `dup:` terms.
    pub(super) fn requery(&self, text: &str) -> Query {
        Query::with_duplicates(text, self.tree.as_deref(), self.dup.members())
    }

    /// Starts a duplicate search over the scanned tree. Does nothing while a run is going or
    /// another operation is busy.
    pub fn start_duplicates(&mut self, cx: &mut Context<Self>) {
        if self.dup.is_running() || self.is_busy() {
            return;
        }
        let (Some(files), Some(scan_root)) = (self.tree.clone(), self.scan_root.clone()) else {
            return;
        };
        let progress = Progress::new();
        self.dup.generation += 1;
        let generation = self.dup.generation;
        self.dup.progress = Some(progress.clone());
        self.dup.report = None;
        self.dup.members = None;

        let job = cx.background_executor().spawn(async move {
            let items = file_items(&files, &tree_base(&scan_root));
            find_duplicates(&items, &Options::default(), &progress)
        });
        self.dup.task = Some(cx.spawn(async move |this, cx| {
            let report = job.await;
            let _ = this.update(cx, |shell, cx| {
                if shell.dup.generation == generation {
                    shell.dup.progress = None;
                    shell.dup.members = Some(member_ids(&report));
                    shell.dup.report = Some(report);
                    shell.query = shell.requery(shell.query.text());
                    cx.notify();
                }
            });
        }));

        // Repaint the stage line until the run leaves the panel.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REFRESH).await;
                let running = this
                    .update(cx, |shell, cx| {
                        let running = shell.dup.generation == generation && shell.dup.is_running();
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
        cx.notify();
    }

    /// Asks the running search to stop. The worker checks the flag between files and between
    /// read chunks; the panel then shows the run as cancelled with no groups.
    pub fn cancel_duplicates(&mut self, cx: &mut Context<Self>) {
        if let Some(progress) = self.dup.progress() {
            progress.cancel();
        }
        cx.notify();
    }

    /// A duplicate row was clicked. A single click selects the file, shared with the tree and
    /// the graphs. A double click opens the folder that holds it.
    pub fn duplicate_click(&mut self, node: NodeId, click_count: usize, cx: &mut Context<Self>) {
        self.menu = None;
        if click_count >= 2 {
            let parent = self
                .tree
                .as_deref()
                .and_then(|files| files.node(node))
                .and_then(|entry| entry.parent());
            if let Some(parent) = parent {
                self.focus_on(parent, cx);
                return;
            }
        }
        self.selected = Some(node);
        cx.notify();
    }
}
