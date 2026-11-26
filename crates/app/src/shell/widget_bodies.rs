//! What each widget shows inside its body: the folder tree with its status line, the scan
//! summary, and the treemap, sunburst and pie for the focused folder.

use gpui_kit::component::{scroll::*, *};
use gpui_kit::*;
use hmstd_core::workspace::WidgetKind;

use super::{
    Phase, Shell, extensions_view, files_tree, render::summary_content, rings, treemap, views,
    widgets::*,
};
use crate::{explore, scan::format_count};

/// The body of one widget. Only list widgets (Summary, Extensions) scroll.
pub(super) fn body(shell: &Shell, kind: WidgetKind, p: Palette, handle: Entity<Shell>) -> Div {
    let inner = match kind {
        WidgetKind::Tree => tree(shell, p, handle),
        WidgetKind::Summary => summary_content(&shell.phase, p, handle),
        WidgetKind::Extensions => extensions_view::body(shell, p),
        WidgetKind::Treemap | WidgetKind::Sunburst | WidgetKind::Pie => {
            graph(shell, kind, p, handle)
        }
    };
    let frame = div().flex_1().min_h_0().w_full().p_2();
    // Lists can outgrow their widget, so they scroll. Graphs fit the widget and clip instead of
    // scrolling, so nothing is scrollable by default.
    match kind {
        WidgetKind::Tree | WidgetKind::Treemap | WidgetKind::Sunburst | WidgetKind::Pie => {
            frame.child(div().size_full().overflow_hidden().child(inner))
        }
        WidgetKind::Summary | WidgetKind::Extensions => {
            frame.child(div().size_full().overflow_y_scrollbar().child(inner))
        }
    }
}

/// The folder tree with the line above it: what the scan finished, a warning for partial
/// totals, and how many shown items match the search.
fn tree(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let (Some(files), Some(state)) = (shell.tree.clone(), shell.tree_state.clone()) else {
        return views::no_tree_message(p);
    };
    let count = shell
        .query
        .is_active()
        .then(|| files_tree::tree_matches(&files, &shell.query));
    div()
        .size_full()
        .v_flex()
        .gap_2()
        .child(tree_status(shell, p, count))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .child(files_tree::files_view(&state, files, p, handle)),
        )
}

fn tree_status(shell: &Shell, p: Palette, count: Option<usize>) -> Div {
    let (mut status, partial) = scan_status(shell);
    let available = shell.tree.as_deref().is_some_and(explore::allocated_known);
    if !available && !status.is_empty() {
        status.push_str(" · on-disk sizes need a Fast scan");
    }
    let mut row = div()
        .h_flex()
        .gap_3()
        .items_center()
        .child(div().text_xs().text_color(p.muted_foreground).child(status));
    if partial {
        row = row.child(
            div()
                .text_xs()
                .text_color(p.warning)
                .child("PARTIAL: some folders could not be read, so totals are incomplete."),
        );
    }
    row = row.child(div().flex_1());
    if let Some(count) = count {
        row = row.child(
            div()
                .text_xs()
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
        0 => "No shown item matches".to_string(),
        1 => "1 shown item matches".to_string(),
        n => format!("{n} shown items match"),
    }
}

/// A graph for the focused folder. Without a tree there is nothing to draw.
fn graph(shell: &Shell, kind: WidgetKind, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(files) = shell.tree.clone() else {
        return views::no_tree_message(p);
    };
    match kind {
        WidgetKind::Treemap => treemap::treemap_page(&files, shell.focus, handle, p, look(shell)),
        WidgetKind::Sunburst => rings::sunburst_page(
            &files,
            shell.focus,
            handle,
            p,
            look(shell),
            shell.sunburst_hover.filter(|_| shell.menu.is_none()),
            shell.sunburst_bounds.clone(),
        ),
        _ => views::pie_card(&files, shell.focus, p, handle, shell.basis),
    }
}

/// How the treemap and sunburst draw right now.
pub(super) fn look(shell: &Shell) -> treemap::Look<'_> {
    treemap::Look {
        basis: shell.basis,
        query: &shell.query,
        selected: shell.selected,
        tooltips: shell.menu.is_none(),
    }
}
