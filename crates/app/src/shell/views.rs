//! The Files, pie, Treemap and Sunburst views over a finished walk. Each takes the
//! scanned tree and the folder the user has focused on, and reads only from them.

use gpui_kit::component::{
    Disableable, IconName,
    button::Button,
    chart::PieChart,
    list::ListItem,
    tree::{TreeItem, TreeState, tree},
    *,
};
use gpui_kit::*;
use std::sync::Arc;

use hmstd_filesystem::{EntryKind, FileTree, NodeId};

use crate::search::Query;

use super::{Shell, widgets::*};
use crate::{
    explore::{self, Basis, Level, Row},
    scan::{format_bytes, format_count},
};

/// Folder levels built into the Files tree.
const TREE_LEVELS: u32 = 3;
/// Children shown per folder in the Files tree; the rest are counted, not listed.
const TREE_ROWS_PER_FOLDER: usize = 40;
const PIE_SLICES: usize = 10;

/// Builds the Files tab's tree from a finished walk. The top folder level starts open.
/// Builds the folder tree. Rows whose name matches `query` are marked with a dot.
pub fn build_tree_state(files: &FileTree, query: &Query, cx: &mut App) -> Entity<TreeState> {
    let rows = explore::level(files, NodeId::ROOT, TREE_LEVELS, TREE_ROWS_PER_FOLDER);
    let items = tree_items(&rows, true, 0, query);
    cx.new(|cx| TreeState::new(cx).items(items))
}

fn tree_items(level: &Level, expand: bool, parent: usize, query: &Query) -> Vec<TreeItem> {
    // While searching, only rows that match, or lead to a match, are listed.
    let mut items: Vec<TreeItem> = level
        .rows
        .iter()
        .filter(|row| {
            !query.is_active() || query.hits(&row.child.name) || has_hit_below(row, query)
        })
        .map(|row| row_item(row, expand, query))
        .collect();
    if level.hidden > 0 && !query.is_active() {
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
    let open = expand || (query.is_active() && has_hit_below(row, query));
    let mut item =
        TreeItem::new(row.child.node.index().to_string(), row.child.name.clone()).expanded(open);
    if let Some(below) = &row.below {
        item = item.children(tree_items(below, false, row.child.node.index(), query));
    }
    item
}

/// True when some row inside this folder matches the query.
fn has_hit_below(row: &Row, query: &Query) -> bool {
    row.below.as_ref().is_some_and(|below| {
        below
            .rows
            .iter()
            .any(|child| query.hits(&child.child.name) || has_hit_below(child, query))
    })
}

/// How many shown rows of the folder tree match the query.
pub fn tree_matches(files: &FileTree, query: &Query) -> usize {
    fn count(level: &Level, query: &Query) -> usize {
        level
            .rows
            .iter()
            .map(|row| {
                usize::from(query.hits(&row.child.name))
                    + row.below.as_ref().map_or(0, |below| count(below, query))
            })
            .sum()
    }
    count(
        &explore::level(files, NodeId::ROOT, TREE_LEVELS, TREE_ROWS_PER_FOLDER),
        query,
    )
}

/// Shown on the folder-based tabs when there is no tree (no walk yet, or a Fast read).
pub fn no_tree_message(p: Palette) -> Div {
    message_block(
        "No folder tree yet",
        "Run a scan to build the folder tree. Walk and Fast scans both fill it.",
        p,
    )
}

/// Height of the folder tree, in pixels.
/// Column widths shared by the header and every row, in pixels.
const COL_PERCENT: f32 = 96.;
const COL_SIZE: f32 = 100.;
const COL_ON_DISK: f32 = 100.;
const COL_ITEMS: f32 = 84.;

/// The folder tree in the WizTree layout: the name, then % of parent, size, on-disk size and
/// item count. Clicking a folder makes it the focus of the graphs below.
pub fn files_view(
    state: &Entity<TreeState>,
    files: Arc<FileTree>,
    p: Palette,
    handle: Entity<Shell>,
) -> Div {
    let on_disk = explore::allocated_known(&files);
    let header = div()
        .h_flex()
        .px_3()
        .py_1()
        .gap_2()
        .text_xs()
        .text_color(p.muted_foreground)
        .border_b_1()
        .border_color(p.border)
        .child(div().flex_1().min_w_0().child("Folder"))
        .child(
            div()
                .w(px(COL_PERCENT))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .child("% of parent"),
        )
        .child(
            div()
                .w(px(COL_SIZE))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .child("Size"),
        )
        .child(
            div()
                .w(px(COL_ON_DISK))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .child("On-disk"),
        )
        .child(
            div()
                .w(px(COL_ITEMS))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .child("Items"),
        );
    let body = div()
        .flex_1()
        .min_h_0()
        .w_full()
        .border_1()
        .border_color(p.border)
        .rounded_lg()
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
            let mut row = ListItem::new(ix)
                .selected(selected)
                .pl(px(16.) * entry.depth() + px(12.));
            if let Some(node) = node {
                let handle = handle.clone();
                row = row.on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.tree_select(node, cx));
                });
            }
            row.child(tree_cells(&files, node, icon, on_disk, p))
        }));
    div().v_flex().flex_1().min_h_0().child(header).child(body)
}

