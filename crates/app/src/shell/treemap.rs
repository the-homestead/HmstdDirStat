//! Treemap in the WizTree style: each folder is a box holding its children, sized by
//! logical size. Click a folder to enter it; hover any tile for its name, size and share.

use gpui_kit::component::{tooltip::Tooltip, *};
use gpui_kit::*;
use hmstd_filesystem::{FileTree, NodeId};

use super::{Shell, views::breadcrumb, widgets::*};
use crate::{
    scan::format_bytes,
    tiles::{self, Limits, Tile},
};

/// Width to height the layout is made for. The board is full width at a fixed height.
const ASPECT: f64 = 2.0;
const BOARD_HEIGHT: f32 = 480.;
/// Height of a folder's name strip, in pixels.
const HEADER_PX: f32 = 18.;
/// Tiles at least this share of the focused folder get a visible label.
const LABEL_SHARE: f64 = 0.01;
const LIMITS: Limits = Limits {
    depth: 3,
    children: 40,
    min_share: 0.0008,
};

pub fn treemap_page(files: &FileTree, focus: NodeId, handle: Entity<Shell>, p: Palette) -> Div {
    let tiles = tiles::nested(files, focus, ASPECT, LIMITS);
    let board = div()
        .relative()
        .w_full()
        .h(px(BOARD_HEIGHT))
        .rounded_lg()
        .overflow_hidden()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .children(tile_views(&tiles, None, &handle, p));
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

            let mut view = div()
                .id(("tile", tile.node.index()))
                .absolute()
                .left(relative(tile.rect.x as f32))
                .top(relative(tile.rect.y as f32))
                .w(relative(tile.rect.w as f32))
                .h(relative(tile.rect.h as f32))
                .overflow_hidden()
                .border_1()
                .border_color(p.background)
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));

            if tile.is_dir {
                let node = tile.node;
                let handle = handle.clone();
                view = view.cursor_pointer().on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.focus_on(node, cx));
                    // The folder's parent also listens for clicks; only the innermost folder should act.
                    cx.stop_propagation();
                });
            }

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
                            .children(tile_views(&tile.children, Some(color), handle, p)),
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
