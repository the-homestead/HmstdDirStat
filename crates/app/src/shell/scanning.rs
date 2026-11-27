//! Scan lifecycle actions, kept out of the main shell state module.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use timberstat_core::config;
use timberstat_engine::{ScanKind, ScanModeChoice};

use gpui_kit::*;
use timberstat_scanner::ScanProgress;

use super::{PROGRESS_REFRESH, Phase, Shell};
use crate::{history, platform, scan::summarize};
use timberstat_engine::fast;

/// The drive letter of a path, uppercase. `?` when there is none.
pub(super) fn drive_letter(drive: &Path) -> char {
    timberstat_engine::drive_letter(drive).unwrap_or('?')
}

impl Shell {
    pub fn is_busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Checking { .. } | Phase::Walking { .. } | Phase::Elevated { .. }
        ) || !self.delete.is_closed()
            || !self.transfer.is_closed()
            || !self.rename.is_closed()
            || self.history.working
    }

    /// Scans a drive. Its mode is the drive's override, or `Auto`. The disk check runs once per
    /// drive per session, off the UI thread, and the engine decides the mode from its answer.
    pub fn choose_volume(&mut self, drive: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let letter = drive_letter(&drive);
        let choice = self.drive_mode(letter);
        match self.hdd_cache.get(&letter).copied() {
            Some(hdd) => {
                let kind = timberstat_engine::plan(&drive, choice, Some(hdd));
                self.begin_scan(drive, kind, cx);
            }
            None if choice == ScanModeChoice::Auto && platform::FAST_NTFS_AVAILABLE => {
                self.detect_then_scan(drive, letter, choice, cx);
            }
            None => {
                let kind = timberstat_engine::plan(&drive, choice, Some(false));
                self.begin_scan(drive, kind, cx);
            }
        }
    }

    /// The mode a drive is set to. Drives without an override use `Auto`.
    pub fn drive_mode(&self, letter: char) -> ScanModeChoice {
        timberstat_engine::choice_for(&self.drive_modes, letter)
    }

    /// Sets a drive's mode and saves it to config.json. The change applies to the next scan.
    pub fn set_drive_mode(&mut self, letter: char, choice: ScanModeChoice, cx: &mut Context<Self>) {
        self.drive_modes
            .insert(letter.to_ascii_uppercase().to_string(), choice);
        match config::default_config_path() {
            Ok(path) => {
                if let Err(error) = config::store_drive_mode(&path, letter, choice) {
                    tracing::error!(%error, "drive mode changed but could not be saved");
                }
            }
            Err(error) => {
                tracing::error!(%error, "drive mode changed but no config path is available");
            }
        }
        cx.notify();
    }

    /// Probes the disk in the background, remembers the answer for the session, then scans.
    fn detect_then_scan(
        &mut self,
        drive: PathBuf,
        letter: char,
        choice: ScanModeChoice,
        cx: &mut Context<Self>,
    ) {
        let probe = drive.clone();
        let detect = cx
            .background_executor()
            .spawn(async move { timberstat_engine::probe::is_spinning_hdd(&probe) });
        self.phase = Phase::Checking {
            drive: drive.clone(),
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let hdd = detect.await;
            let _ = this.update(cx, |shell, cx| {
                shell.hdd_cache.insert(letter, hdd);
                // The check is over. Clear its phase first: the scan start refuses to run while
                // the app is busy, and a `Checking` phase counts as busy.
                shell.phase = Phase::Idle;
                let kind = timberstat_engine::plan(&drive, choice, Some(hdd));
                shell.begin_scan(drive, kind, cx);
            });
        })
        .detach();
    }

    fn begin_scan(&mut self, drive: PathBuf, kind: ScanKind, cx: &mut Context<Self>) {
        self.history.notice = None;
        self.fast_mode = kind == ScanKind::Fast;
        match kind {
            ScanKind::Fast => self.start_fast(drive, cx),
            ScanKind::Walk => self.start_walk(drive, cx),
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
            timberstat_engine::scan_walk(&scan_root, &cancel, &mut on_progress)
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
                        let provenance = history::new_provenance(
                            history::method_from_scanner(output.report.method),
                            &root,
                        );
                        shell.install_tree(output.tree, root.clone(), provenance, cx);
                        Phase::Walked(summary)
                    }
                    Err(timberstat_engine::EngineError::Cancelled) => Phase::Cancelled(root),
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
                        let provenance = history::new_provenance(
                            timberstat_scan_history::ScanMethod::NtfsMft,
                            &drive,
                        );
                        shell.install_tree(tree, drive.clone(), provenance, cx);
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
