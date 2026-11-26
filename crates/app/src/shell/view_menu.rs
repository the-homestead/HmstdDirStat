//! The View menu: saved workspaces, widget visibility and layout reset. Each widget is listed
//! with a tick when shown. Workspace presets and custom layouts live above the widget list.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    *,
};
use gpui_kit::*;
use hmstd_core::workspace::WidgetKind;

use super::{Shell, widgets::*};

/// The toolbar button that opens and closes the menu.
pub(super) fn view_button(handle: Entity<Shell>) -> Button {
    Button::new("view-menu")
        .ghost()
        .small()
        .icon(gpui_kit::component::IconName::LayoutDashboard)
        .tooltip("Workspaces, widgets and layout")
        .on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| shell.toggle_view_menu(cx));
        })
}

/// The dropdown, drawn over the window while it is open. A click outside it closes it.
pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    if !shell.view_menu {
        return None;
    }
    let mut card = div()
        .absolute()
        .top(px(super::toolbar::BAR_PX + 4.))
        .right(px(16.))
        .v_flex()
        .w(px(380.))
        .max_h(px(760.))
        .overflow_y_scrollbar()
        .p_1()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        // Clicks on the card must not reach the overlay, or the menu would close first.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(super::workspace_menu::section(shell, p, handle.clone()))
        .child(div().h(px(1.)).my_1().bg(p.border))
        .child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(p.muted_foreground)
                .child("Widgets"),
        );
    for kind in WidgetKind::ALL {
        let shown = shell.layout.is_shown(kind);
        let toggle = handle.clone();
        card = card.child(item(kind.name(), kind, shown, p, move |cx| {
            toggle.update(cx, |shell, cx| shell.toggle_widget(kind, cx));
        }));
    }
    let reset = handle.clone();
    card = card.child(div().h(px(1.)).my_1().bg(p.border)).child(
        div()
            .id("view-reset")
            .px_2()
            .py_1()
            .rounded_sm()
            .text_sm()
            .cursor_pointer()
            .hover(|style| style.bg(p.background))
            .child("Reset layout")
            .on_click(move |_, _, cx| {
                reset.update(cx, |shell, cx| shell.reset_layout(cx));
            }),
    );

    let close = handle;
    Some(
        div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                close.update(cx, |shell, cx| shell.close_view_menu(cx));
            })
            .child(card),
    )
}

fn item(
    name: &'static str,
    kind: WidgetKind,
    shown: bool,
    p: Palette,
    on_click: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    let mark = if shown { "✓" } else { " " };
    div()
        .id(format!("view-{name}"))
        .h_flex()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_sm()
        .text_sm()
        .cursor_pointer()
        .hover(|style| style.bg(p.background))
        .child(div().w(px(12.)).child(mark))
        .child(kind.title())
        .on_click(move |_, _, cx| on_click(cx))
}
