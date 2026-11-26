//! Main window state: the chosen scan mode, what is being scanned, and the result.
//! Rendering lives in `render.rs`; shared widgets in `widgets.rs`. The visualizations are
//! widgets laid out by `workspace.rs`; their bodies are in `widget_bodies.rs`.

mod actions;
mod duplicates;
mod duplicates_view;
mod empty_state;
mod extensions_view;
#[path = "../fileops_ui/mod.rs"]
mod fileops_ui;
mod files_tree;
mod menu;
mod render;
mod results;
mod rings;
mod settings;
mod toolbar;
mod treemap;
mod view_menu;
mod views;
mod widget_bodies;
mod widgets;
mod workspace;
mod workspace_manager;
mod workspace_menu;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use gpui_kit::Subscription;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tree::TreeState;
use gpui_kit::*;
use timberstat_core::workspace::WorkspaceLayout;
use timberstat_filesystem::{FileTree, NodeId};
use timberstat_ntfs::{SummaryProgress, VolumeSummary};
use timberstat_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanProgress,
};

use crate::{
    explore::Basis,
    fast,
    scan::{ScanSummary, summarize},
    search::Query,
};
use workspace::DragStart;

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

/// A right-click menu opened on a tile or arc: which node, and where the pointer was.
pub struct ContextMenu {
    pub node: NodeId,
    pub at: Point<Pixels>,
    /// The gpui-kit popup, built on the first render after opening (it needs the window).
    pub popup: Option<Entity<gpui_kit::component::menu::PopupMenu>>,
}

pub struct Shell {
    volumes: Vec<PathBuf>,
    phase: Phase,
    fast_mode: bool,
    /// Where the log files are written; the sidebar opens this folder.
    log_dir: PathBuf,
    /// The tree from the last walk, shared by the views. Fast mode leaves it empty.
    tree: Option<Arc<FileTree>>,
    /// Absolute root used to turn tree-relative paths into file-operation paths.
    scan_root: Option<PathBuf>,
    tree_state: Option<Entity<TreeState>>,
    /// The folder the Treemap, Sunburst and pie are showing.
    focus: NodeId,
    /// Where the sunburst disc was last painted, in window coordinates.
    sunburst_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The sunburst arc under the pointer, and the pointer's place on the disc.
    sunburst_hover: Option<(NodeId, Point<Pixels>)>,
    /// Which size the treemap, sunburst and pie show.
    basis: Basis,
    /// Extension totals for the focused folder. Recomputed by `refresh_extensions` when the
    /// focus, basis or tree changes, never per frame.
    extension_totals: Arc<Vec<crate::extensions::ExtensionTotal>>,
    /// Scales the window's rem size (text and rem-based spacing). Clamped at load.
    ui_scale: f32,
    /// Which widgets show, where, and in what stacking order.
    layout: WorkspaceLayout,
    /// Saved presets, custom workspaces and persistence status.
    workspaces: workspace_manager::WorkspaceUi,
    /// Where the widget drag in progress began, if one is running.
    drag_start: Option<DragStart>,
    /// The workspace's size in pixels, measured while it paints.
    workspace_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Content size of each widget body (width, height) as measured on the last frame.
    /// Widgets size their graphs and lists from it.
    widget_rooms: Rc<RefCell<HashMap<timberstat_core::workspace::WidgetKind, (f32, f32)>>>,
    /// Whether the View menu is open.
    view_menu: bool,
    /// Whether the Settings panel is open.
    settings_open: bool,
    /// The layout changed and has not been written yet.
    layout_dirty: bool,
    /// The running workspace-document write. Replacing it drops a write not yet started.
    layout_save: Option<Task<()>>,
    /// The tile or arc the user selected with a single click.
    selected: Option<NodeId>,
    /// The open right-click menu, if any.
    menu: Option<ContextMenu>,
    /// The search box. Made on first render, because creating it needs a window.
    search_input: Option<Entity<InputState>>,
    _search_sub: Option<Subscription>,
    /// The search text the treemap and sunburst highlight.
    query: Query,
    /// Holds the running scan job. Dropping it cancels the task, so it lives here.
    task: Option<Task<()>>,
    /// Delete preview, confirmation, worker and result state.
    delete: fileops_ui::DeleteUi,
    /// Copy and move dialog state.
    transfer: fileops_ui::TransferUi,
    /// Rename dialog state.
    rename: fileops_ui::RenameUi,
    /// Duplicate search run and its result.
    dup: duplicates::DupUi,
}

