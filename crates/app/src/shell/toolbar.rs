//! The toolbar under the menu row. Three zones on one 44 px line:
//! - left: the app mark, the drive picker and the scan mode
//! - centre: the query field, with its errors in a popover under it so the bar never grows
//! - right: the size basis
//!
//! Drives, modes and sizes are segmented groups, so the active choice reads at a glance. Layout,
//! theme, settings and every other action live in the menu row above.

use gpui_kit::component::{
    IconName,
    button::{Button, ButtonGroup},
    input::Input,
    *,
};
use gpui_kit::*;

use super::scanning::drive_letter;
use super::{Shell, widgets::*};
use crate::{explore, explore::Basis, platform, search::Query};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use timberstat_core::scan_mode::ScanModeChoice;

/// Height of the bar in pixels. The menu row sits above it; overlays open under both.
pub(super) const BAR_PX: f32 = 44.0;

/// The whole bar.
pub(super) fn bar(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
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
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_flex()
                .justify_center()
                .child(search(shell, p)),
        )
        .child(basis(shell, handle.clone()))
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
                .child(timberstat_core::config::APP_NAME),
        )
}

fn divider(p: Palette) -> Div {
    div().flex_shrink_0().w(px(1.)).h(px(20.)).bg(p.border)
}

/// One segment per drive. The drive that holds the current scan is selected; clicking any
/// drive scans it with the chosen mode.
/// One button per drive. Left-click scans it with the drive's mode. Right-click shows that mode
/// and lets it be changed; the choice is saved for next time.
fn drives(shell: &Shell, handle: Entity<Shell>) -> Div {
    let busy = shell.is_busy();
    let current = shell.scan_root.clone();
    let mut row = div().h_flex().gap_1();
    for (index, drive) in shell.volumes.iter().enumerate() {
        let letter = drive_letter(drive);
        let selected = current.as_ref().is_some_and(|root| root.starts_with(drive));
        let choice = shell.drive_mode(letter);
        let label = drive.display().to_string();
        let pick = drive.clone();
        let click_handle = handle.clone();
        let menu_handle = handle.clone();
        row = row.child(
            div()
                .id(("drive", index))
                .context_menu(move |menu, _, _| {
                    let mut menu = menu.label(format!("Scan mode for {letter}:"));
                    if !platform::FAST_NTFS_AVAILABLE {
                        menu = menu.label(platform::FAST_NTFS_UNAVAILABLE);
                    }
                    for (name, option) in [
                        ("Auto (file table for hard disks)", ScanModeChoice::Auto),
                        ("Fast NTFS (file table)", ScanModeChoice::Fast),
                        ("Walk (folders)", ScanModeChoice::Walk),
                    ] {
                        let target = menu_handle.clone();
                        menu =
                            menu.item(PopupMenuItem::new(name).checked(choice == option).on_click(
                                move |_, _, cx| {
                                    target.update(cx, |shell, cx| {
                                        shell.set_drive_mode(letter, option, cx)
                                    });
                                },
                            ));
                    }
                    menu
                })
                .child(
                    Button::new(("drive-button", index))
                        .outline()
                        .small()
                        .compact()
                        .label(label.trim_end_matches('\\').to_string())
                        .tooltip(format!(
                            "Scan {label}. Right-click to change its scan mode."
                        ))
                        .selected(selected)
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            click_handle
                                .update(cx, |shell, cx| shell.choose_volume(pick.clone(), cx));
                        }),
                ),
        );
    }
    row
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
