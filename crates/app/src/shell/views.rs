//! The Files, pie, Treemap and Sunburst views over a finished walk. Each takes the
//! scanned tree and the folder the user has focused on, and reads only from them.

use gpui_kit::component::{Disableable, button::Button, chart::PieChart, *};
use gpui_kit::*;

use hmstd_filesystem::{FileTree, NodeId};

use super::{Shell, widgets::*};
use crate::{
    explore::{self, Basis},
    scan::format_bytes,
};

const PIE_SLICES: usize = 10;

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
    room: super::widget_bodies::Room,
) -> Div {
    let slices = explore::slices(files, focus, PIE_SLICES, basis);
    let below = (room.h - PIE_HEADING_PX).max(0.0);
    let side = below.min(room.w * 0.5).max(60.0);
    let legend_rows = legend_rows(below);
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
        .outer_radius(side / 2.0 - 2.0);

    // The widget is the card, so the pie draws straight into its body.
    div()
        .v_flex()
        .gap_3()
        .child(section_heading("Share by item", p))
        .child(
            div()
                .h_flex()
                .gap_6()
                .items_center()
                .child(div().flex_shrink_0().w(px(side)).h(px(side)).child(chart))
                .child(legend(items, total, legend_rows, p, handle)),
        )
}

/// Height of the "Share by item" heading and the gap under it, in pixels.
const PIE_HEADING_PX: f32 = 36.;
/// Height of one legend row, in pixels.
const LEGEND_ROW_PX: f32 = 28.;

/// Legend rows that fit in `height` pixels.
pub(super) fn legend_rows(height: f32) -> usize {
    ((height / LEGEND_ROW_PX).floor() as usize).max(1)
}

/// The legend, cut to `max_rows`. When rows are left out, the last line says how many.
pub(super) fn legend(
    items: Vec<Item>,
    total: u64,
    max_rows: usize,
    p: Palette,
    handle: Entity<Shell>,
) -> Div {
    let shown = if items.len() > max_rows {
        max_rows.saturating_sub(1).max(1)
    } else {
        items.len()
    };
    let hidden = items.len() - shown;
    let mut list = div().v_flex().gap_1().flex_1().min_w_0();
    for (index, item) in items.into_iter().take(shown).enumerate() {
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
    if hidden > 0 {
        list = list.child(
            div()
                .text_xs()
                .text_color(p.muted_foreground)
                .child(format!("{hidden} more")),
        );
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
