//! The Files, pie, Treemap and Sunburst views over a finished walk. Each takes the
//! scanned tree and the folder the user has focused on, and reads only from them.

use gpui_kit::component::{Disableable, button::Button, chart::PieChart, *};
use gpui_kit::*;

use hmstd_filesystem::{FileTree, NodeId};

use crate::search::Query;

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

/// The search box in the toolbar. Matches are shown in the tree and the graphs. A query
/// that does not parse, or a term the scan cannot answer, is explained under the box.
pub fn search_field(
    input: Option<&Entity<gpui_kit::component::input::InputState>>,
    query: &Query,
    p: Palette,
) -> Div {
    let field = div().v_flex().w(px(240.)).min_w_0().gap_1();
    let field = match input {
        Some(input) => field.child(gpui_kit::component::input::Input::new(input)),
        None => field,
    };
    let mut notes: Vec<Div> = Vec::new();
    if let Some(error) = query.error() {
        notes.push(
            div()
                .text_xs()
                .text_color(p.danger)
                .child(error.to_string()),
        );
    }
    for warning in query.warnings() {
        notes.push(div().text_xs().text_color(p.warning).child(warning.clone()));
    }
    field.children(notes)
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
