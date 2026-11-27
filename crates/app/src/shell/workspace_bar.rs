//! The workspace switcher in the bottom status bar. A ghost xsmall button shows the active
//! workspace and opens a dropdown of presets and custom workspaces, with a mark on the active
//! one. "Manage workspaces…" opens the management dialog.

use gpui_kit::component::{
    button::{Button, ButtonVariants, DropdownButton},
    menu::PopupMenuItem,
    *,
};
use gpui_kit::*;
use timberstat_core::workspace::saved::WORKSPACE_PRESETS;

use super::Shell;

/// The switcher button. It is rebuilt with the status bar, so its label is always current.
pub(super) fn switcher(shell: &Shell, handle: Entity<Shell>) -> DropdownButton {
    let document = shell.workspaces.document();
    let active = document.active.clone();
    let label = if document.is_modified() {
        format!("{active} · modified")
    } else {
        active.clone()
    };
    let mut names: Vec<String> = WORKSPACE_PRESETS
        .iter()
        .map(|preset| preset.name().to_string())
        .collect();
    names.extend(document.custom.iter().map(|saved| saved.name.clone()));

    DropdownButton::new("workspace-switch")
        .button(
            Button::new("workspace-switch-trigger")
                .ghost()
                .xsmall()
                .label(label),
        )
        .dropdown_menu(move |mut menu, _, _| {
            for name in names.clone() {
                let handle = handle.clone();
                let mark = if name == active { "✓ " } else { "   " };
                let target = name.clone();
                menu = menu.item(PopupMenuItem::new(format!("{mark}{name}")).on_click(
                    move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.switch_workspace(&target, cx));
                    },
                ));
            }
            let manage = handle.clone();
            menu.separator()
                .item(
                    PopupMenuItem::new("Manage workspaces…").on_click(move |_, window, cx| {
                        manage.update(cx, |shell, cx| shell.open_manage_workspaces(window, cx));
                    }),
                )
        })
}
