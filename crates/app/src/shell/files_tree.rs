//! The Files tab: the folder tree in the WizTree layout, built from the scanned tree and
//! filtered by the search. While a search is active only matching rows and the folders that
//! lead to them are listed, and those folders open.

use gpui_kit::component::{
    IconName,
    list::ListItem,
    tree::{TreeItem, TreeState, tree},
    *,
};
use gpui_kit::*;
use std::sync::Arc;

use hmstd_filesystem::{EntryKind, FileTree, NodeId};

use super::{Shell, widgets::*};
use crate::search::Query;
use crate::{
    explore::{self, Basis, Level, Row},
    scan::{format_bytes, format_count},
};

/// Folder levels built into the Files tree.
const TREE_LEVELS: u32 = 3;
/// Children shown per folder in the Files tree; the rest are counted, not listed.
const TREE_ROWS_PER_FOLDER: usize = 40;
/// Rows listed in total while a search is active. Matches past this are counted per folder
/// as "more not shown", so a query that matches most of a drive stays responsive.
const TREE_MAX_SEARCH_ROWS: usize = 5_000;

/// Builds the folder tree; the top folder level starts open. While a search is active, only
/// matching rows and the folders that hold them are listed, at any depth, and those open.
pub fn build_tree_state(files: &FileTree, query: &Query, cx: &mut App) -> Entity<TreeState> {
    let rows = if query.is_active() {
        let mut budget = TREE_MAX_SEARCH_ROWS;
        explore::filtered_level(
            files,
            NodeId::ROOT,
            TREE_ROWS_PER_FOLDER,
            &|node| query.shows(node),
            &|node| query.contains_hit(node),
            &mut budget,
        )
    } else {
        explore::level(files, NodeId::ROOT, TREE_LEVELS, TREE_ROWS_PER_FOLDER)
    };
    let items = tree_items(&rows, true, 0, query);
    cx.new(|cx| TreeState::new(cx).items(items))
}

fn tree_items(level: &Level, expand: bool, parent: usize, query: &Query) -> Vec<TreeItem> {
    let mut items: Vec<TreeItem> = level
        .rows
        .iter()
        .map(|row| row_item(row, expand, query))
        .collect();
    if level.hidden > 0 {
        items.push(
            TreeItem::new(
                format!("more-{parent}"),
                format!("{} more not shown", format_count(level.hidden as u64)),
            )
            .disabled(true),
        );
    }
    items
}

fn row_item(row: &Row, expand: bool, query: &Query) -> TreeItem {
    // While a search is active, a folder opens when something inside it matches, so every
    // counted match is on screen.
    let open = expand || query.contains_hit(row.child.node);
    let mut item =
        TreeItem::new(row.child.node.index().to_string(), row.child.name.clone()).expanded(open);
    if let Some(below) = &row.below {
        item = item.children(tree_items(below, false, row.child.node.index(), query));
    }
    item
}

/// Column widths shared by the header and every row, in pixels.
/// The name column never gets narrower than this; a narrow widget clips the right columns.
const COL_NAME_MIN: f32 = 110.;
const COL_PERCENT: f32 = 60.;
const COL_SIZE: f32 = 72.;
const COL_ON_DISK: f32 = 72.;
const COL_ITEMS: f32 = 56.;

/// Height of the column header above the folder tree, in pixels.
const TREE_HEADER_PX: f32 = 24.0;
/// Space between the list's left edge and the first column, in pixels.
const LINE_INSET: f32 = 8.0;
/// Extra indent per tree level, in pixels.
const INDENT_PX: f32 = 14.0;
/// Number column widths, left to right: % of parent, size, on-disk, items.
const COL_WIDTHS: [f32; 4] = [COL_PERCENT, COL_SIZE, COL_ON_DISK, COL_ITEMS];

/// One tree line: the name cell, then the four number cells. The header and every row are
/// built here, so their columns always line up.
fn line(name: Div, cells: [Div; 4], p: Palette) -> Div {
    let mut row = div()
        .h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .pl(px(LINE_INSET))
        .pr(px(LINE_INSET + super::widget_bodies::SCROLL_GUTTER_PX))
        .child(name.flex_1().min_w(px(COL_NAME_MIN)).overflow_hidden());
    for (cell, width) in cells.into_iter().zip(COL_WIDTHS) {
        row = row.child(
            cell.flex_shrink_0()
                .w(px(width))
                .px_1()
                .border_l_1()
                .border_color(p.border)
                .text_right()
                .truncate(),
        );
    }
    row
}

