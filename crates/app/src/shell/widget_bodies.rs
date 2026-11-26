//! What each widget shows inside its body: the folder tree with its status line, the scan
//! summary, and the treemap, sunburst and pie for the focused folder.

use gpui_kit::component::{scroll::*, *};
use gpui_kit::*;
use hmstd_core::workspace::WidgetKind;

use super::{
    Phase, Shell, duplicates_view, empty_state, extensions_view, files_tree,
    render::summary_content, rings, treemap, views, widgets::*,
};
use crate::{explore, scan::format_count};

/// Height of the status line above the folder tree, in pixels.
const STATUS_PX: f32 = 22.0;

/// The inner size of a widget body, in pixels. Graphs size themselves from it, so they fit
/// the widget instead of being drawn at a fixed size and cut off.
#[derive(Clone, Copy, Debug)]
pub(super) struct Room {
    pub(super) w: f32,
    pub(super) h: f32,
}

/// Pixels a body loses to the 2px gap, its padding and borders, across and down. The header
/// is added on top of the vertical figure.
const CHROME_W: f32 = 2.0 * PAD_PX + 6.0;
const CHROME_H: f32 = 2.0 * PAD_PX + 6.0;
/// Space between a widget's edge and its content, in pixels.
const PAD_PX: f32 = 9.0;
/// Room kept clear on the right of scrolling content, so the scrollbar does not cover text.
pub(super) const SCROLL_GUTTER_PX: f32 = 12.0;

/// The room inside `kind`'s body: measured on the last frame, or estimated from its frame
/// before the first measurement.
fn room(shell: &Shell, kind: WidgetKind) -> Room {
    if let Some(&(w, h)) = shell.widget_rooms.borrow().get(&kind) {
        return Room { w, h };
    }
    match (shell.layout.frame(kind), shell.workspace_bounds.get()) {
        (Some(frame), Some(bounds)) => Room {
            w: (frame.w * f32::from(bounds.size.width) - CHROME_W).max(0.0),
            h: (frame.h * f32::from(bounds.size.height) - CHROME_H - super::workspace::HEADER_PX)
                .max(0.0),
        },
        // Before the first layout: a middling size, corrected on the next frame.
        _ => Room { w: 400.0, h: 300.0 },
    }
}

/// A box that fills its parent exactly. Absolute insets give a definite size, which a flex or
/// percentage height does not always give the list inside it.
fn fill() -> Div {
    div().absolute().top_0().bottom_0().left_0().right_0()
}

