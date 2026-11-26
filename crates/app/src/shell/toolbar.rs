//! The top bar. Three zones on one 44 px line:
//! - left: the app mark, the drive picker and the scan mode
//! - centre: the query field, with its errors in a popover under it so the bar never grows
//! - right: the size basis, then icon buttons for layout, theme and settings
//!
//! Drives and modes are segmented groups, so the active choice reads at a glance. Rare actions
//! (the log folder) live in Settings.

use gpui_kit::component::{
    IconName,
    button::{Button, ButtonGroup, ButtonVariants},
    input::Input,
    *,
};
use gpui_kit::*;

use super::{Shell, settings, view_menu, widgets::*};
use crate::{explore, explore::Basis, search::Query, theme::toggle_theme};

/// Height of the bar in pixels. The overlays (View, Settings) open just under it.
pub(super) const BAR_PX: f32 = 44.0;

/// The whole bar.
pub(super) fn bar(shell: &Shell, p: Palette, handle: Entity<Shell>, cx: &App) -> Div {
    div()
        .h(px(BAR_PX))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap_3()
        .px_3()
        .border_b_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(brand(p))
        .child(drives(shell, handle.clone()))
        .child(divider(p))
        .child(mode(shell, handle.clone()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_flex()
                .justify_center()
                .child(search(shell, p)),
        )
        .child(basis(shell, handle.clone()))
        .child(divider(p))
        .child(actions(handle, cx))
}

fn brand(p: Palette) -> Div {
    div()
        .h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(Icon::new(IconName::HardDrive).text_color(p.chart[0]))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(hmstd_core::config::APP_NAME),
        )
}

fn divider(p: Palette) -> Div {
    div().flex_shrink_0().w(px(1.)).h(px(20.)).bg(p.border)
}

/// One segment per drive. The drive that holds the current scan is selected; clicking any
/// drive scans it with the chosen mode.
fn drives(shell: &Shell, handle: Entity<Shell>) -> ButtonGroup {
    let busy = shell.is_busy();
    let current = shell.scan_root.clone();
    let volumes = shell.volumes.clone();
    let mut group = ButtonGroup::new("drives").outline().small().compact();
    for (index, drive) in volumes.iter().enumerate() {
        let selected = current.as_ref().is_some_and(|root| root.starts_with(drive));
        let label = drive.display().to_string();
        group = group.child(
            Button::new(("drive", index))
                .label(label.trim_end_matches('\\').to_string())
                .tooltip(format!("Scan {label}"))
                .selected(selected)
                .disabled(busy),
        );
    }
    group.on_click(move |clicked, _, cx| {
        if let Some(drive) = clicked.first().and_then(|ix| volumes.get(*ix)).cloned() {
            handle.update(cx, |shell, cx| shell.choose_volume(drive, cx));
        }
    })
}

/// Walk (no elevation) or Fast (reads the NTFS file table; Windows asks for permission when a
/// drive is scanned, never when this is switched).
fn mode(shell: &Shell, handle: Entity<Shell>) -> ButtonGroup {
    let fast = shell.fast_mode;
    ButtonGroup::new("scan-mode")
        .outline()
        .small()
        .compact()
        .disabled(shell.is_busy())
        .child(
            Button::new("mode-walk")
                .label("Walk")
                .tooltip("Walk folders. No administrator rights needed.")
                .selected(!fast),
        )
        .child(
            Button::new("mode-fast")
                .label("Fast NTFS")
                .tooltip("Read the NTFS file table. Windows asks for permission when you scan.")
                .selected(fast),
        )
        .on_click(move |clicked, _, cx| {
            let fast = clicked.first() == Some(&1);
            handle.update(cx, |shell, cx| shell.set_fast_mode(fast, cx));
        })
}

/// The query field. Errors and warnings float under it instead of pushing the bar taller.
fn search(shell: &Shell, p: Palette) -> Div {
    let mut field = div().relative().w_full().max_w(px(520.)).min_w(px(160.));
    if let Some(input) = shell.search_input.as_ref() {
        field = field.child(
            Input::new(input)
                .small()
                .cleanable(true)
                .prefix(Icon::new(IconName::Search).text_color(p.muted_foreground)),
        );
    }
    match notes(&shell.query, p) {
        Some(notes) => field.child(deferred(notes).with_priority(1)),
        None => field,
    }
}

fn notes(query: &Query, p: Palette) -> Option<Div> {
    let mut lines: Vec<Div> = Vec::new();
    if let Some(error) = query.error() {
        lines.push(div().text_color(p.danger).child(error.to_string()));
    }
    for warning in query.warnings() {
        lines.push(div().text_color(p.warning).child(warning.clone()));
    }
    if lines.is_empty() {
        return None;
    }
    Some(
        div()
            .absolute()
            .top(px(30.))
            .left_0()
            .right_0()
            .v_flex()
            .gap_1()
            .px_2()
            .py_1()
            .text_xs()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .shadow_md()
            .children(lines),
    )
}

/// Logical or on-disk sizes. On-disk is disabled, with the reason in its tooltip, when the
/// scan did not record allocated sizes (Plan §17: reduced functions are shown).
fn basis(shell: &Shell, handle: Entity<Shell>) -> ButtonGroup {
    let available = shell.tree.as_deref().is_some_and(explore::allocated_known);
    let current = shell.basis;
    ButtonGroup::new("basis")
        .outline()
        .small()
        .compact()
        .child(
            Button::new("basis-logical")
                .label("Logical")
                .tooltip("Sizes as files report them")
                .selected(current == Basis::Logical),
        )
        .child(
            Button::new("basis-allocated")
                .label("On-disk")
                .tooltip(if available {
                    "Space the files take on disk"
                } else {
                    "On-disk sizes need a Fast NTFS scan"
                })
                .selected(current == Basis::Allocated)
                .disabled(!available),
        )
        .on_click(move |clicked, _, cx| {
            let basis = if clicked.first() == Some(&1) {
                Basis::Allocated
            } else {
                Basis::Logical
            };
            handle.update(cx, |shell, cx| shell.set_basis(basis, cx));
        })
}

/// Icon buttons: widget layout, theme, settings.
fn actions(handle: Entity<Shell>, cx: &App) -> Div {
    let dark = cx.theme().is_dark();
    div()
        .h_flex()
        .flex_shrink_0()
        .gap_1()
        .child(view_menu::view_button(handle.clone()))
        .child(
            Button::new("toggle-theme")
                .ghost()
                .small()
                .icon(if dark { IconName::Sun } else { IconName::Moon })
                .tooltip(if dark {
                    "Switch to the light theme"
                } else {
                    "Switch to the dark theme"
                })
                .on_click(|_, _, cx| toggle_theme(cx)),
        )
        .child(settings::settings_button(handle))
}
