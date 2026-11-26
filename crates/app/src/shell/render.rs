//! Main window layout: a toolbar with every control, the folder tree as the main area, and
//! the graphs in a tabbed pane below it. Before a tree exists, the scan's progress or result
//! fills the main area.

use gpui_kit::component::{
    button::Button, label::Label, progress::Progress, spinner::Spinner, status_bar::StatusBar,
    switch::Switch, tag::Tag, *,
};
use gpui_kit::*;
use hmstd_ntfs::SummaryProgress;

use super::{
    Phase, Shell, menu::menu_overlay, results::*, view_menu, views, widgets::*, workspace,
};
use crate::{explore, theme::toggle_theme};

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let p = Palette::from_app(cx);
        let handle = cx.entity();

        div()
            .size_full()
            .v_flex()
            .bg(p.background)
            .text_color(p.foreground)
            .child(toolbar(self, p, handle.clone(), cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .child(workspace::area(self, p, handle.clone())),
            )
            .child(status_bar(self, p))
            .children(menu_overlay(self, p, handle.clone()))
            .children(view_menu::overlay(self, p, handle))
    }
}

/// The one row of controls. Volumes and scan mode sit on the left; the size switch, search,
/// log folder and theme sit on the right. Every control is outlined so it reads as a control.
fn toolbar(shell: &Shell, p: Palette, handle: Entity<Shell>, cx: &App) -> Div {
    let busy = shell.is_busy();
    let dark = cx.theme().is_dark();
    let log_dir = shell.log_dir.clone();
    let available = shell.tree.as_deref().is_some_and(explore::allocated_known);

    let mut volumes = div().h_flex().gap_2();
    for (index, drive) in shell.volumes.iter().enumerate() {
        let target = drive.clone();
        let handle = handle.clone();
        volumes = volumes.child(
            Button::new(format!("volume-{index}"))
                .outline()
                .label(drive.display().to_string())
                .disabled(busy)
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.choose_volume(target.clone(), cx));
                }),
        );
    }

    let fast = shell.fast_mode;
    let mode_handle = handle.clone();
    let mode = Switch::new("fast-mode")
        .checked(fast)
        .label("Fast NTFS read")
        .on_click(move |checked, _, cx| {
            mode_handle.update(cx, |shell, cx| shell.set_fast_mode(*checked, cx));
        });

    div()
        .h_flex()
        .px_4()
        .py_3()
        .gap_4()
        .items_center()
        .border_b_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::BOLD)
                .child("HmstdDirStat"),
        )
        .child(volumes)
        .child(mode)
        .child(div().flex_1())
        .child(views::search_field(shell.search_input.as_ref()))
        .child(views::basis_buttons(shell.basis, available, handle.clone()))
        .child(
            Button::new("open-logs")
                .outline()
                .label("Log folder")
                .tooltip(log_dir.display().to_string())
                .on_click(move |_, _, _| {
                    if let Err(error) = crate::open::open_folder(&log_dir) {
                        tracing::error!(%error, "could not open the log folder");
                    }
                }),
        )
        .child(view_menu::view_button(handle.clone()))
        .child(
            Button::new("toggle-theme")
                .outline()
                .label(if dark { "Light theme" } else { "Dark theme" })
                .on_click(|_, _, cx| toggle_theme(cx)),
        )
}

pub(super) fn summary_content(phase: &Phase, p: Palette, handle: Entity<Shell>) -> Div {
    match phase {
        Phase::Idle => message_block(
            "Choose a volume",
            "Pick a drive in the toolbar. Walk mode works without administrator rights. Fast mode asks Windows for permission, then reads the file table.",
            p,
        ),
        Phase::Walking { root, entries, .. } => {
            let cancel = Button::new("cancel-walk")
                .outline()
                .label("Cancel scan")
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.cancel_walk(cx));
                });
            div()
                .v_flex()
                .gap_4()
                .child(
                    div()
                        .h_flex()
                        .gap_3()
                        .items_center()
                        .child(Spinner::new())
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Walking {}", root.display())),
                        ),
                )
                .child(
                    Progress::new("walk-progress")
                        .loading(true)
                        .accessibility_label("Scanning folders"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(p.muted_foreground)
                        .child(entries_text(
                            entries.load(std::sync::atomic::Ordering::Relaxed),
                        )),
                )
                .child(cancel)
        }
        Phase::Elevated {
            drive,
            granted,
            stage,
        } => elevated_view(drive, *granted, *stage, p),
        Phase::Walked(summary) => walked_result(summary, p),
        Phase::FastDone(summary) => fast_result(summary, p),
        Phase::Cancelled(root) => message_block(
            "Scan cancelled",
            &format!(
                "{} was cancelled. No partial result is shown.",
                root.display()
            ),
            p,
        ),
        Phase::Failed { drive, message } => {
            message_block(&format!("Could not scan {drive}"), message, p)
        }
    }
}

/// `//./C:` from the helper becomes `C:\\` for display.
pub(super) fn drive_label(volume: &str) -> String {
    format!("{}\\", volume.trim_start_matches("//./"))
}

fn status_bar(shell: &Shell, p: Palette) -> StatusBar {
    let text = match &shell.phase {
        Phase::Idle => "Ready".to_string(),
        Phase::Walking { root, .. } => format!("Walking {}", root.display()),
        Phase::Elevated {
            drive,
            granted,
            stage,
        } => match stage {
            _ if !*granted => format!("Waiting for administrator approval: {}", drive.display()),
            Some(SummaryProgress::Reading { done, total }) => format!(
                "Reading {}: {} of {} MFT records",
                drive.display(),
                crate::scan::format_count(*done),
                crate::scan::format_count(*total)
            ),
            Some(SummaryProgress::Building) => {
                format!("Building the folder tree: {}", drive.display())
            }
            None => format!("Administrator access granted: {}", drive.display()),
        },
        Phase::Walked(summary) => format!("Walk finished: {}", summary.root.display()),
        Phase::FastDone(summary) => format!("Fast read finished: {}", drive_label(&summary.volume)),
        Phase::Cancelled(_) => "Cancelled".to_string(),
        Phase::Failed { .. } => "Scan failed".to_string(),
    };
    let mode = if shell.fast_mode {
        Tag::primary().child("Fast NTFS")
    } else {
        Tag::secondary().child("Walk")
    };
    let _ = p;
    StatusBar::new().left(Label::new(text)).right(mode)
}
