//! Main window state: the chosen scan mode, what is being scanned, and the result.
//! Rendering lives in `render.rs`; shared widgets in `widgets.rs`.

mod menu;
mod render;
mod results;
mod rings;
mod treemap;
mod views;
mod widgets;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use std::{cell::Cell, rc::Rc};

use gpui_kit::component::tree::TreeState;
use gpui_kit::*;
use hmstd_filesystem::{EntryKind, FileTree, NodeId};
use hmstd_ntfs::{SummaryProgress, VolumeSummary};
use hmstd_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanProgress,
};

use crate::{
    explore::Basis,
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
    Files,
    Treemap,
    Sunburst,
}

impl View {
    fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Files,
            2 => Self::Treemap,
            3 => Self::Sunburst,
            _ => Self::Summary,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Summary => 0,
            Self::Files => 1,
            Self::Treemap => 2,
            Self::Sunburst => 3,
        }
    }
}

/// A right-click menu opened on a tile or arc: which node, and where the pointer was.
#[derive(Debug, Clone, Copy)]
pub struct ContextMenu {
    pub node: NodeId,
    pub at: Point<Pixels>,
}

pub struct Shell {
    volumes: Vec<PathBuf>,
    phase: Phase,
    fast_mode: bool,
    view: View,
    /// Where the log files are written; the sidebar opens this folder.
    log_dir: PathBuf,
    /// The tree from the last walk, shared by the views. Fast mode leaves it empty.
    tree: Option<Arc<FileTree>>,
    tree_state: Option<Entity<TreeState>>,
    /// The folder the Treemap, Sunburst and pie are showing.
    focus: NodeId,
    /// Where the sunburst disc was last painted, in window coordinates.
    sunburst_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The sunburst arc under the pointer, and the pointer's place on the disc.
    sunburst_hover: Option<(NodeId, Point<Pixels>)>,
    /// Which size the treemap, sunburst and pie show.
    basis: Basis,
    /// The tile or arc the user selected with a single click.
    selected: Option<NodeId>,
    /// The open right-click menu, if any.
    menu: Option<ContextMenu>,
    /// Holds the running job. Dropping it cancels the task, so it lives here.
    task: Option<Task<()>>,
}

impl Shell {
    pub fn new(volumes: Vec<PathBuf>, log_dir: PathBuf) -> Self {
        Self {
            volumes,
            phase: Phase::Idle,
            fast_mode: false,
            view: View::Summary,
            log_dir,
            tree: None,
            tree_state: None,
            focus: NodeId::ROOT,
            sunburst_bounds: Rc::new(Cell::new(None)),
            sunburst_hover: None,
            basis: Basis::Logical,
            selected: None,
            menu: None,
            task: None,
        }
    }

    fn clear_tree(&mut self) {
        self.tree = None;
        self.tree_state = None;
        self.focus = NodeId::ROOT;
        self.sunburst_hover = None;
    }

    /// Keeps a finished walk's tree for the views and builds the Files tab from it.
    fn install_tree(&mut self, files: FileTree, cx: &mut Context<Self>) {
        self.tree_state = Some(views::build_tree_state(&files, cx));
        self.tree = Some(Arc::new(files));
        self.focus = NodeId::ROOT;
    }

    pub fn focus_on(&mut self, node: NodeId, cx: &mut Context<Self>) {
        self.focus = node;
        self.selected = None;
        self.menu = None;
        self.sunburst_hover = None;
        cx.notify();
    }

