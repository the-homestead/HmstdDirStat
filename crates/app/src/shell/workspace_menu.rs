//! Saved-workspace controls inside the View menu. Every listed preset is backed by widgets in
//! this build; custom actions call the timberstat-core document operations through Shell.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::Input,
    *,
};
use gpui_kit::*;
use timberstat_core::workspace::saved::{WORKSPACE_PRESETS, WorkspacePreset};

use super::{Shell, widgets::*};

pub(super) fn section(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let workspaces = &shell.workspaces;
    let document = workspaces.document();
    let active_custom = document.is_active_custom();
    let writable = workspaces.can_write();
    let modified = document.is_modified();
    let active = if modified {
        format!("{} · modified", document.active)
    } else {
        document.active.clone()
    };

    let mut section = div().v_flex().gap_1().child(
        div()
            .h_flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(section_heading("Workspaces", p))
            .child(div().text_xs().text_color(p.muted_foreground).child(active)),
    );
    for preset in WORKSPACE_PRESETS {
        section = section.child(preset_row(
            preset,
            document.active == preset.name(),
            p,
            handle.clone(),
        ));
    }

    section = section
        .child(div().h(px(1.)).my_1().bg(p.border))
        .child(section_heading("Custom layouts", p));
    if document.custom.is_empty() {
        section = section.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(p.muted_foreground)
                .child("No custom workspaces saved yet."),
        );
    }
    for (index, workspace) in document.custom.iter().enumerate() {
        let name = workspace.name.clone();
        let selected = document.active == workspace.name;
        let switch = handle.clone();
        let mut row = div()
            .id(("workspace-custom", index))
            .h_flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_sm()
            .text_sm()
            .cursor_pointer()
            .hover(|style| style.bg(p.background))
            .child(div().w(px(12.)).child(if selected { "✓" } else { " " }))
            .child(div().flex_1().child(name.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(p.muted_foreground)
                    .child(format!("{} widgets", workspace.layout.widgets.len())),
            )
            .on_click(move |_, _, cx| {
                switch.update(cx, |shell, cx| shell.switch_workspace(&name, cx));
            });
        if selected {
            row = row.bg(p.background);
        }
        section = section.child(row);
    }

    if let Some(input) = workspaces.name_input() {
        section = section.child(
            div().px_1().pt_2().child(
                Input::new(input)
                    .small()
                    .aria_label("Custom workspace name"),
            ),
        );
    }
    let save_as = handle.clone();
    let update = handle.clone();
    let restore = handle.clone();
    let rename = handle.clone();
    let delete = handle.clone();
    section = section
        .child(
            div().px_1().child(
                Button::new("workspace-save-as")
                    .primary()
                    .small()
                    .label("Save current layout as")
                    .tooltip("Create a custom workspace using the name field")
                    .disabled(!writable)
                    .on_click(move |_, _, cx| {
                        save_as.update(cx, |shell, cx| shell.save_workspace_as(cx));
                    }),
            ),
        )
        .child(
            div()
                .h_flex()
                .gap_1()
                .px_1()
                .child(
                    Button::new("workspace-update")
                        .outline()
                        .small()
                        .label("Update")
                        .tooltip("Replace the active custom snapshot with this layout")
                        .disabled(!writable || !active_custom || !modified)
                        .on_click(move |_, _, cx| {
                            update.update(cx, |shell, cx| shell.update_workspace(cx));
                        }),
                )
                .child(
                    Button::new("workspace-restore")
                        .outline()
                        .small()
                        .label("Restore")
                        .tooltip("Discard layout changes and restore the active saved snapshot")
                        .disabled(!modified)
                        .on_click(move |_, _, cx| {
                            restore.update(cx, |shell, cx| shell.restore_workspace(cx));
                        }),
                )
                .child(
                    Button::new("workspace-rename")
                        .outline()
                        .small()
                        .label("Rename")
                        .tooltip("Rename the active custom workspace to the name field")
                        .disabled(!writable || !active_custom)
                        .on_click(move |_, _, cx| {
                            rename.update(cx, |shell, cx| shell.rename_workspace(cx));
                        }),
                )
                .child(
                    Button::new("workspace-delete")
                        .danger()
                        .small()
                        .label("Delete")
                        .tooltip("Delete the active custom workspace and restore Disk overview")
                        .disabled(!writable || !active_custom)
                        .on_click(move |_, _, cx| {
                            delete.update(cx, |shell, cx| shell.delete_workspace(cx));
                        }),
                ),
        );

    if let Some(error) = workspaces.error() {
        section = section.child(
            div()
                .p_2()
                .rounded_sm()
                .border_1()
                .border_color(p.danger)
                .text_xs()
                .text_color(p.danger)
                .child(error.to_string()),
        );
    } else if let Some(notice) = workspaces.notice() {
        section = section.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(p.chart[2])
                .child(notice.to_string()),
        );
    }
    section.child(
        div()
            .px_2()
            .text_xs()
            .text_color(p.muted_foreground)
            .child("Current layout is remembered. Update or Save as changes a named snapshot."),
    )
}

fn preset_row(
    preset: WorkspacePreset,
    selected: bool,
    p: Palette,
    handle: Entity<Shell>,
) -> Stateful<Div> {
    let name = preset.name();
    let mut row = div()
        .id(("workspace-preset", preset as usize))
        .h_flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(p.background))
        .child(div().w(px(12.)).child(if selected { "✓" } else { " " }))
        .child(
            div().v_flex().child(div().text_sm().child(name)).child(
                div()
                    .text_xs()
                    .text_color(p.muted_foreground)
                    .child(preset.description()),
            ),
        )
        .on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| shell.switch_workspace(name, cx));
        });
    if selected {
        row = row.bg(p.background);
    }
    row
}
