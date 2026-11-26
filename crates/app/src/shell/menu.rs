//! The right-click menu over a tile, arc or row. It is gpui-kit's `PopupMenu`: hover and
//! keyboard highlight, Escape and click-outside dismissal come from the component. This file
//! builds its items for the clicked node and places it where the pointer was.

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use timberstat_fileops::DeleteMode;
use timberstat_filesystem::{EntryKind, NodeId};

use super::{Shell, fileops_ui::TransferKindUi, widgets::*};

/// Builds the popup for a menu that was just opened. Runs in render, which has the window the
/// popup needs. Does nothing when no menu is open or its popup already exists.
pub(super) fn ensure_popup(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let Some(menu) = shell.menu.as_ref() else {
        return;
    };
    if menu.popup.is_some() {
        return;
    }
    let node = menu.node;
    let Some(entry) = shell.tree.as_deref().and_then(|files| files.node(node)) else {
        return;
    };
    let is_dir = entry.kind() == EntryKind::Directory;
    let name = entry.name().to_string_lossy().into_owned();
    // A folder opens itself. A file opens the folder that holds it.
    let open_target = if is_dir { Some(node) } else { entry.parent() };
    let shell_ref = cx.entity().downgrade();

    let popup = PopupMenu::build(window, cx, move |mut popup, _, _| {
        let act = |f: fn(&mut Shell, NodeId, &mut Context<Shell>)| {
            let shell_ref = shell_ref.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                let _ = shell_ref.update(cx, |shell, cx| f(shell, node, cx));
            }
        };
        popup = popup.min_w(px(220.)).label(name).separator();
        if let Some(target) = open_target {
            let shell_ref = shell_ref.clone();
            popup = popup.item(PopupMenuItem::new("Open folder").on_click(move |_, _, cx| {
                let _ = shell_ref.update(cx, |shell, cx| {
                    shell.close_menu(cx);
                    shell.focus_on(target, cx);
                });
            }));
        }
        popup = popup
            .item(PopupMenuItem::new("Copy path").on_click(act(|s, n, cx| s.copy_path(n, cx))))
            .item(
                PopupMenuItem::new("Show in Explorer")
                    .on_click(act(|s, n, cx| s.show_in_explorer(n, cx))),
            );
        if node != NodeId::ROOT {
            popup = popup
                .separator()
                .item(PopupMenuItem::new("Copy to…").on_click(act(|s, n, cx| {
                    s.begin_transfer(n, TransferKindUi::Copy, cx)
                })))
                .item(PopupMenuItem::new("Move to…").on_click(act(|s, n, cx| {
                    s.begin_transfer(n, TransferKindUi::Move, cx)
                })))
                .item(PopupMenuItem::new("Rename…").on_click(act(|s, n, cx| s.begin_rename(n, cx))))
                .separator()
                .item(
                    PopupMenuItem::new("Delete (Recycle Bin)").on_click(act(|s, n, cx| {
                        s.begin_delete(n, DeleteMode::RecycleBin, cx)
                    })),
                )
                .item(
                    PopupMenuItem::new("Delete permanently")
                        .on_click(act(|s, n, cx| s.begin_delete(n, DeleteMode::Permanent, cx))),
                );
        }
        popup
    });
    // Escape, a click outside or a chosen item dismisses the popup; the menu closes with it.
    cx.subscribe(&popup, |shell, _, _: &DismissEvent, cx| {
        shell.close_menu(cx)
    })
    .detach();
    // Focused, so the arrow keys move the highlight and Enter picks.
    popup.focus_handle(cx).focus(window, cx);
    if let Some(menu) = shell.menu.as_mut() {
        menu.popup = Some(popup);
    }
}

/// The open menu's popup, placed at the pointer, over a full-window layer that closes it on
/// any click outside.
pub(super) fn menu_overlay(shell: &Shell, _p: Palette, handle: Entity<Shell>) -> Option<Div> {
    let menu = shell.menu.as_ref()?;
    let popup = menu.popup.clone()?;
    let close = handle;
    Some(
        div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .size_full()
            .on_mouse_down(MouseButton::Left, {
                let close = close.clone();
                move |_, _, cx| close.update(cx, |shell, cx| shell.close_menu(cx))
            })
            .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                close.update(cx, |shell, cx| shell.close_menu(cx))
            })
            .child(
                div()
                    .absolute()
                    .left(menu.at.x)
                    .top(menu.at.y)
                    // Clicks on the popup must not reach the layer, or it would close first.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(popup),
            ),
    )
}
