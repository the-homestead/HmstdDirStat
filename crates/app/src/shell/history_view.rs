//! Saving the open scan and reopening a saved one. The Scan history dialog is a small view of
//! its own, so its list updates when the background load finishes without a dialog rebuild.

use gpui_kit::component::{button::Button, *};
use gpui_kit::*;
use timberstat_filesystem::FileTree;
use timberstat_scan_history::{ScanMethod as HistoryMethod, ScanProvenance, ScanSummary as Saved};

use super::{Phase, Shell, results::result_page, widgets::*};
use crate::{
    history,
    scan::{ScanSummary, format_bytes, format_count},
};

/// Save and reopen state on the shell.
#[derive(Default)]
pub struct HistoryUi {
    /// A save or reopen is running. Scans wait for it, and a second one is refused.
    pub working: bool,
    /// The last save or reopen outcome, shown in the status bar until the next scan.
    pub notice: Option<String>,
    /// Dropping the running job cancels it, so it lives here.
    pub task: Option<Task<()>>,
}

impl Shell {
    /// Writes the open scan to the store on a background thread.
    pub(super) fn save_scan(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let (Some(tree), Some(provenance)) = (self.tree.clone(), self.provenance.clone()) else {
            return;
        };
        let dir = match history::store_dir() {
            Ok(dir) => dir,
            Err(error) => {
                self.history.notice = Some(format!("Scan not saved: {error}"));
                cx.notify();
                return;
            }
        };
        self.history.working = true;
        self.history.notice = Some("Saving scan…".to_string());
        cx.notify();

        let root = provenance.root.clone();
        let job = cx
            .background_executor()
            .spawn(async move { history::save_scan(&dir, &tree, &provenance) });
        self.history.task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |shell, cx| {
                shell.history.working = false;
                shell.history.notice = Some(match outcome {
                    Ok(()) => {
                        tracing::info!(root = %root.display(), "scan saved");
                        format!("Saved scan: {}", root.display())
                    }
                    Err(error) => {
                        tracing::error!(%error, "scan not saved");
                        format!("Scan not saved: {error}")
                    }
                });
                cx.notify();
            });
        }));
    }

    /// Loads a saved scan on a background thread, then shows it in place of the current one.
    pub(super) fn reopen_scan(&mut self, id: String, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let dir = match history::store_dir() {
            Ok(dir) => dir,
            Err(error) => {
                self.history.notice = Some(format!("Could not open saved scan: {error}"));
                cx.notify();
                return;
            }
        };
        self.history.working = true;
        self.history.notice = Some("Opening saved scan…".to_string());
        cx.notify();

        let job = cx
            .background_executor()
            .spawn(async move { history::open_scan(&dir, &id) });
        self.history.task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |shell, cx| {
                shell.history.working = false;
                match outcome {
                    Ok((tree, provenance)) => shell.show_saved(tree, provenance, cx),
                    Err(error) => {
                        tracing::error!(%error, "saved scan not opened");
                        shell.history.notice = Some(format!("Could not open saved scan: {error}"));
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Replaces the open scan with a reopened one and shows its result panel.
    fn show_saved(&mut self, tree: FileTree, provenance: ScanProvenance, cx: &mut Context<Self>) {
        let summary = history::summary_for_tree(&tree, &provenance);
        let saved = history::format_utc(provenance.timestamp_unix_secs);
        tracing::info!(root = %provenance.root.display(), "saved scan reopened");
        self.history.notice = None;
        self.clear_tree();
        self.fast_mode = provenance.method == HistoryMethod::NtfsMft;
        let root = provenance.root.clone();
        self.install_tree(tree, root, provenance, cx);
        self.phase = Phase::Reopened(summary, saved);
    }

    /// Opens the Scan history dialog. Its list loads in the background.
    pub(super) fn open_scan_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = cx.entity();
        let list = cx.new(|cx| HistoryDialog::new(handle, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Scan history").w(px(640.)).child(list.clone())
        });
    }
}

/// The list inside the Scan history dialog. It owns its rows and reloads them itself.
pub struct HistoryDialog {
    shell: Entity<Shell>,
    rows: Option<Result<Vec<Saved>, String>>,
    _load: Option<Task<()>>,
}

impl HistoryDialog {
    fn new(shell: Entity<Shell>, cx: &mut Context<Self>) -> Self {
        let dir = match history::store_dir() {
            Ok(dir) => dir,
            Err(error) => {
                return Self {
                    shell,
                    rows: Some(Err(error)),
                    _load: None,
                };
            }
        };
        let job = cx
            .background_executor()
            .spawn(async move { history::list_scans(&dir) });
        let load = cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |view, cx| {
                view.rows = Some(outcome);
                cx.notify();
            });
        });
        Self {
            shell,
            rows: None,
            _load: Some(load),
        }
    }
}