impl Shell {
    pub fn new(
        volumes: Vec<PathBuf>,
        log_dir: PathBuf,
        layout: WorkspaceLayout,
        ui_scale: f32,
    ) -> Self {
        let (workspaces, layout) = workspace_manager::WorkspaceUi::restore(layout);
        Self {
            volumes,
            phase: Phase::Idle,
            fast_mode: false,
            log_dir,
            tree: None,
            scan_root: None,
            tree_state: None,
            focus: NodeId::ROOT,
            sunburst_bounds: Rc::new(Cell::new(None)),
            sunburst_hover: None,
            basis: Basis::Logical,
            extension_totals: Arc::new(Vec::new()),
            ui_scale: timberstat_core::config::clamp_ui_scale(ui_scale),
            layout,
            workspaces,
            drag_start: None,
            workspace_bounds: Rc::new(Cell::new(None)),
            widget_rooms: Rc::new(RefCell::new(HashMap::new())),
            view_menu: false,
            settings_open: false,
            layout_dirty: false,
            layout_save: None,
            selected: None,
            menu: None,
            search_input: None,
            _search_sub: None,
            query: Query::default(),
            task: None,
            delete: fileops_ui::DeleteUi::default(),
            transfer: fileops_ui::TransferUi::default(),
            rename: fileops_ui::RenameUi::default(),
            dup: duplicates::DupUi::default(),
        }
    }

    /// Rebuilds the folder tree with the current search, so matching rows are marked.
    pub fn refresh_tree_state(&mut self, cx: &mut Context<Self>) {
        if let Some(files) = self.tree.clone() {
            self.tree_state = Some(files_tree::build_tree_state(&files, &self.query, cx));
        }
    }

    /// Creates the search box once, and keeps `query` in step with what the user types.
    pub fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_input.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search  ·  name  ext:rpf  size:>1gb  path:steam")
        });
        let sub = cx.subscribe_in(
            &input,
            window,
            |shell: &mut Shell, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value();
                    shell.query = shell.requery(&text);
                    shell.refresh_tree_state(cx);
                    cx.notify();
                }
            },
        );
        self.search_input = Some(input);
        self._search_sub = Some(sub);
    }

    fn clear_tree(&mut self) {
        self.dup.reset();
        self.tree = None;
        self.scan_root = None;
        self.query = Query::new(self.query.text(), None);
        self.tree_state = None;
        self.focus = NodeId::ROOT;
        self.refresh_extensions();
        self.sunburst_hover = None;
    }

    /// Keeps a finished walk's tree for the views and builds the Files tab from it.
    fn install_tree(&mut self, files: FileTree, scan_root: PathBuf, cx: &mut Context<Self>) {
        self.query = Query::new(self.query.text(), Some(&files));
        self.tree_state = Some(files_tree::build_tree_state(&files, &self.query, cx));
        self.tree = Some(Arc::new(files));
        self.scan_root = Some(scan_root);
        self.focus = NodeId::ROOT;
        self.refresh_extensions();
    }

    /// Recomputes the Extensions totals for the focused folder. This walks the whole subtree,
    /// so it runs only when the focus, basis or tree changes.
    pub fn refresh_extensions(&mut self) {
        self.extension_totals = Arc::new(match &self.tree {
            Some(files) => crate::extensions::totals(files, self.focus, self.basis),
            None => Vec::new(),
        });
    }

    pub fn focus_on(&mut self, node: NodeId, cx: &mut Context<Self>) {
        self.focus = node;
        self.refresh_extensions();
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

    pub fn focus_up(&mut self, cx: &mut Context<Self>) {
        let parent = self
            .tree
            .as_ref()
            .and_then(|files| files.node(self.focus))
            .and_then(|node| node.parent());
        if let Some(parent) = parent {
            self.focus = parent;
            self.refresh_extensions();
            self.sunburst_hover = None;
            cx.notify();
        }
    }

    fn is_busy(&self) -> bool {
        matches!(self.phase, Phase::Walking { .. } | Phase::Elevated { .. })
            || !self.delete.is_closed()
            || !self.transfer.is_closed()
            || !self.rename.is_closed()
    }

    pub fn set_fast_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.is_busy() {
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

    fn cancel_walk(&mut self, cx: &mut Context<Self>) {
        if let Phase::Walking { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        cx.notify();
    }
}