/// One tree row. Hovering it shows the full path, logical size and allocated size.
fn tree_cells(
    files: &FileTree,
    node: Option<NodeId>,
    icon: IconName,
    on_disk: bool,
    p: Palette,
) -> Div {
    let row = tree_cell_row(files, node, icon, on_disk, p);
    let Some(id) = node.filter(|id| files.node(*id).is_some()) else {
        return row;
    };
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
    div().h_flex().flex_1().min_w_0().child(
        div()
            .id(("tree-row", id.index()))
            .flex_1()
            .min_w_0()
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
            })
            .child(row),
    )
}

/// One tree row's cells, aligned under the header.
fn tree_cell_row(
    files: &FileTree,
    node: Option<NodeId>,
    icon: IconName,
    on_disk: bool,
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
    div()
        .h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_flex()
                .gap_2()
                .child(icon)
                .child(div().truncate().child(name)),
        )
        .child(
            div()
                .w(px(COL_PERCENT))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .text_color(p.muted_foreground)
                .child(share_text),
        )
        .child(
            div()
                .w(px(COL_SIZE))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .child(format_bytes(bytes)),
        )
        .child(
            div()
                .w(px(COL_ON_DISK))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .child(on_disk_text),
        )
        .child(
            div()
                .w(px(COL_ITEMS))
                .border_l_1()
                .border_color(p.muted_foreground.opacity(0.4))
                .text_right()
                .text_color(p.muted_foreground)
                .child(items.unwrap_or_default()),
        )
}

/// One entry in a legend or a pie: name, size, and the node to drill into when it is a folder.
#[derive(Clone)]
pub(super) struct Item {
    pub(super) name: String,
    pub(super) bytes: u64,
    pub(super) node: Option<NodeId>,
    pub(super) color: Hsla,
}

/// The folder's share by item, as a pie chart with a clickable legend.
pub fn pie_card(
    files: &FileTree,
    focus: NodeId,
    p: Palette,
    handle: Entity<Shell>,
    basis: Basis,
) -> Div {
    let slices = explore::slices(files, focus, PIE_SLICES, basis);
    let items: Vec<Item> = slices
        .iter()
        .enumerate()
        .map(|(index, slice)| Item {
            name: slice.name.clone(),
            bytes: slice.bytes,
            node: slice.node.filter(|_| slice.is_dir),
            color: slot_color(index, slice.node.is_none(), p),
        })
        .collect();
    let total: u64 = items.iter().map(|item| item.bytes).sum();

    let chart = PieChart::new(items.clone())
        .value(|item| item.bytes as f32)
        .color(|item| item.color)
        .tooltip_name(|item| SharedString::from(item.name.clone()))
        .tooltip_value(move |item, _, _| {
            let share = if total > 0 {
                item.bytes as f64 * 100.0 / total as f64
            } else {
                0.0
            };
            SharedString::from(format!("{}  ({share:.1}%)", format_bytes(item.bytes)))
        })
        .inner_radius_ratio(0.6)
        .outer_radius(120.);

    div()
        .v_flex()
        .gap_3()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(section_heading("Share by item", p))
        .child(
            div()
                .h_flex()
                .gap_6()
                .items_center()
                .child(div().w(px(260.)).h(px(260.)).child(chart))
                .child(legend(items, total, p, handle)),
        )
}

