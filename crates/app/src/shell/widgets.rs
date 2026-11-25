//! Shared visual pieces: the colour palette, stat cards, section headings, and the
//! text shown for each phase.

use gpui_kit::component::*;
use gpui_kit::*;

use crate::scan::format_count;

/// Colours copied out of the theme once per frame, so no borrow of `cx` is held.
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub surface: Hsla,
    pub muted_foreground: Hsla,
    pub border: Hsla,
    pub warning: Hsla,
}

impl Palette {
    pub fn from_app(cx: &App) -> Self {
        let colors = &cx.theme().colors;
        Self {
            background: colors.background,
            foreground: colors.foreground,
            surface: colors.list,
            muted_foreground: colors.muted_foreground,
            border: colors.border,
            warning: colors.warning,
        }
    }
}

/// One labelled value in a result card.
pub struct Stat {
    pub label: &'static str,
    pub value: String,
}

/// Formats a byte count for display, or "n/a" when it is unknown.
pub fn bytes_or_na(value: Option<u64>) -> String {
    value.map_or_else(|| "n/a".to_string(), crate::scan::format_bytes)
}

/// A grid of stat cards.
pub fn stat_grid(stats: &[Stat], p: Palette) -> Div {
    let mut grid = div().h_flex().flex_wrap().gap_3();
    for stat in stats {
        grid = grid.child(
            div()
                .v_flex()
                .min_w(px(168.))
                .p_4()
                .gap_1()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(
                    div()
                        .text_xs()
                        .text_color(p.muted_foreground)
                        .child(stat.label),
                )
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(stat.value.clone()),
                ),
        );
    }
    grid
}

/// Small uppercase heading above a group of controls or results.
pub fn section_heading(title: &str, p: Palette) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.muted_foreground)
        .child(title.to_uppercase())
}

/// Centred message for empty and busy states.
pub fn message_block(title: &str, detail: &str, p: Palette) -> Div {
    div()
        .v_flex()
        .gap_2()
        .py_8()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child(detail.to_string()),
        )
}

/// Human-readable entries count, e.g. `12,345 entries so far`.
pub fn entries_text(count: u64) -> String {
    format!("{} entries so far", format_count(count))
}
