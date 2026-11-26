//! The Extensions widget: the file types under the focused folder, largest first. Each row has a
//! colour swatch, a share bar, its size and its file count. Rows are read only, so nothing here
//! is a control.

use gpui_kit::component::*;
use gpui_kit::*;

use super::{Shell, empty_state, views, widgets::*};
use crate::{
    extensions::ExtensionTotal,
    scan::{format_bytes, format_count},
};
use hmstd_core::workspace::WidgetKind;

/// Rows drawn before the list stops. The rest are counted in one line below it.
const MAX_ROWS: usize = 40;

/// The widget body for the focused folder.
pub(super) fn body(shell: &Shell, p: Palette) -> Div {
    if shell.tree.is_none() {
        return empty_state::no_tree(WidgetKind::Extensions, shell.is_scanning());
    }
    // Cached on Shell. Walking the focused subtree here would run on every frame.
    let rows = shell.extension_totals.clone();
    if rows.is_empty() {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(message_block(
                "No files here",
                "The focused folder holds no files.",
                p,
            ));
    }
    let total: u64 = rows.iter().map(|row| row.bytes).sum();
    let mut list = div().v_flex().gap_1().w_full().child(
        div()
            .text_xs()
            .text_color(p.muted_foreground)
            .child(format!(
                "{} file types · {}",
                rows.len(),
                format_bytes(total)
            )),
    );
    for (index, row) in rows.iter().take(MAX_ROWS).enumerate() {
        list = list.child(row_line(index, row, total, p));
    }
    if rows.len() > MAX_ROWS {
        list = list.child(
            div()
                .text_xs()
                .text_color(p.muted_foreground)
                .child(format!(
                    "{} more file types not shown",
                    rows.len() - MAX_ROWS
                )),
        );
    }
    list
}

/// One extension: swatch, name, share bar, size and file count.
fn row_line(index: usize, row: &ExtensionTotal, total: u64, p: Palette) -> Div {
    let share = if total > 0 {
        row.bytes as f32 / total as f32
    } else {
        0.0
    };
    let label = if row.extension.is_empty() {
        "(no extension)".to_string()
    } else {
        format!(".{}", row.extension)
    };
    let color = views::slot_color(index, false, p);
    let files = if row.files == 1 {
        "1 file".to_string()
    } else {
        format!("{} files", format_count(row.files))
    };
    div()
        .h_flex()
        .h(px(22.))
        .gap_2()
        .items_center()
        .text_sm()
        .child(div().size(px(10.)).flex_shrink_0().rounded_sm().bg(color))
        .child(
            div()
                .w(px(96.))
                .flex_shrink_0()
                .min_w_0()
                .overflow_hidden()
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h(px(8.))
                .rounded_sm()
                .bg(p.background)
                .child(div().h_full().rounded_sm().w(relative(share)).bg(color)),
        )
        .child(
            div()
                .w(px(84.))
                .flex_shrink_0()
                .text_right()
                .child(format_bytes(row.bytes)),
        )
        .child(
            div()
                .w(px(72.))
                .flex_shrink_0()
                .text_right()
                .text_xs()
                .text_color(p.muted_foreground)
                .child(files),
        )
}
