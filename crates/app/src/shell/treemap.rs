//! Treemap in the WizTree style: each folder is a box holding its children, sized by the
//! chosen basis. Click selects a tile, double-click enters a folder, right-click opens the
//! tile menu, and hover shows name, size and share. A search dims everything but matches.

use gpui_kit::component::{tooltip::Tooltip, *};
use gpui_kit::*;
use timberstat_filesystem::{FileTree, NodeId};

use super::{Shell, views::breadcrumb, widgets::*};
use crate::{
    explore::Basis,
    scan::format_bytes,
    search::Query,
    tiles::{self, Limits, Tile},
};

/// Height the breadcrumb and the gap under it take above a graph, in pixels.
pub(super) const CRUMB_PX: f32 = 40.;
/// Height of a folder's name strip, in pixels.
const HEADER_PX: f32 = 18.;
/// Tiles at least this share of the focused folder get a visible label.
const LABEL_SHARE: f64 = 0.01;
/// Opacity of tiles and arcs that are not part of a search match.
pub const DIMMED: f32 = 0.25;

/// How the treemap and sunburst draw: the size basis, the search, the selection and whether
/// hover tooltips show.
pub struct Look<'a> {
    pub basis: Basis,
    pub query: &'a Query,
    pub selected: Option<NodeId>,
    pub tooltips: bool,
    /// The room inside the widget body; the graph sizes itself to fit it.
    pub room: super::widget_bodies::Room,
}

fn limits(basis: Basis) -> Limits {
    Limits {
        basis,
        depth: 3,
        children: 40,
        min_share: 0.0008,
    }
}

pub fn treemap_page(
    files: &FileTree,
    focus: NodeId,
    handle: Entity<Shell>,
    p: Palette,
    look: Look<'_>,
) -> Div {
    let room = look.room;
    // The breadcrumb and gap take the top; the board gets the rest of the widget.
    let board_h = (room.h - CRUMB_PX).max(40.0);
    let aspect = (room.w.max(40.0) / board_h).clamp(0.25, 8.0);
    let tiles = tiles::nested(files, focus, aspect as _, limits(look.basis));
    let board = div()
        .relative()
        .w_full()
        .h(px(board_h))
        .rounded_lg()
        .overflow_hidden()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .children(tile_views(&tiles, None, &handle, p, &look));
    div()
        .v_flex()
        .gap_3()
        .child(breadcrumb(files, focus, handle.clone(), p))
        .child(board)
}

/// Builds the tiles for one level. Top-level tiles take a colour from their slot; tiles
/// inside a folder share the folder's colour so each folder reads as one group.
fn tile_views(
    tiles: &[Tile],
    group: Option<Hsla>,
    handle: &Entity<Shell>,
    p: Palette,
    look: &Look<'_>,
) -> Vec<Stateful<Div>> {
    tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            let color = group.unwrap_or_else(|| group_color(index));
            let label = format!("{}  {}", tile.name, format_bytes(tile.bytes));
            let tip = format!(
                "{}\n{}  ·  {:.1}% of this folder",
                tile.name,
                format_bytes(tile.bytes),
                tile.share * 100.0
            );
            let is_selected = look.selected == Some(tile.node);
            let is_hit = look.query.hits(tile.node);
            let dimmed = look.query.is_active() && !look.query.tile_hits(tile);
            let border = if is_selected {
                p.foreground
            } else if is_hit {
                p.warning
            } else {
                p.background
            };

            let mut view = div()
                .id(("tile", tile.node.index()))
                .absolute()
                .left(relative(tile.rect.x as f32))
                .top(relative(tile.rect.y as f32))
                .w(relative(tile.rect.w as f32))
                .h(relative(tile.rect.h as f32))
                .overflow_hidden()
                .border_1()
                .border_color(border)
                .cursor_pointer();
            if look.tooltips {
                view = view.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));
            }
            if dimmed {
                view = view.opacity(DIMMED);
            }

            // Each tile handles its own clicks and stops them reaching the folder around it,
            // so the innermost tile under the pointer is the one that acts.
            let node = tile.node;
            let click_handle = handle.clone();
            let menu_handle = handle.clone();
            view = view
                .on_click(move |event: &ClickEvent, _, cx| {
                    let count = event.click_count();
                    click_handle.update(cx, |shell, cx| shell.click_node(node, count, cx));
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
                    let at = event.position;
                    menu_handle.update(cx, |shell, cx| shell.open_menu(node, at, cx));
                    cx.stop_propagation();
                });

            if tile.children.is_empty() {
                view = view.bg(color.opacity(0.8)).p_1();
                if tile.share >= LABEL_SHARE {
                    view = view.child(
                        div()
                            .text_xs()
                            .text_color(p.background)
                            .truncate()
                            .child(label),
                    );
                }
            } else {
                view = view
                    .bg(color.opacity(0.3))
                    .child(
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(px(0.))
                            .right(px(0.))
                            .h(px(HEADER_PX))
                            .px_1()
                            .bg(color)
                            .text_xs()
                            .text_color(p.background)
                            .truncate()
                            .child(label),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(HEADER_PX))
                            .left(px(0.))
                            .right(px(0.))
                            .bottom(px(0.))
                            .children(tile_views(&tile.children, Some(color), handle, p, look)),
                    );
            }
            view
        })
        .collect()
}

/// A distinct hue per top-level item. Golden-angle steps keep neighbouring folders far apart.
fn group_color(index: usize) -> Hsla {
    let hue = (index as f32 * 0.618_034) % 1.0;
    hsla(hue, 0.55, 0.55, 1.0)
}
