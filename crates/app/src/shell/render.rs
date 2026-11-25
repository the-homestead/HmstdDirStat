//! Layout for the main window: sidebar, visualization tabs, phase content, status bar.

use gpui_kit::component::{
    button::Button,
    label::Label,
    scroll::*,
    spinner::Spinner,
    status_bar::StatusBar,
    switch::Switch,
    tab::{Tab, TabBar},
    tag::Tag,
    *,
};
use gpui_kit::*;
use hmstd_ntfs::VolumeSummary;

use super::{Phase, Shell, View, widgets::*};
use crate::{scan::ScanSummary, theme::toggle_theme};

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::from_app(cx);
        let handle = cx.entity();

        div()
            .size_full()
            .h_flex()
            .bg(p.background)
            .text_color(p.foreground)
            .child(sidebar(self, p, handle.clone(), cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .v_flex()
                    .min_w_0()
                    .child(tab_header(self.view, p, handle.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .p_6()
                            .overflow_y_scrollbar()
                            .child(content(self, p, handle)),
                    )
                    .child(status_bar(self, p)),
            )
    }
}

fn sidebar(shell: &Shell, p: Palette, handle: Entity<Shell>, cx: &App) -> Div {
    let busy = shell.is_busy();
    let dark = cx.theme().is_dark();

    let mut volumes = div().v_flex().gap_2().w_full();
    for (index, drive) in shell.volumes.iter().enumerate() {
        let target = drive.clone();
        let handle = handle.clone();
        volumes = volumes.child(
            Button::new(format!("volume-{index}"))
                .label(drive.display().to_string())
                .w_full()
                .disabled(busy)
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.choose_volume(target.clone(), cx));
                }),
        );
    }

    let fast = shell.fast_mode;
    let mode_caption = if fast {
        "Reads the MFT directly. Windows asks for administrator permission on each scan. Best on large NVMe drives and spinning disks."
    } else {
        "Walks folders on parallel threads. No administrator rights needed."
    };
    let mode_handle = handle.clone();
    let mode = Switch::new("fast-mode")
        .checked(fast)
        .label("Fast NTFS read")
        .on_click(move |checked, _, cx| {
            mode_handle.update(cx, |shell, cx| shell.set_fast_mode(*checked, cx));
        });

    div()
        .v_flex()
        .w(px(272.))
        .h_full()
        .flex_shrink_0()
        .p_4()
        .gap_5()
        .border_r_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(
            div()
                .v_flex()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .child("HmstdDirStat"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(p.muted_foreground)
                        .child("Disk usage"),
                ),
        )
        .child(
            div()
                .v_flex()
                .gap_2()
                .child(section_heading("Scan mode", p))
                .child(mode)
                .child(
                    div()
                        .text_xs()
                        .text_color(p.muted_foreground)
                        .child(mode_caption),
                ),
        )
        .child(
            div()
                .v_flex()
                .gap_2()
                .child(section_heading("Volumes", p))
                .child(volumes),
        )
        .child(div().flex_1())
        .child(
            Button::new("toggle-theme")
                .outline()
                .w_full()
                .label(if dark {
                    "Switch to light theme"
                } else {
                    "Switch to dark theme"
                })
                .on_click(|_, _, cx| toggle_theme(cx)),
        )
}

fn tab_header(view: View, p: Palette, handle: Entity<Shell>) -> Div {
    let tabs = TabBar::new("visualizations")
        .underline()
        .selected_index(view.index())
        .on_click(move |index: &usize, _, cx| {
            let next = View::from_index(*index);
            handle.update(cx, |shell, cx| shell.set_view(next, cx));
        })
        .child(Tab::new().label("Summary"))
        .child(Tab::new().label("Treemap"))
        .child(Tab::new().label("Sunburst"));

    div()
        .h_flex()
        .px_6()
        .pt_4()
        .border_b_1()
        .border_color(p.border)
        .child(tabs)
}

fn content(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    match shell.view {
        View::Summary => summary_content(&shell.phase, p, handle),
        View::Treemap => message_block(
            "Treemap view",
            "Not built yet. It will draw each folder as an area sized by its disk usage.",
            p,
        ),
        View::Sunburst => message_block(
            "Sunburst view",
            "Not built yet. It will draw folders as rings, sized by disk usage.",
            p,
        ),
    }
}