    /// Tracks which sunburst arc the pointer is over, for its tooltip.
    pub fn sunburst_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let hover = match (self.sunburst_bounds.get(), self.tree.as_deref()) {
            (Some(bounds), Some(files)) => {
                let arcs = rings::sunburst_arcs(files, self.focus, self.basis);
                rings::sunburst_hit(&arcs, bounds, position)
                    .map(|node| (node, position - bounds.origin))
            }
            _ => None,
        };
        if hover != self.sunburst_hover {
            self.sunburst_hover = hover;
            cx.notify();
        }
    }

    pub fn sunburst_leave(&mut self, cx: &mut Context<Self>) {
        if self.sunburst_hover.take().is_some() {
            cx.notify();
        }
    }

    pub fn set_basis(&mut self, basis: Basis, cx: &mut Context<Self>) {
        if self.basis != basis {
            self.basis = basis;
            cx.notify();
        }
    }

    /// Single click selects; double-click on a folder enters it.
    pub fn click_node(&mut self, node: NodeId, click_count: usize, cx: &mut Context<Self>) {
        self.menu = None;
        let is_dir = self
            .tree
            .as_deref()
            .and_then(|files| files.node(node))
            .is_some_and(|entry| entry.kind() == EntryKind::Directory);
        if click_count >= 2 && is_dir {
            self.focus_on(node, cx);
        } else {
            self.selected = Some(node);
            cx.notify();
        }
    }

    /// The sunburst arc under a window position, if any.
    fn sunburst_node_at(&self, at: Point<Pixels>) -> Option<NodeId> {
        let bounds = self.sunburst_bounds.get()?;
        let files = self.tree.as_deref()?;
        let arcs = rings::sunburst_arcs(files, self.focus, self.basis);
        rings::sunburst_hit(&arcs, bounds, at)
    }

    pub fn sunburst_click(
        &mut self,
        at: Point<Pixels>,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        match self.sunburst_node_at(at) {
            Some(node) => self.click_node(node, click_count, cx),
            None => {
                self.menu = None;
                self.selected = None;
                cx.notify();
            }
        }
    }

    pub fn sunburst_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(node) = self.sunburst_node_at(at) {
            self.open_menu(node, at, cx);
        }
    }

    pub fn open_menu(&mut self, node: NodeId, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu = Some(ContextMenu { node, at });
        cx.notify();
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    /// Puts the node's full path on the clipboard and closes the menu.
    pub fn copy_path(&mut self, node: NodeId, cx: &mut Context<Self>) {
        if let Some(path) = self.path_of_node(node) {
            cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
        }
        self.menu = None;
        cx.notify();
    }

    /// Opens Explorer with the node selected, and closes the menu.
    pub fn show_in_explorer(&mut self, node: NodeId, cx: &mut Context<Self>) {
        if let Some(path) = self.path_of_node(node)
            && let Err(error) = crate::open::select_in_folder(&path)
        {
            tracing::error!(%error, path = %path.display(), "could not open Explorer");
        }
        self.menu = None;
        cx.notify();
    }

    /// The full path of a node in the current tree, if it has one.
    pub fn path_of_node(&self, node: NodeId) -> Option<PathBuf> {
        self.tree.as_deref()?.path_of(node)
    }

    pub fn focus_up(&mut self, cx: &mut Context<Self>) {
        let parent = self
            .tree
            .as_ref()
            .and_then(|files| files.node(self.focus))
            .and_then(|node| node.parent());
        if let Some(parent) = parent {
            self.focus = parent;
            self.sunburst_hover = None;
            cx.notify();
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
        // The pointer may leave the sunburst while another tab is showing.
        self.sunburst_hover = None;
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
            let _ = this.update(cx, |shell, cx| {
                shell.phase = match outcome {
                    Ok(output) => {
                        tracing::info!(root = %root.display(), entries = output.report.entries_visited, "walk finished");
                        let summary = summarize(root, &output, elapsed);
                        shell.install_tree(output.tree, cx);
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
                let phase = match outcome {
                    Ok((summary, tree)) => {
                        tracing::info!(volume = %summary.volume, files = summary.files, "fast scan finished");
                        shell.install_tree(tree, cx);
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

    fn cancel_walk(&mut self, cx: &mut Context<Self>) {
        if let Phase::Walking { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        cx.notify();
    }
}
