//! The Files tab: the folder tree in the WizTree layout. Rows come from the scanned tree, cut
//! down by the search and the Hidden setting, sorted by the chosen column, and built only for
//! the folders that are open. A closed folder holds one placeholder row, so its toggle shows.

use std::collections::HashSet;
use std::sync::Arc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{
    IconName,
    list::ListItem,
    tree::{TreeItem, TreeState, tree},
    *,
};
use gpui_kit::*;

use timberstat_filesystem::{EntryKind, FileTree, NodeId};

use super::{Shell, widgets::*};
use crate::search::Query;
use crate::{
    explore::{self, Basis, Level, Row},
    extensions::extension_of,
    scan::{format_bytes, format_count},
    tree_model::{self, Column, FileKind, Sort},
};

/// Children shown per folder in the Files tree; the rest are counted, not listed.
pub const TREE_ROWS_PER_FOLDER: usize = 40;
/// Folder levels that open when a tree first loads.
pub const TREE_OPEN_LEVELS: u32 = 2;
/// Rows listed in total while a search is active. Matches past this are counted per folder
/// as "more not shown", so a query that matches most of a drive stays responsive.
const TREE_MAX_SEARCH_ROWS: usize = 5_000;

/// The settings the Files tab is built from, borrowed from the shell.
pub struct TreeView<'a> {
    pub query: &'a Query,
    pub sort: Sort,
    pub show_hidden: bool,
    /// Folders the user has opened. Only these get their children built.
    pub expanded: &'a HashSet<NodeId>,
}

/// Every row of the Files tab for `view`. While a search is active, only matching rows and the
/// folders that hold them are listed, at any depth, and those folders open.
pub fn tree_items(files: &FileTree, view: &TreeView) -> Vec<TreeItem> {
    let searching = view.query.is_active();
    let keep = |node: NodeId| {
        (view.show_hidden || !tree_model::is_hidden(files, node))
            && (!searching || view.query.shows(node))
    };
    let descend = |node: NodeId| {
        view.expanded.contains(&node) || (searching && view.query.contains_hit(node))
    };
    let mut budget = if searching {
        TREE_MAX_SEARCH_ROWS
    } else {
        usize::MAX
    };
    let level = explore::filtered_level(
        files,
        NodeId::ROOT,
        TREE_ROWS_PER_FOLDER,
        view.sort,
        &keep,
        &descend,
        &mut budget,
    );
    level_items(files, &level, view, 0)
}

