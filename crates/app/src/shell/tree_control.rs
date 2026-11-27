//! The Files tab's state on the shell: rebuilding the tree from its view settings, the sort and
//! Hidden toggles, folders opening and closing, and a refresh that keeps the view across a
//! rescan.

use std::ffi::OsString;
use std::path::PathBuf;

use gpui_kit::component::tree::{TreeEvent, TreeState};
use gpui_kit::*;
use timberstat_filesystem::NodeId;

use super::{Shell, files_tree};
use crate::tree_model::{self, Column};

/// The Files tab's view, kept by name so it survives a rescan, when node indexes change.
#[derive(Debug, Clone, Default)]
pub struct PendingView {
    /// The folder the view belongs to. It is applied only to a scan of the same folder.
    pub root: PathBuf,
    pub expanded: Vec<Vec<OsString>>,
    pub focus: Vec<OsString>,
    pub selected: Option<Vec<OsString>>,
}

impl Shell {
    /// The view settings the Files tab is built from.
    fn tree_view(&self) -> files_tree::TreeView<'_> {
        files_tree::TreeView {
            query: &self.query,
            sort: self.tree_sort,
            show_hidden: self.tree_show_hidden,
            expanded: &self.tree_expanded,
        }
    }

    /// Rebuilds the Files tree from the current tree and view settings. Open folders, the sort,
    /// the Hidden setting, the search and the selected row all carry over.
    pub fn rebuild_tree(&mut self, cx: &mut Context<Self>) {
        let Some(files) = self.tree.clone() else {
            return;
        };
        let items = files_tree::tree_items(&files, &self.tree_view());
        match self.tree_state.clone() {
            Some(state) => {
                let keep = state
                    .read(cx)
                    .selected_item()
                    .and_then(|item| item.id.to_string().parse::<usize>().ok());
                state.update(cx, |tree, cx| {
                    tree.set_items(items, cx);
                    if let Some(node) = keep {
                        select_node(tree, node, cx);
                    }
                });
            }
            None => {
                let keep = self.tree_restore_select.take().map(NodeId::index);
                let state = cx.new(|cx| TreeState::new(cx).items(items));
                if let Some(node) = keep {
                    state.update(cx, |tree, cx| select_node(tree, node, cx));
                }
                self.tree_sub = Some(cx.subscribe(
                    &state,
                    |shell: &mut Shell, _state, event: &TreeEvent, cx| {
                        shell.on_tree_event(event, cx);
                    },
                ));
                self.tree_state = Some(state);
            }
        }
        cx.notify();
    }

    /// A folder opened or closed in the tree. The tree is rebuilt, so the opened folder gets its
    /// rows and a closed one drops them.
    fn on_tree_event(&mut self, event: &TreeEvent, cx: &mut Context<Self>) {
        let (id, open) = match event {
            TreeEvent::Expanded(id) => (id, true),
            TreeEvent::Collapsed(id) => (id, false),
        };
        if let Some(node) = id
            .to_string()
            .parse::<usize>()
            .ok()
            .and_then(NodeId::from_index)
        {
            if open {
                self.tree_expanded.insert(node);
            } else {
                self.tree_expanded.remove(&node);
            }
        }
        self.rebuild_tree(cx);
    }

    /// A header was clicked: sort by that column, or flip the direction if it is the sort column.
    pub fn set_tree_sort(&mut self, column: Column, cx: &mut Context<Self>) {
        self.tree_sort = self.tree_sort.clicked(column);
        self.rebuild_tree(cx);
    }

    /// Shows or hides hidden files (the Windows hidden attribute, or a dot name).
    pub fn toggle_tree_hidden(&mut self, cx: &mut Context<Self>) {
        self.tree_show_hidden = !self.tree_show_hidden;
        self.rebuild_tree(cx);
    }

    /// Rescans the scanned folder and keeps the Files view: open folders, the focus, the
    /// selected row, the sort and the Hidden setting. Used by the in-tree Refresh and the
    /// Refresh command.
    pub fn refresh_scan(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let (Some(files), Some(root)) = (self.tree.clone(), self.scan_root.clone()) else {
            return;
        };
        let selected = self
            .tree_state
            .clone()
            .and_then(|state| {
                state
                    .read(cx)
                    .selected_item()
                    .and_then(|item| item.id.to_string().parse::<usize>().ok())
            })
            .and_then(NodeId::from_index)
            .map(|node| tree_model::name_chain(&files, node));
        self.pending_view = Some(PendingView {
            root: root.clone(),
            expanded: self
                .tree_expanded
                .iter()
                .map(|&node| tree_model::name_chain(&files, node))
                .collect(),
            focus: tree_model::name_chain(&files, self.focus),
            selected,
        });
        self.choose_volume(root, cx);
    }

    /// Applies a view captured before a rescan, once the new tree is in. Names that are gone
    /// are dropped; the rest of the view stays.
    pub(super) fn apply_pending_view(
        &mut self,
        files: &timberstat_filesystem::FileTree,
        root: &std::path::Path,
    ) {
        let Some(pending) = self
            .pending_view
            .take()
            .filter(|pending| pending.root == root)
        else {
            return;
        };
        self.tree_expanded = pending
            .expanded
            .iter()
            .filter_map(|chain| tree_model::resolve_chain(files, chain))
            .collect();
        self.focus = tree_model::resolve_chain(files, &pending.focus).unwrap_or(NodeId::ROOT);
        self.tree_restore_select = pending
            .selected
            .as_deref()
            .and_then(|chain| tree_model::resolve_chain(files, chain));
    }
}

/// Selects the row for `node` in a rebuilt tree, if the node is still listed.
fn select_node(tree: &mut TreeState, node: usize, cx: &mut Context<TreeState>) {
    let key = node.to_string();
    let found = (0..)
        .map_while(|ix| tree.entry(ix))
        .position(|entry| entry.item().id == key.as_str());
    if let Some(ix) = found {
        tree.set_selected_index(Some(ix), cx);
    }
}
