//! Main window state: the chosen scan mode, what is being scanned, and the result.
//! Rendering lives in `render.rs`; shared widgets in `widgets.rs`.

mod render;
mod widgets;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use gpui_kit::*;
use hmstd_ntfs::{SummaryProgress, VolumeSummary};
use hmstd_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanProgress,
};

use crate::{
    fast,
    scan::{ScanSummary, summarize},
};

/// How often the window refreshes its live entry counter while a walk runs.
const PROGRESS_REFRESH: Duration = Duration::from_millis(200);

/// What the window is showing.
pub enum Phase {
    Idle,
    /// Folder walk in progress; the counter is shared with the worker threads.
    Walking {
        root: PathBuf,
        entries: Arc<AtomicU64>,
        cancel: CancelToken,
    },
    /// Elevated MFT read. `granted` turns true once the helper has started, which
    /// means the administrator prompt was approved. `stage` is the helper's latest
    /// progress report.
    Elevated {
        drive: PathBuf,
        granted: bool,
        stage: Option<SummaryProgress>,
    },
    Walked(ScanSummary),
    FastDone(VolumeSummary),
    Cancelled(PathBuf),
    Failed {
        drive: String,
        message: String,
    },
}

/// Which visualization the main area shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Summary,
    Treemap,
    Sunburst,
}

impl View {
    fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Treemap,
            2 => Self::Sunburst,
            _ => Self::Summary,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Summary => 0,
            Self::Treemap => 1,
            Self::Sunburst => 2,
        }
    }
}

pub struct Shell {
    volumes: Vec<PathBuf>,
    phase: Phase,
    fast_mode: bool,
    view: View,
    /// Holds the running job. Dropping it cancels the task, so it lives here.
    task: Option<Task<()>>,
}

impl Shell {
    pub fn new(volumes: Vec<PathBuf>) -> Self {
        Self {
            volumes,
            phase: Phase::Idle,
            fast_mode: false,
            view: View::Summary,
            task: None,
        }
    }

    fn is_busy(&self) -> bool {
        matches!(self.phase, Phase::Walking { .. } | Phase::Elevated { .. })
    }

    pub fn set_fast_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.is_busy() {
            self.fast_mode = on;
            cx.notify();
        }
    }

    fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.view = view;
        cx.notify();
    }

    /// Scans the chosen drive with the mode the user has selected.
    pub fn choose_volume(&mut self, drive: PathBuf, cx: &mut Context<Self>) {
        if self.fast_mode {
            self.start_fast(drive, cx);
        } else {
            self.start_walk(drive, cx);
        }
    }

    fn start_walk(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let entries = Arc::new(AtomicU64::new(0));
        let cancel = CancelToken::new();
        self.phase = Phase::Walking {
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

        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let elapsed = started.elapsed();
            let _ = this.update(cx, |shell, cx| {
                shell.phase = match outcome {
                    Ok(output) => {
                        tracing::info!(root = %root.display(), entries = output.report.entries_visited, "walk finished");
                        Phase::Walked(summarize(root, &output, elapsed))
                    }
                    Err(ScanError::Cancelled) => Phase::Cancelled(root),
                    Err(error) => {
                        tracing::error!(%error, root = %root.display(), "walk failed");
                        Phase::Failed {
                            drive: root.display().to_string(),
                            message: error.to_string(),
                        }
                    }
                };
                cx.notify();
            });
        }));

        // Repaint the live counter until the walk leaves the Walking phase.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REFRESH).await;
                let still_walking = this
                    .update(cx, |shell, cx| {
                        let walking = matches!(shell.phase, Phase::Walking { .. });
                        if walking {
                            cx.notify();
                        }
                        walking
                    })
                    .unwrap_or(false);
                if !still_walking {
                    break;
                }
            }
        })
        .detach();

        cx.notify();
    }

    fn start_fast(&mut self, drive: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(volume) = fast::raw_volume_name(&drive) else {
            self.phase = Phase::Failed {
                drive: drive.display().to_string(),
                message: "Fast mode needs a drive letter.".to_string(),
            };
            cx.notify();
            return;
        };
        self.phase = Phase::Elevated {
            drive: drive.clone(),
            granted: false,
            stage: None,
        };
        cx.notify();

        let paths = fast::FastPaths::for_this_run();
        let job_paths = paths.clone();
        let job = cx
            .background_executor()
            .spawn(async move { fast::run_elevated(&volume, &job_paths) });

        // The helper creates a marker once it runs, which proves approval. Watch for it.
        let started = paths.started.clone();
        let progress_file = paths.progress.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REFRESH).await;
                let approved = started.exists();
                let stage = fast::read_stage(&progress_file);
                let still_elevated = this
                    .update(cx, |shell, cx| match &mut shell.phase {
                        Phase::Elevated {
                            granted,
                            stage: shown,
                            ..
                        } => {
                            if approved && !*granted {
                                *granted = true;
                                cx.notify();
                            }
                            if stage.is_some() && *shown != stage {
                                *shown = stage;
                                cx.notify();
                            }
                            true
                        }
                        _ => false,
                    })
                    .unwrap_or(false);
                if !still_elevated {
                    break;
                }
            }
        })
        .detach();
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |shell, cx| {
                shell.phase = match outcome {
                    Ok(summary) => {
                        tracing::info!(volume = %summary.volume, files = summary.files, "fast scan finished");
                        Phase::FastDone(summary)
                    }
                    Err(message) => {
                        tracing::error!(%message, "fast scan failed");
                        Phase::Failed {
                            drive: drive.display().to_string(),
                            message,
                        }
                    }
                };
                cx.notify();
            });
        }));
    }

    fn cancel_walk(&mut self, cx: &mut Context<Self>) {
        if let Phase::Walking { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        cx.notify();
    }
}