impl Render for HistoryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::from_app(cx);
        let body = match &self.rows {
            None => message_line("Loading saved scans…", p),
            Some(Err(error)) => message_line(&format!("Saved scans are unavailable: {error}"), p),
            Some(Ok(list)) if list.is_empty() => message_line(
                "No saved scans yet. Use File > Save scan after a scan finishes.",
                p,
            ),
            Some(Ok(list)) => list
                .iter()
                .enumerate()
                .fold(div().v_flex().gap_1(), |column, (index, saved)| {
                    column.child(self.row(index, saved, p))
                }),
        };
        div().v_flex().gap_2().p_2().child(body)
    }
}

impl HistoryDialog {
    /// One saved scan: its root and when it was saved, with Reopen when it can be read.
    fn row(&self, index: usize, saved: &Saved, p: Palette) -> Div {
        let detail = match &saved.error {
            Some(error) => format!("Cannot be opened: {error}"),
            None => format!(
                "{} · saved {}",
                history::method_label(saved.provenance.method),
                history::format_utc(saved.provenance.timestamp_unix_secs)
            ),
        };
        let info = div()
            .flex_1()
            .min_w_0()
            .v_flex()
            .child(
                div()
                    .text_sm()
                    .truncate()
                    .child(saved.provenance.root.display().to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(p.muted_foreground)
                    .truncate()
                    .child(detail),
            );
        let row = div()
            .h_flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(p.border)
            .child(info);
        if saved.error.is_some() {
            return row;
        }
        let id = saved.id.clone();
        let shell = self.shell.clone();
        row.child(
            Button::new(("scan-reopen", index))
                .small()
                .label("Reopen")
                .on_click(move |_, window, cx| {
                    let id = id.clone();
                    shell.update(cx, |shell, cx| shell.reopen_scan(id, cx));
                    window.close_dialog(cx);
                }),
        )
    }
}

fn message_line(text: &str, p: Palette) -> Div {
    div()
        .text_xs()
        .text_color(p.muted_foreground)
        .child(text.to_string())
}

/// The result panel for a reopened scan: the saved totals and the time it was saved.
pub(super) fn reopened_result(summary: &ScanSummary, saved: &str, p: Palette) -> Div {
    let stats = [
        Stat {
            label: "Files",
            value: format_count(summary.files),
        },
        Stat {
            label: "Folders",
            value: format_count(summary.directories),
        },
        Stat {
            label: "Logical size",
            value: format_bytes(summary.logical_bytes),
        },
        Stat {
            label: "Physical size",
            value: bytes_or_na(summary.physical_bytes),
        },
        Stat {
            label: "Saved",
            value: saved.to_string(),
        },
        Stat {
            label: "Unreadable",
            value: format_count(summary.issues),
        },
    ];
    result_page(
        &summary.root.display().to_string(),
        &format!("Saved scan · {}", summary.method),
        summary.complete,
        p,
    )
    .child(stat_grid(&stats, p))
}