pub(super) fn legend(items: Vec<Item>, total: u64, p: Palette, handle: Entity<Shell>) -> Div {
    let mut list = div().v_flex().gap_1().flex_1().min_w_0();
    for (index, item) in items.into_iter().enumerate() {
        let share = if total > 0 {
            item.bytes as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        let mut row = div()
            .id(("legend", index))
            .h_flex()
            .gap_3()
            .items_center()
            .py_1()
            .child(div().size(px(10.)).rounded_sm().bg(item.color))
            .child(div().flex_1().text_sm().child(item.name.clone()))
            .child(
                div()
                    .text_sm()
                    .text_color(p.muted_foreground)
                    .child(format!("{}  {share:.1}%", format_bytes(item.bytes))),
            );
        if let Some(node) = item.node {
            let handle = handle.clone();
            row = row.cursor_pointer().on_click(move |_, _, cx| {
                handle.update(cx, |shell, cx| shell.focus_on(node, cx));
            });
        }
        list = list.child(row);
    }
    list
}

/// Colour for a slot: chart colours in order, and muted grey for the combined "other" piece.
pub(super) fn slot_color(index: usize, is_other: bool, p: Palette) -> Hsla {
    if is_other {
        p.muted_foreground
    } else {
        p.chart[index % p.chart.len()]
    }
}

/// The search box in the toolbar. Matches are shown in the tree and the graphs.
pub fn search_field(input: Option<&Entity<gpui_kit::component::input::InputState>>) -> Div {
    let field = div().w(px(240.)).min_w_0();
    match input {
        Some(input) => field.child(gpui_kit::component::input::Input::new(input)),
        None => field,
    }
}

/// Chooses between logical and on-disk sizes. On-disk is disabled when the scan did not
/// record it, with the reason shown beside the buttons (Plan §17: reduced functions are shown).
/// The Logical / On-disk size switch. On-disk is disabled when the scan did not record it.
pub fn basis_buttons(basis: Basis, available: bool, handle: Entity<Shell>) -> Div {
    let logical_handle = handle.clone();
    let allocated_handle = handle;
    div()
        .h_flex()
        .gap_2()
        .child(
            Button::new("basis-logical")
                .outline()
                .label("Logical")
                .toggled(basis == Basis::Logical)
                .on_click(move |_, _, cx| {
                    logical_handle.update(cx, |shell, cx| shell.set_basis(Basis::Logical, cx));
                }),
        )
        .child(
            Button::new("basis-allocated")
                .outline()
                .label("On-disk")
                .toggled(basis == Basis::Allocated)
                .disabled(!available)
                .on_click(move |_, _, cx| {
                    allocated_handle.update(cx, |shell, cx| shell.set_basis(Basis::Allocated, cx));
                }),
        )
}

/// Path to the focused folder, with an Up button that moves to its parent.
pub(super) fn breadcrumb(
    files: &FileTree,
    focus: NodeId,
    handle: Entity<Shell>,
    p: Palette,
) -> Div {
    let path = files
        .path_of(focus)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    div()
        .h_flex()
        .gap_3()
        .items_center()
        .child(
            Button::new("focus-up")
                .outline()
                .label("Up")
                .disabled(focus == NodeId::ROOT)
                .on_click(move |_, _, cx| handle.update(cx, |shell, cx| shell.focus_up(cx))),
        )
        .child(div().text_sm().text_color(p.muted_foreground).child(path))
}
