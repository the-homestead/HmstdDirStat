//! Scan lifecycle actions, kept out of the main shell state module.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use gpui_kit::*;
use timberstat_scanner::{FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanProgress};

use super::{PROGRESS_REFRESH, Phase, Shell};
use crate::{fast, platform, scan::summarize};

impl Shell {
    pub fn is_busy(&self) -> bool {
        matches!(self.phase, Phase::Walking { .. } | Phase::Elevated { .. })
            || !self.delete.is_closed()
            || !self.transfer.is_closed()
            || !self.rename.is_closed()
    }

    pub fn set_fast_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        // Fast NTFS is Windows only; on other systems it can never be switched on.
        let allowed = !on || platform::FAST_NTFS_AVAILABLE;
        if !self.is_busy() && allowed {
            self.fast_mode = on;
            cx.notify();
        }
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
        let cancel = timberstat_scanner::CancelToken::new();
        self.clear_tree();
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
            tracing::info!("{}", timberstat_core::timing::line("scan", elapsed));
            let _ = this.update(cx, |shell, cx| {
                shell.phase = match outcome {
                    Ok(output) => {
                        tracing::info!(root = %root.display(), entries = output.report.entries_visited, "walk finished");
                        let summary = summarize(root.clone(), &output, elapsed);
                        shell.install_tree(output.tree, root.clone(), cx);
                        Phase::Walked(summary)
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
                let phase = match outcome {
                    Ok((summary, tree)) => {
                        tracing::info!(volume = %summary.volume, files = summary.files, "fast scan finished");
                        shell.install_tree(tree, drive.clone(), cx);
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
                shell.phase = phase;
                cx.notify();
            });
        }));
    }

    pub(super) fn cancel_walk(&mut self, cx: &mut Context<Self>) {
        if let Phase::Walking { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        cx.notify();
    }
}
