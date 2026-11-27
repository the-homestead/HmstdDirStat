//! The menu bar: File, View, Tools and Help. Each label opens a gpui-kit `PopupMenu` under
//! itself, with checkmarks for the active workspace and shown widgets. Search, drives and scan
//! modes stay in the toolbar below it.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    menu::{PopupMenu, PopupMenuItem},
    *,
};
use gpui_kit::*;
use timberstat_core::workspace::{WidgetKind, saved::WORKSPACE_PRESETS};

use super::{Shell, widgets::*};

/// Height of the menu row in pixels: the gpui-kit TitleBar height. Popups open under both rows.
pub(super) const MENU_PX: f32 = 34.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    File,
    View,
    Tools,
    Help,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::File, Kind::View, Kind::Tools, Kind::Help];

    fn label(self) -> &'static str {
        match self {
            Kind::File => "File",
            Kind::View => "View",
            Kind::Tools => "Tools",
            Kind::Help => "Help",
        }
    }
}

/// The popup that is open from the menu row, built on the first render after it opens.
pub(super) struct OpenMenu {
    kind: Kind,
    at: Point<Pixels>,
    popup: Option<Entity<PopupMenu>>,
}

impl Shell {
    /// Opens `kind` under its label, or closes it when it is already open.
    pub(super) fn open_bar_menu(&mut self, kind: Kind, at: Point<Pixels>, cx: &mut Context<Self>) {
        let same = self.bar_menu.as_ref().is_some_and(|m| m.kind == kind);
        self.bar_menu = (!same).then_some(OpenMenu {
            kind,
            at,
            popup: None,
        });
        self.workspace_panel = false;
        cx.notify();
    }

