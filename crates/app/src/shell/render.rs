//! Layout for the main window: sidebar, visualization tabs, phase content, status bar.

use gpui_kit::component::{
    button::Button,
    label::Label,
    progress::Progress,
    scroll::*,
    spinner::Spinner,
    status_bar::StatusBar,
    switch::Switch,
    tab::{Tab, TabBar},
    tag::Tag,
    *,
};
use gpui_kit::*;
use hmstd_ntfs::SummaryProgress;

use super::{
    Phase, Shell, View, menu::menu_overlay, results::*, rings, treemap, views, widgets::*,
};
use crate::theme::toggle_theme;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
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
                            .child(content(self, p, handle.clone())),
                    )
                    .child(status_bar(self, p)),
            )
            .children(menu_overlay(self, p, handle))
    }
}

fn sidebar(shell: &Shell, p: Palette, handle: Entity<Shell>, cx: &App) -> Div {
    let log_dir = shell.log_dir.clone();
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
            Button::new("open-logs")
                .outline()
                .w_full()
                .label("Open log folder")
                .tooltip(log_dir.display().to_string())
                .on_click(move |_, _, _| {
                    if let Err(error) = crate::open::open_folder(&log_dir) {
                        tracing::error!(%error, "could not open the log folder");
                    }
                }),
        )
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
        .child(Tab::new().label("Files"))
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
    let files = shell.tree.as_deref();
    match shell.view {
        View::Summary => {
            let mut page =
                div()
                    .v_flex()
                    .gap_6()
                    .child(summary_content(&shell.phase, p, handle.clone()));
            if let (Phase::Walked(_) | Phase::FastDone(_), Some(files)) = (&shell.phase, files) {
                page = page.child(views::pie_card(files, shell.focus, p, handle, shell.basis));
            }
            page
        }
        View::Files => match (&shell.tree_state, files) {
            (Some(state), Some(files)) => {
                let count = shell
                    .query
                    .is_active()
                    .then(|| views::tree_matches(files, &shell.query));
                div()
                    .v_flex()
                    .gap_3()
                    .child(views::search_bar(shell.search_input.as_ref(), count, p))
                    .child(views::files_view(state, p))
            }
            _ => views::no_tree_message(p),
        },
        View::Treemap => match files {
            Some(files) => {
                let count = shell.query.is_active().then(|| {
                    treemap::matches_in_view(files, shell.focus, shell.basis, &shell.query)
                });
                viz_chrome(
                    shell,
                    p,
                    handle.clone(),
                    count,
                    treemap::treemap_page(files, shell.focus, handle, p, look(shell)),
                )
            }
            None => views::no_tree_message(p),
        },
        View::Sunburst => match files {
            Some(files) => {
                let count = shell
                    .query
                    .is_active()
                    .then(|| rings::matches_in_view(files, shell.focus, shell.basis, &shell.query));
                viz_chrome(
                    shell,
                    p,
                    handle.clone(),
                    count,
                    rings::sunburst_page(
                        files,
                        shell.focus,
                        handle,
                        p,
                        look(shell),
                        // No hover card while a menu is open: the two would sit on top of each other.
                        shell.sunburst_hover.filter(|_| shell.menu.is_none()),
                        shell.sunburst_bounds.clone(),
                    ),
                )
            }
            None => views::no_tree_message(p),
        },
    }
}

/// The size toggle and search box above a view drawn by size or name. `count` is how many
/// shown items match, or `None` when search is off.
fn viz_chrome(
    shell: &Shell,
    p: Palette,
    handle: Entity<Shell>,
    count: Option<usize>,
    page: Div,
) -> Div {
    let available = shell
        .tree
        .as_deref()
        .is_some_and(crate::explore::allocated_known);
    div()
        .v_flex()
        .gap_3()
        .child(views::basis_toggle(shell.basis, available, handle, p))
        .child(views::search_bar(shell.search_input.as_ref(), count, p))
        .child(page)
}

/// How the treemap and sunburst draw right now.
fn look(shell: &Shell) -> treemap::Look<'_> {
    treemap::Look {
        basis: shell.basis,
        query: &shell.query,
        selected: shell.selected,
        tooltips: shell.menu.is_none(),
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

/// `//./C:` from the helper becomes `C:\` for display.
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