/// The body of one widget. Only list widgets (Summary, Extensions) scroll.
pub(super) fn body(shell: &Shell, kind: WidgetKind, p: Palette, handle: Entity<Shell>) -> Div {
    let inner = match kind {
        WidgetKind::Tree => tree(shell, p, handle),
        WidgetKind::Summary if matches!(shell.phase, Phase::Idle) => {
            empty_state::no_scan(shell, handle)
        }
        WidgetKind::Summary => summary_content(&shell.phase, p, handle),
        WidgetKind::Extensions => extensions_view::body(shell, p),
        WidgetKind::Treemap | WidgetKind::Sunburst | WidgetKind::Pie => {
            graph(shell, kind, p, handle)
        }
        WidgetKind::Duplicates => duplicates_view::body(shell, p, handle),
    };
    // The content box sits PAD_PX inside the widget. Padding on an absolute box would not move
    // absolute children, so the gap is an inset instead.
    let rooms = shell.widget_rooms.clone();
    // Measures the content box. When its size changes, the next frame lays out from it.
    let probe = canvas(
        move |bounds, window, _| {
            let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let old = rooms.borrow_mut().insert(kind, size);
            let moved =
                old.is_none_or(|(w, h)| (w - size.0).abs() > 0.5 || (h - size.1).abs() > 0.5);
            if moved {
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .bottom_0()
    .left_0()
    .right_0();
    let content = div()
        .absolute()
        .top(px(PAD_PX))
        .bottom(px(PAD_PX))
        .left(px(PAD_PX))
        .right(px(PAD_PX))
        .child(probe);
    // Graphs size themselves to the room and clip as a guard. Lists scroll only when the widget
    // is too small for them.
    match kind {
        WidgetKind::Tree | WidgetKind::Treemap | WidgetKind::Sunburst | WidgetKind::Pie => {
            fill().child(content.overflow_hidden().child(inner))
        }
        WidgetKind::Summary | WidgetKind::Extensions | WidgetKind::Duplicates => fill().child(
            content.child(
                div()
                    .id(("widget-scroll", kind as usize))
                    .size_full()
                    .overflow_y_scrollbar()
                    .child(div().pr(px(SCROLL_GUTTER_PX)).child(inner)),
            ),
        ),
    }
}

/// The folder tree with the line above it: what the scan finished, a warning for partial
/// totals, and how many items in the scan match the search.
fn tree(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let (Some(files), Some(state)) = (shell.tree.clone(), shell.tree_state.clone()) else {
        return empty_state::no_tree(WidgetKind::Tree, shell.is_scanning());
    };
    let count = shell.query.is_active().then(|| shell.query.match_count());
    div()
        .relative()
        .size_full()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(STATUS_PX))
                .child(tree_status(shell, p, count)),
        )
        .child(
            div()
                .absolute()
                .top(px(STATUS_PX + 4.0))
                .bottom_0()
                .left_0()
                .right_0()
                .child(files_tree::files_view(
                    &state,
                    files,
                    p,
                    handle,
                    (room(shell, WidgetKind::Tree).h - STATUS_PX - 4.0).max(0.0),
                    shell.menu.is_none() && shell.delete.is_closed(),
                )),
        )
}

fn tree_status(shell: &Shell, p: Palette, count: Option<usize>) -> Div {
    let (mut status, partial) = scan_status(shell);
    let available = shell.tree.as_deref().is_some_and(explore::allocated_known);
    if !available && !status.is_empty() {
        status.push_str(" · on-disk sizes need a Fast scan");
    }
    // One line. The scan text gives way first and ends in an ellipsis; the partial flag and
    // the match count keep their width.
    let mut row = div()
        .h_flex()
        .w_full()
        .min_w_0()
        .gap_3()
        .items_center()
        .text_xs()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(p.muted_foreground)
                .child(status),
        );
    if partial {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_color(p.warning)
                .child("Partial totals"),
        );
    }
    if let Some(count) = count {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_color(p.muted_foreground)
                .child(match_text(count)),
        );
    }
    row
}

/// What the last scan finished, and whether its totals are partial.
fn scan_status(shell: &Shell) -> (String, bool) {
    match &shell.phase {
        Phase::Walked(summary) => (
            format!(
                "Walk complete: {} files, {} folders",
                format_count(summary.files),
                format_count(summary.directories)
            ),
            !summary.complete,
        ),
        Phase::FastDone(summary) => (
            format!(
                "Fast read complete: {} files, {} folders",
                format_count(summary.files),
                format_count(summary.directories)
            ),
            !summary.complete,
        ),
        _ => (String::new(), false),
    }
}

fn match_text(count: usize) -> String {
    match count {
        0 => "No item matches".to_string(),
        1 => "1 item matches".to_string(),
        n => format!("{} items match", format_count(n as u64)),
    }
}

/// A graph for the focused folder. Without a tree there is nothing to draw.
fn graph(shell: &Shell, kind: WidgetKind, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(files) = shell.tree.clone() else {
        return empty_state::no_tree(kind, shell.is_scanning());
    };
    match kind {
        WidgetKind::Treemap => {
            treemap::treemap_page(&files, shell.focus, handle, p, look(shell, kind))
        }
        WidgetKind::Sunburst => rings::sunburst_page(
            &files,
            shell.focus,
            handle,
            p,
            look(shell, kind),
            shell
                .sunburst_hover
                .filter(|_| shell.menu.is_none() && shell.delete.is_closed()),
            shell.sunburst_bounds.clone(),
        ),
        _ => views::pie_card(
            &files,
            shell.focus,
            p,
            handle,
            shell.basis,
            room(shell, kind),
        ),
    }
}

/// How the treemap and sunburst draw right now.
pub(super) fn look(shell: &Shell, kind: WidgetKind) -> treemap::Look<'_> {
    treemap::Look {
        room: room(shell, kind),
        basis: shell.basis,
        query: &shell.query,
        selected: shell.selected,
        tooltips: shell.menu.is_none() && shell.delete.is_closed(),
    }
}
