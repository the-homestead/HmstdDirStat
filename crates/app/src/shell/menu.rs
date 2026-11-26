//! The right-click menu over a tile or arc. A click outside the card closes it.

use gpui_kit::component::*;
use gpui_kit::*;
use hmstd_fileops::DeleteMode;
use hmstd_filesystem::{EntryKind, NodeId};

use super::{Shell, fileops_ui::TransferKindUi, widgets::*};

/// The right-click menu for the node in `shell.menu`, drawn over the whole window. A click
/// anywhere outside the menu closes it.
pub(super) fn menu_overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    let menu = shell.menu?;
    let files = shell.tree.as_deref()?;
    let entry = files.node(menu.node)?;
    let is_dir = entry.kind() == EntryKind::Directory;
    let name = entry.name().to_string_lossy().into_owned();
    let node = menu.node;

    let mut card = div()
        .absolute()
        .left(menu.at.x)
        .top(menu.at.y)
        .v_flex()
        .min_w(px(200.))
        .p_1()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        // Clicks on the card must not reach the overlay, or the menu would close first.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .px_2()
                .py_1()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(name),
        );
    // A folder opens itself. A file opens the folder that holds it.
    if let Some(target) = if is_dir { Some(node) } else { entry.parent() } {
        let open = handle.clone();
        card = card.child(menu_item("menu-open", "Open folder", p, move |cx| {
            open.update(cx, |shell, cx| shell.focus_on(target, cx));
        }));
    }
    let copy = handle.clone();
    card = card.child(menu_item("menu-copy", "Copy path", p, move |cx| {
        copy.update(cx, |shell, cx| shell.copy_path(node, cx));
    }));
    let explorer = handle.clone();
    card = card.child(menu_item(
        "menu-explorer",
        "Show in Explorer",
        p,
        move |cx| {
            explorer.update(cx, |shell, cx| shell.show_in_explorer(node, cx));
        },
    ));
    if menu.node != NodeId::ROOT {
        let copy_to = handle.clone();
        card = card.child(menu_item("menu-copy-to", "Copy to…", p, move |cx| {
            copy_to.update(cx, |shell, cx| {
                shell.begin_transfer(node, TransferKindUi::Copy, cx)
            });
        }));
        let move_to = handle.clone();
        card = card.child(menu_item("menu-move-to", "Move to…", p, move |cx| {
            move_to.update(cx, |shell, cx| {
                shell.begin_transfer(node, TransferKindUi::Move, cx)
            });
        }));
        let rename = handle.clone();
        card = card.child(menu_item("menu-rename", "Rename…", p, move |cx| {
            rename.update(cx, |shell, cx| shell.begin_rename(node, cx));
        }));
        let recycle = handle.clone();
        card = card.child(menu_item(
            "menu-delete-recycle",
            "Delete (Recycle Bin)",
            p,
            move |cx| {
                recycle.update(cx, |shell, cx| {
                    shell.begin_delete(node, DeleteMode::RecycleBin, cx)
                });
            },
        ));
        let permanent = handle.clone();
        card = card.child(menu_item(
            "menu-delete-permanent",
            "Delete permanently",
            p,
            move |cx| {
                permanent.update(cx, |shell, cx| {
                    shell.begin_delete(node, DeleteMode::Permanent, cx)
                });
            },
        ));
    }

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
            .child(card),
    )
}

fn menu_item(
    id: &'static str,
    label: &'static str,
    p: Palette,
    on_click: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .text_sm()
        .cursor_pointer()
        .hover(|style| style.bg(p.background))
        .child(label)
        .on_click(move |_, _, cx| on_click(cx))
}
