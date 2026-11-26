//! Main window layout: the top bar (toolbar.rs), the folder tree as the main area, and
//! the graphs in a tabbed pane below it. Before a tree exists, the scan's progress or result
//! fills the main area.

use gpui_kit::component::{
    button::Button, label::Label, progress::Progress, spinner::Spinner, status_bar::StatusBar,
    tag::Tag, *,
};
use gpui_kit::*;
use hmstd_ntfs::SummaryProgress;

use super::{
    Phase, Shell, fileops_ui, menu::menu_overlay, results::*, settings, toolbar, view_menu,
    widgets::*, workspace,
};

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        fileops_ui::ensure_inputs(self, window, cx);
        // GPUI's rem is 16px. Scaling it scales every rem-based size: text and most spacing.
        window.set_rem_size(px(16.0 * self.ui_scale));
        let p = Palette::from_app(cx);
        let handle = cx.entity();

        div()
            .size_full()
            .v_flex()
            .bg(p.background)
            .text_color(p.foreground)
            .child(toolbar::bar(self, p, handle.clone(), cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .child(workspace::area(self, p, handle.clone())),
            )
            .child(status_bar(self, p))
            .children(menu_overlay(self, p, handle.clone()))
            .children(settings::overlay(self, p, handle.clone()))
            .children(view_menu::overlay(self, p, handle.clone()))
            .children(fileops_ui::overlays(self, p, handle))
    }
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
                .gap_2()
                .child(
                    div()
                        .h_flex()
                        .gap_2()
                        .items_center()
                        .child(Spinner::new())
                        .child(
                            div()
                                .text_sm()
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
                        .text_xs()
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
