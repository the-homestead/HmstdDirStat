//! Main window state. Rendering and interaction live in focused sibling modules.

mod actions;
mod duplicates;
mod duplicates_view;
mod empty_state;
mod extensions_view;
#[path = "../fileops_ui/mod.rs"]
mod fileops_ui;
mod files_tree;
mod menu;
mod menubar;
mod palette;
mod render;
mod results;
mod rings;
mod scanning;
mod settings;
mod toolbar;
mod treemap;
mod views;
mod widget_bodies;
mod widgets;
mod workspace;
mod workspace_bar;
mod workspace_manager;
mod workspace_menu;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use gpui_kit::Subscription;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tree::TreeState;
use gpui_kit::*;
use timberstat_core::workspace::WorkspaceLayout;
use timberstat_filesystem::{FileTree, NodeId};
use timberstat_ntfs::{SummaryProgress, VolumeSummary};
use timberstat_scanner::CancelToken;

use crate::{explore::Basis, scan::ScanSummary, search::Query};
use workspace::DragStart;

const PROGRESS_REFRESH: Duration = Duration::from_millis(200);

/// What the window is showing.
pub enum Phase {
    Idle,
    Walking {
        root: PathBuf,
        entries: Arc<AtomicU64>,
        cancel: CancelToken,
    },
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
    pub popup: Option<Entity<gpui_kit::component::menu::PopupMenu>>,
}

pub struct Shell {
    volumes: Vec<PathBuf>,
    phase: Phase,
    fast_mode: bool,
    /// Each drive's chosen scan mode, by letter.
    drive_modes: std::collections::BTreeMap<String, timberstat_core::scan_mode::ScanModeChoice>,
    /// True while a drive's disk type is being checked, before its scan has started.
    probing: bool,
    /// Whether each drive is a spinning disk, found once per session by the background probe.
    hdd_cache: std::collections::HashMap<char, bool>,
    log_dir: PathBuf,
    /// The tree from the last walk, shared by the views.
    tree: Option<Arc<FileTree>>,
    /// Absolute root used to turn tree-relative paths into file-operation paths.
    scan_root: Option<PathBuf>,
    tree_state: Option<Entity<TreeState>>,
    focus: NodeId,
    sunburst_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    sunburst_hover: Option<(NodeId, Point<Pixels>)>,
    basis: Basis,
    /// Recomputed only when the focus, basis or tree changes, never per frame.
    extension_totals: Arc<Vec<crate::extensions::ExtensionTotal>>,
    ui_scale: f32,
    layout: WorkspaceLayout,
    workspaces: workspace_manager::WorkspaceUi,
    drag_start: Option<DragStart>,
    workspace_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    widget_rooms: Rc<RefCell<HashMap<timberstat_core::workspace::WidgetKind, (f32, f32)>>>,
    menu_bar: Option<gpui_kit::Entity<gpui_kit::component::menu::AppMenuBar>>,
    menu_signature: String,
    palette: palette::PaletteUi,
    layout_dirty: bool,
    layout_save: Option<Task<()>>,
    selected: Option<NodeId>,
    menu: Option<ContextMenu>,
    search_input: Option<Entity<InputState>>,
    _search_sub: Option<Subscription>,
    query: Query,
    /// Dropping the running task cancels it, so it lives here.
    task: Option<Task<()>>,
    delete: fileops_ui::DeleteUi,
    transfer: fileops_ui::TransferUi,
    rename: fileops_ui::RenameUi,
    dup: duplicates::DupUi,
}

impl Shell {
    pub fn new(
        volumes: Vec<PathBuf>,
        log_dir: PathBuf,
        layout: WorkspaceLayout,
        ui_scale: f32,
        drive_modes: std::collections::BTreeMap<String, timberstat_core::scan_mode::ScanModeChoice>,
    ) -> Self {
        let (workspaces, layout) = workspace_manager::WorkspaceUi::restore(layout);
        Self {
            volumes,
            phase: Phase::Idle,
            fast_mode: false,
            drive_modes,
            hdd_cache: Default::default(),
            probing: false,
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
            menu_bar: None,
            menu_signature: String::new(),
            palette: palette::PaletteUi::default(),
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

    fn install_tree(&mut self, files: FileTree, scan_root: PathBuf, cx: &mut Context<Self>) {
        self.query = Query::new(self.query.text(), Some(&files));
        self.tree_state = Some(files_tree::build_tree_state(&files, &self.query, cx));
        self.tree = Some(Arc::new(files));
        self.scan_root = Some(scan_root);
        self.focus = NodeId::ROOT;
        self.refresh_extensions();
    }

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
}