    pub(super) fn close_bar_menu(&mut self, cx: &mut Context<Self>) {
        if self.bar_menu.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn open_workspace_panel(&mut self, cx: &mut Context<Self>) {
        self.bar_menu = None;
        self.workspace_panel = true;
        cx.notify();
    }
}

/// The menu row: one ghost label per menu, the open one selected.
pub(super) fn bar(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let open = shell.bar_menu.as_ref().map(|m| m.kind);
    let mut row = div()
        .h_flex()
        .items_center()
        .gap_1()
        .px_2()
        .text_color(p.foreground);
    for (index, kind) in Kind::ALL.into_iter().enumerate() {
        let handle = handle.clone();
        row = row.child(
            Button::new(("menu-bar", index))
                .ghost()
                .small()
                .label(kind.label())
                .selected(open == Some(kind))
                .on_click(move |event, _, cx| {
                    let at = event.position();
                    handle.update(cx, |shell, cx| shell.open_bar_menu(kind, at, cx));
                }),
        );
    }
    row
}

/// Builds the open popup. Runs in render, which has the window the popup needs.
pub(super) fn ensure_popup(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let Some(menu) = shell.bar_menu.as_ref() else {
        return;
    };
    if menu.popup.is_some() {
        return;
    }
    let kind = menu.kind;
    let handle = cx.entity().downgrade();
    let busy = shell.is_busy();
    let dark = cx.theme().is_dark();
    let active = shell.workspaces.document().active.clone();
    let shown: Vec<(WidgetKind, bool)> = WidgetKind::ALL
        .into_iter()
        .map(|kind| (kind, shell.layout.is_shown(kind)))
        .collect();
    let log_dir = shell.log_dir.clone();

    let popup = PopupMenu::build(window, cx, move |mut popup, _, _| {
        match kind {
            Kind::File => {
                popup = popup
                    .item(
                        PopupMenuItem::new("Find duplicates")
                            .disabled(busy)
                            .on_click(act(handle.clone(), |s, cx| s.start_duplicates(cx))),
                    )
                    .item(
                        PopupMenuItem::new("Stop scan")
                            .disabled(!busy)
                            .on_click(act(handle.clone(), |s, cx| s.cancel_walk(cx))),
                    );
            }
            Kind::View => {
                for preset in WORKSPACE_PRESETS {
                    let name = preset.name();
                    popup = popup.item(PopupMenuItem::new(name).checked(active == name).on_click(
                        act(handle.clone(), move |s, cx| s.switch_workspace(name, cx)),
                    ));
                }
                popup = popup.separator();
                for (kind, is_shown) in shown.clone() {
                    popup = popup.item(
                        PopupMenuItem::new(kind.name())
                            .checked(is_shown)
                            .on_click(act(handle.clone(), move |s, cx| s.toggle_widget(kind, cx))),
                    );
                }
                popup = popup
                    .separator()
                    .item(
                        PopupMenuItem::new("Reset layout")
                            .on_click(act(handle.clone(), |s, cx| s.reset_layout(cx))),
                    )
                    .item(
                        PopupMenuItem::new("Manage workspaces…")
                            .on_click(act(handle.clone(), |s, cx| s.open_workspace_panel(cx))),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Dark theme")
                            .checked(dark)
                            .on_click(act(handle.clone(), |_, cx| crate::theme::toggle_theme(cx))),
                    );
            }
            Kind::Tools => {
                popup = popup
                    .item(
                        PopupMenuItem::new("Command palette…  Ctrl+Shift+P")
                            .on_click(palette_act(handle.clone())),
                    )
                    .item(
                        PopupMenuItem::new("Settings…")
                            .on_click(act(handle.clone(), |s, cx| s.toggle_settings(cx))),
                    );
            }
            Kind::Help => {
                popup = popup.item(PopupMenuItem::new("Open log folder").on_click(act(
                    handle.clone(),
                    move |_, _| {
                        if let Err(error) = crate::open::open_folder(&log_dir) {
                            tracing::error!(%error, "could not open the log folder");
                        }
                    },
                )));
            }
        }
        popup.min_w(px(220.))
    });
    // Escape, a click outside or a chosen item dismisses the popup; the menu closes with it.
    cx.subscribe(&popup, |shell, _, _: &DismissEvent, cx| {
        shell.close_bar_menu(cx)
    })
    .detach();
    popup.focus_handle(cx).focus(window, cx);
    if let Some(menu) = shell.bar_menu.as_mut() {
        menu.popup = Some(popup);
    }
}

/// Runs `f` on the shell and closes the menu first, so a chosen item never leaves it open.
fn act(
    handle: WeakEntity<Shell>,
    f: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        let _ = handle.update(cx, |shell, cx| {
            shell.close_bar_menu(cx);
            f(shell, cx);
        });
    }
}

/// The palette needs the window, so it cannot use `act`.
fn palette_act(handle: WeakEntity<Shell>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        let _ = handle.update(cx, |shell, cx| {
            shell.close_bar_menu(cx);
            shell.open_command_palette(window, cx);
        });
    }
}

/// The open popup or workspace panel, over a full-window layer that closes on any click outside.
pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    let popup = shell
        .bar_menu
        .as_ref()
        .and_then(|m| m.popup.clone().map(|popup| (popup, m.at)));
    if popup.is_none() && !shell.workspace_panel {
        return None;
    }
    let close = handle.clone();
    let mut layer = div()
        .absolute()
        .top(px(0.))
        .left(px(0.))
        .size_full()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            close.update(cx, |shell, cx| {
                shell.close_bar_menu(cx);
                shell.close_workspace_panel(cx);
            })
        });
    if let Some((popup, at)) = popup {
        layer = layer.child(
            div()
                .absolute()
                .left(at.x)
                .top(px(MENU_PX + super::toolbar::BAR_PX))
                // Clicks on the popup must not reach the layer, or it would close first.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(popup),
        );
    }
    if shell.workspace_panel {
        layer = layer.child(
            div()
                .absolute()
                .top(px(MENU_PX + super::toolbar::BAR_PX + 4.))
                .right(px(16.))
                .w(px(340.))
                .v_flex()
                .p_2()
                .gap_2()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(super::workspace_menu::section(shell, p, handle)),
        );
    }
    Some(layer)
}