fn summary_content(phase: &Phase, p: Palette, handle: Entity<Shell>) -> Div {
    match phase {
        Phase::Idle => message_block(
            "Choose a volume",
            "Pick a drive in the sidebar. Walk mode works without administrator rights. Fast mode asks Windows for permission, then reads the file table.",
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
                    div()
                        .text_sm()
                        .text_color(p.muted_foreground)
                        .child(entries_text(
                            entries.load(std::sync::atomic::Ordering::Relaxed),
                        )),
                )
                .child(cancel)
        }
        Phase::Elevated { drive, granted } => {
            let (title, detail) = if *granted {
                (
                    format!("Administrator access granted. Reading {}", drive.display()),
                    "Reading the file table. The result replaces this message when the read finishes.",
                )
            } else {
                (
                    format!("Waiting for administrator approval for {}", drive.display()),
                    "Windows is asking for permission to run HmstdDirStat as administrator. Approve it to start the read.",
                )
            };
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
                                .child(title),
                        ),
                )
                .child(div().text_sm().text_color(p.muted_foreground).child(detail))
        }
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

fn walked_result(summary: &ScanSummary, p: Palette) -> Div {
    let stats = [
        Stat {
            label: "Files",
            value: crate::scan::format_count(summary.files),
        },
        Stat {
            label: "Folders",
            value: crate::scan::format_count(summary.directories),
        },
        Stat {
            label: "Logical size",
            value: crate::scan::format_bytes(summary.logical_bytes),
        },
        Stat {
            label: "Physical size",
            value: bytes_or_na(summary.physical_bytes),
        },
        Stat {
            label: "Scan time",
            value: format!("{:.2} s", summary.elapsed.as_secs_f64()),
        },
        Stat {
            label: "Unreadable",
            value: crate::scan::format_count(summary.issues),
        },
    ];
    let mut page = result_page(
        &summary.root.display().to_string(),
        &format!("Walk · {}", summary.method),
        summary.complete,
        p,
    )
    .child(stat_grid(&stats, p));
    if !summary.issue_paths.is_empty() {
        let mut list = div()
            .v_flex()
            .gap_1()
            .child(section_heading("Unreadable folders", p));
        for line in &summary.issue_paths {
            list = list.child(div().text_sm().text_color(p.warning).child(line.clone()));
        }
        page = page.child(list);
    }
    page
}

fn fast_result(summary: &VolumeSummary, p: Palette) -> Div {
    let stats = [
        Stat {
            label: "Files",
            value: crate::scan::format_count(summary.files),
        },
        Stat {
            label: "Folders",
            value: crate::scan::format_count(summary.directories),
        },
        Stat {
            label: "Logical size",
            value: crate::scan::format_bytes(summary.logical),
        },
        Stat {
            label: "Physical size",
            value: crate::scan::format_bytes(summary.physical),
        },
        Stat {
            label: "Read time",
            value: format!("{:.2} s", summary.total_secs),
        },
        Stat {
            label: "Orphans",
            value: crate::scan::format_count(summary.orphans),
        },
    ];
    result_page(
        &drive_label(&summary.volume),
        "Fast · NTFS file table",
        summary.complete,
        p,
    )
    .child(stat_grid(&stats, p))
}

/// Header line for a result: where it came from, how it was read, and whether it is complete.
fn result_page(title: &str, method: &str, complete: bool, p: Palette) -> Div {
    let completeness = if complete {
        Tag::success().child("Complete")
    } else {
        Tag::warning().child("Partial")
    };
    div()
        .v_flex()
        .gap_4()
        .child(
            div()
                .h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(completeness),
        )
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child(method.to_string()),
        )
}

/// `//./C:` from the helper becomes `C:\` for display.
fn drive_label(volume: &str) -> String {
    format!("{}\\", volume.trim_start_matches("//./"))
}

fn status_bar(shell: &Shell, p: Palette) -> StatusBar {
    let text = match &shell.phase {
        Phase::Idle => "Ready".to_string(),
        Phase::Walking { root, .. } => format!("Walking {}", root.display()),
        Phase::Elevated { drive, granted } => {
            if *granted {
                format!("Admin access granted: reading {}", drive.display())
            } else {
                format!("Waiting for administrator approval: {}", drive.display())
            }
        }
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
