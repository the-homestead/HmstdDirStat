//! Selection, navigation and menu actions on the shell: the size basis, click and
//! double-click, the right-click menu, and the clipboard and Explorer hand-offs.

use gpui_kit::*;
use std::path::PathBuf;
use timberstat_filesystem::{EntryKind, NodeId};

use super::{ContextMenu, Shell, rings};
use crate::explore::Basis;

impl Shell {
    /// A tree row was clicked: a folder becomes the focus of the graphs, a file is selected.
    pub fn tree_select(&mut self, node: NodeId, cx: &mut Context<Self>) {
        let is_dir = self
            .tree
            .as_deref()
            .and_then(|files| files.node(node))
            .is_some_and(|entry| entry.kind() == EntryKind::Directory);
        if is_dir {
            self.focus_on(node, cx);
        } else {
            self.menu = None;
            self.selected = Some(node);
            cx.notify();
        }
    }

    pub fn set_basis(&mut self, basis: Basis, cx: &mut Context<Self>) {
        if self.basis != basis {
            self.basis = basis;
            self.refresh_extensions();
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
        self.menu = Some(ContextMenu {
            node,
            at,
            popup: None,
        });
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

    /// The absolute path of a node in the current tree, if it has one.
    pub fn path_of_node(&self, node: NodeId) -> Option<PathBuf> {
        let relative = self.tree.as_deref()?.path_of(node)?;
        let scan_root = self.scan_root.as_ref()?;
        Some(timberstat_fileops::tree_base(scan_root).join(relative))
    }
}