fn level_items(files: &FileTree, level: &Level, view: &TreeView, parent: usize) -> Vec<TreeItem> {
    let mut items: Vec<TreeItem> = level
        .rows
        .iter()
        .map(|row| row_item(files, row, view))
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

fn row_item(files: &FileTree, row: &Row, view: &TreeView) -> TreeItem {
    let node = row.child.node;
    let mut item = TreeItem::new(node.index().to_string(), row.child.name.clone())
        .expanded(row.below.is_some());
    if let Some(below) = &row.below {
        item = item.children(level_items(files, below, view, node.index()));
    } else if row.child.is_dir && !view.query.is_active() && files.children(node).next().is_some() {
        // A closed folder needs one child to show its toggle. The real rows are built on open.
        item = item
            .child(TreeItem::new(format!("pending-{}", node.index()), "Loading…").disabled(true));
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
const COL_MODIFIED: f32 = 118.;
const COL_EXTENSION: f32 = 56.;

/// Height of the column header above the folder tree, in pixels.
const TREE_HEADER_PX: f32 = 24.0;
/// Space between the list's left edge and the first column, in pixels.
const LINE_INSET: f32 = 8.0;
/// Extra indent per tree level, in pixels.
const INDENT_PX: f32 = 14.0;
/// Number column widths, left to right: % of parent, size, on-disk, items, modified, extension.
const COL_WIDTHS: [f32; 6] = [
    COL_PERCENT,
    COL_SIZE,
    COL_ON_DISK,
    COL_ITEMS,
    COL_MODIFIED,
    COL_EXTENSION,
];

/// One tree line: the name cell, then the six number cells. The header and every row are built
/// here, so their columns always line up.
fn line<E: Styled + IntoElement>(name: E, cells: [E; 6], p: Palette) -> Div {
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

/// A header cell that sorts the tree by `column` when clicked. The active column shows its
/// direction.
fn sort_header(
    label: &str,
    column: Column,
    sort: Sort,
    handle: &Entity<Shell>,
) -> impl Styled + IntoElement {
    let mark = match (sort.column == column, sort.descending) {
        (true, true) => " ▼",
        (true, false) => " ▲",
        _ => "",
    };
    let handle = handle.clone();
    div()
        .id(("tree-sort", column as usize))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, _, cx| {
            handle.update(cx, |shell, cx| shell.set_tree_sort(column, cx));
        })
        .child(format!("{label}{mark}"))
}

/// The folder tree in the WizTree layout: the name, then % of parent, size, on-disk size, item
/// count, modification date and extension. Clicking a folder makes it the focus of the graphs.
pub fn files_view(
    state: &Entity<TreeState>,
    files: Arc<FileTree>,
    p: Palette,
    handle: Entity<Shell>,
    height: f32,
    tooltips: bool,
    sort: Sort,
) -> Div {
    let on_disk = explore::allocated_known(&files);
    // The list needs a definite pixel height. Percentage heights through the absolute boxes
    // above resolved to 0 px (measured), which drew the header and no rows.
    // Two pixels go to the box's top and bottom border.
    let list_h = (height - TREE_HEADER_PX - 2.0).max(0.0);
    let header = line(
        sort_header("Folder", Column::Name, sort, &handle),
        [
            sort_header("% parent", Column::Percent, sort, &handle),
            sort_header("Size", Column::Size, sort, &handle),
            sort_header("On-disk", Column::OnDisk, sort, &handle),
            sort_header("Items", Column::Items, sort, &handle),
            sort_header("Modified", Column::Modified, sort, &handle),
            sort_header("Ext", Column::Extension, sort, &handle),
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
            // The line owns all horizontal spacing, so the item adds none.
            let mut row = ListItem::new(ix).selected(selected).pl(px(0.)).pr(px(0.));
            let node = entry
                .item()
                .id
                .parse::<usize>()
                .ok()
                .and_then(NodeId::from_index);
            let Some(node) = node else {
                // The "more not shown" line and the loading placeholder have no node.
                let label = entry.item().label.to_string();
                return row.child(line(
                    div().text_color(p.muted_foreground).child(label),
                    [div(), div(), div(), div(), div(), div()],
                    p,
                ));
            };
            let icon = icon_for(&files, node, entry.is_expanded());
            let handle_click = handle.clone();
            row = row.on_click(move |_, _, cx| {
                handle_click.update(cx, |shell, cx| shell.tree_select(node, cx));
            });
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

/// A folder gets the open or closed folder icon by its state. A file gets the icon of its kind.
fn icon_for(files: &FileTree, node: NodeId, expanded: bool) -> AnyElement {
    let Some(entry) = files.node(node) else {
        return IconName::File.into();
    };
    if entry.kind() == EntryKind::Directory {
        let folder = if expanded {
            IconName::FolderOpen
        } else {
            IconName::Folder
        };
        return folder.into();
    }
    let glyph = match tree_model::file_kind(entry.name()) {
        FileKind::Archive => Glyph::FileArchive,
        FileKind::Image => Glyph::FileImage,
        FileKind::Audio => Glyph::FileMusic,
        FileKind::Video => Glyph::FilePlay,
        FileKind::Code => Glyph::FileCode,
        FileKind::Document => Glyph::FileText,
        FileKind::Sheet => Glyph::FileSpreadsheet,
        FileKind::Other => return IconName::File.into(),
    };
    Icon::new(glyph).into()
}

/// Pointer behavior shared by every row. Grouping it keeps the cell builder's arity bounded.
struct RowInteraction {
    handle: Entity<Shell>,
    tooltips: bool,
}

/// One tree row. Hovering it shows the full path, logical size, allocated size and the
/// modification time.
fn tree_cells(
    files: &FileTree,
    id: NodeId,
    icon: AnyElement,
    on_disk: bool,
    depth: usize,
    p: Palette,
    interaction: RowInteraction,
) -> Div {
    let row = tree_cell_row(files, id, icon, on_disk, depth, p);
    let Some(entry) = files.node(id) else {
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
    let modified = entry.metadata().modified_unix_secs.map_or_else(
        || "not recorded by this scan".to_string(),
        |secs| format!("{} UTC", tree_model::format_modified(secs)),
    );
    let tip = format!(
        "{path}\nSize: {}\nAllocated: {allocated}\nModified: {modified}",
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
    id: NodeId,
    icon: AnyElement,
    on_disk: bool,
    depth: usize,
    p: Palette,
) -> Div {
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
    let modified_text = entry
        .metadata()
        .modified_unix_secs
        .map_or_else(|| "—".to_string(), tree_model::format_modified);
    let ext = extension_of(entry.name());
    let extension_text = if entry.kind() == EntryKind::File && !ext.is_empty() {
        format!(".{ext}")
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
            div().text_color(p.muted_foreground).child(modified_text),
            div().text_color(p.muted_foreground).child(extension_text),
        ],
        p,
    )
    .text_sm()
}
