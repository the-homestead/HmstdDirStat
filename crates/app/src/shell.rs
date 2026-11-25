//! The main window: volume buttons, live progress, cancel, and the result.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use gpui_kit::component::button::*;
use gpui_kit::component::label::Label;
use gpui_kit::component::*;
use gpui_kit::*;
use hmstd_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanProgress,
};

use crate::{
    scan::{ScanSummary, summarize},
    status::status_lines,
    theme::{current_mode_name, toggle_theme},
};

/// How often the window refreshes its live entry counter while a scan runs.
const PROGRESS_REFRESH: Duration = Duration::from_millis(200);

/// What the window is showing. Scanning happens on a background thread; the
/// window only reads the shared counter and the final result.
pub enum Phase {
    Idle,
    Scanning {
        root: PathBuf,
        entries: Arc<AtomicU64>,
        cancel: CancelToken,
    },
    Finished(ScanSummary),
    Cancelled(PathBuf),
    Failed {
        root: PathBuf,
        message: String,
    },
}

pub struct Shell {
    volumes: Vec<PathBuf>,
    phase: Phase,
    /// Holds the running scan. Dropping it would cancel the task, so it lives here.
    scan_task: Option<Task<()>>,
}

impl Shell {
    pub fn new(volumes: Vec<PathBuf>) -> Self {
        Self {
            volumes,
            phase: Phase::Idle,
            scan_task: None,
        }
    }

    fn is_scanning(&self) -> bool {
        matches!(self.phase, Phase::Scanning { .. })
    }

    /// Starts a scan of `root` on the background executor. Ignored while one runs.
    fn start_scan(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.is_scanning() {
            return;
        }
        let entries = Arc::new(AtomicU64::new(0));
        let cancel = CancelToken::new();
        self.phase = Phase::Scanning {
            root: root.clone(),
            entries: entries.clone(),
            cancel: cancel.clone(),
        };

        let started = Instant::now();
        let scan_root = root.clone();
        let job = cx.background_executor().spawn(async move {
            let mut on_progress =
                |progress: ScanProgress| entries.store(progress.entries_visited, Ordering::Relaxed);
            GenericScanner.scan(
                &scan_root,
                &ScanOptions::default(),
                &cancel,
                &mut on_progress,
            )
        });

        self.scan_task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let elapsed = started.elapsed();
            let delivered = this.update(cx, |shell, cx| {
                shell.phase = match outcome {
                    Ok(output) => {
                        tracing::info!(root = %root.display(), entries = output.report.entries_visited, "scan finished");
                        Phase::Finished(summarize(root, &output, elapsed))
                    }
                    Err(ScanError::Cancelled) => Phase::Cancelled(root),
                    Err(error) => {
                        tracing::error!(%error, root = %root.display(), "scan failed");
                        Phase::Failed {
                            root,
                            message: error.to_string(),
                        }
                    }
                };
                cx.notify();
            });
            if let Err(error) = delivered {
                tracing::debug!(%error, "window closed before the scan finished");
            }
        }));

        // Repaint the live counter until the scan leaves the Scanning phase.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REFRESH).await;
                let still_scanning = this
                    .update(cx, |shell, cx| {
                        let scanning = shell.is_scanning();
                        if scanning {
                            cx.notify();
                        }
                        scanning
                    })
                    .unwrap_or(false);
                if !still_scanning {
                    break;
                }
            }
        })
        .detach();

        cx.notify();
    }

    fn cancel_scan(&mut self, cx: &mut Context<Self>) {
        if let Phase::Scanning { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        cx.notify();
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shell = cx.entity();
        let mode = current_mode_name(cx);
        let scanning = self.is_scanning();

        let mut volume_row = div().h_flex().gap_2().flex_wrap();
        for (index, volume) in self.volumes.iter().enumerate() {
            let target = volume.clone();
            let shell = shell.clone();
            volume_row = volume_row.child(
                Button::new(format!("volume-{index}"))
                    .label(volume.display().to_string())
                    .disabled(scanning)
                    .on_click(move |_, _, cx| {
                        shell.update(cx, |this, cx| this.start_scan(target.clone(), cx));
                    }),
            );
        }

        let mut status = div().v_flex().gap_1();
        for line in status_lines(&self.phase) {
            status = status.child(Label::new(line));
        }

        let mut page = div()
            .v_flex()
            .gap_3()
            .size_full()
            .p_4()
            .child(Label::new("HmstdDirStat"))
            .child(Label::new(format!("Theme: {mode}")))
            .child(
                Button::new("toggle-theme")
                    .primary()
                    .label("Toggle theme")
                    .on_click(|_, _, cx| toggle_theme(cx)),
            )
            .child(Label::new("Volumes"))
            .child(volume_row);

        if scanning {
            let shell = shell.clone();
            page = page.child(Button::new("cancel-scan").label("Cancel scan").on_click(
                move |_, _, cx| {
                    shell.update(cx, |this, cx| this.cancel_scan(cx));
                },
            ));
        }

        page.child(status)
    }
}