/// The folder tree in the WizTree layout: the name, then % of parent, size, on-disk size and
/// item count. Clicking a folder makes it the focus of the graphs below.
pub fn files_view(
    state: &Entity<TreeState>,
    files: Arc<FileTree>,
    p: Palette,
    handle: Entity<Shell>,
    height: f32,
    tooltips: bool,
) -> Div {
    let on_disk = explore::allocated_known(&files);
    // The list needs a definite pixel height. Percentage heights through the absolute boxes
    // above resolved to 0 px (measured), which drew the header and no rows.
    // Two pixels go to the box's top and bottom border.
    let list_h = (height - TREE_HEADER_PX - 2.0).max(0.0);
    let header = line(
        div().child("Folder"),
        [
            div().child("% parent"),
            div().child("Size"),
            div().child("On-disk"),
            div().child("Items"),
        ],
        p,
    )
    .absolute()
    .top_0()
    .left_0()
    .right_0()
    .h(px(TREE_HEADER_PX))
    .text_xs()
    .text_color(p.muted_foreground)
    .border_b_1()
    .border_color(p.border);
    let list = div()
        .absolute()
        .top(px(TREE_HEADER_PX))
        .left_0()
        .right_0()
        .h(px(list_h))
        .child(tree(state, move |ix, entry, selected, _window, _cx| {
            let icon = if !entry.is_folder() {
                IconName::File
            } else if entry.is_expanded() {
                IconName::FolderOpen
            } else {
                IconName::Folder
            };
            let node = entry
                .item()
                .id
                .parse::<usize>()
                .ok()
                .and_then(NodeId::from_index);
            // The line owns all horizontal spacing, so the item adds none.
            let mut row = ListItem::new(ix).selected(selected).pl(px(0.)).pr(px(0.));
            if let Some(node) = node {
                let handle = handle.clone();
                row = row.on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.tree_select(node, cx));
                });
            }
            row.child(tree_cells(
                &files,
                node,
                icon,
                on_disk,
                entry.depth(),
                p,
                RowInteraction {
                    handle: handle.clone(),
                    tooltips,
                },
            ))
        }));
    // Header and list share one bordered box, so both start at the same x.
    div()
        .relative()
        .w_full()
        .h(px(height))
        .border_1()
        .border_color(p.border)
        .rounded_lg()
        .overflow_hidden()
        .child(header)
        .child(list)
}

/// Pointer behavior shared by every row. Grouping it keeps the cell builder's arity bounded.
struct RowInteraction {
    handle: Entity<Shell>,
    tooltips: bool,
}

/// One tree row. Hovering it shows the full path, logical size and allocated size.
fn tree_cells(
    files: &FileTree,
    node: Option<NodeId>,
    icon: IconName,
    on_disk: bool,
    depth: usize,
    p: Palette,
    interaction: RowInteraction,
) -> Div {
    let row = tree_cell_row(files, node, icon, on_disk, depth, p);
    let Some(id) = node.filter(|id| files.node(*id).is_some()) else {
        return row;
    };
    let RowInteraction { handle, tooltips } = interaction;
    let bytes = explore::bytes_of(files, id, Basis::Logical);
    let path = files
        .path_of(id)
        .map_or_else(String::new, |path| path.display().to_string());
    let allocated = if on_disk {
        format_bytes(explore::bytes_of(files, id, Basis::Allocated))
    } else {
        "not recorded by this scan".to_string()
    };
    let tip = format!(
        "{path}
Size: {}
Allocated: {allocated}",
        format_bytes(bytes)
    );
    let mut wrapper = div().id(("tree-row", id.index())).w_full().on_mouse_down(
        MouseButton::Right,
        move |event: &MouseDownEvent, _, cx| {
            let at = event.position;
            handle.update(cx, |shell, cx| shell.open_menu(id, at, cx));
            cx.stop_propagation();
        },
    );
    if tooltips {
        wrapper = wrapper.tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
        });
    }
    div().w_full().child(wrapper.child(row))
}

/// One tree row's cells, aligned under the header.
fn tree_cell_row(
    files: &FileTree,
    node: Option<NodeId>,
    icon: IconName,
    on_disk: bool,
    depth: usize,
    p: Palette,
) -> Div {
    let Some(id) = node else {
        return div().child("Unknown item");
    };
    let Some(entry) = files.node(id) else {
        return div().child("Unknown item");
    };
    let bytes = explore::bytes_of(files, id, Basis::Logical);
    let share = entry.parent().and_then(|parent| {
        let whole = explore::bytes_of(files, parent, Basis::Logical);
        (whole > 0).then(|| bytes as f64 * 100.0 / whole as f64)
    });
    let share_text = share.map_or_else(|| "—".to_string(), |s| format!("{s:.1}%"));
    let items = (entry.kind() == EntryKind::Directory).then(|| {
        let totals = entry.totals();
        format_count(totals.files + totals.directories)
    });
    let on_disk_text = if on_disk {
        format_bytes(explore::bytes_of(files, id, Basis::Allocated))
    } else {
        "—".to_string()
    };
    let name = entry.name().to_string_lossy().into_owned();
    line(
        div()
            .h_flex()
            .gap_2()
            .items_center()
            .pl(px(INDENT_PX * depth as f32))
            .child(icon)
            .child(div().min_w_0().truncate().child(name)),
        [
            div().text_color(p.muted_foreground).child(share_text),
            div().child(format_bytes(bytes)),
            div().text_color(p.muted_foreground).child(on_disk_text),
            div()
                .text_color(p.muted_foreground)
                .child(items.unwrap_or_default()),
        ],
        p,
    )
    .text_sm()
}
