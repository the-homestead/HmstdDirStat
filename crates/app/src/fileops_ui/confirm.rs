//! Second, separate permanent-delete confirmation. The engine checks the phrase again.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::Input,
    *,
};
use gpui_kit::*;

use super::{Shell, view_model};
use crate::shell::widgets::Palette;

pub(super) fn body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(plan) = shell.delete.plan.as_ref() else {
        return div().child("No delete plan is available.");
    };
    let phrase = plan.confirm_phrase();
    let typed = &shell.delete.typed;
    let matches = view_model::confirmation_matches(typed, phrase);
    let count = plan.preview().entry_count();

    let mut field = div().v_flex().gap_2().child(
        div()
            .text_sm()
            .child(format!("Type {phrase} to confirm permanent delete.")),
    );
    if let Some(input) = shell.delete.input.as_ref() {
        field = field.child(Input::new(input).aria_label("Permanent delete confirmation"));
    }
    if !typed.is_empty() && !matches {
        field = field.child(
            div()
                .text_xs()
                .text_color(p.danger)
                .child("The text does not match. Nothing has been deleted."),
        );
    }

    let back = handle.clone();
    let cancel = handle.clone();
    div()
        .v_flex()
        .gap_3()
        .child(
            div()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(p.danger)
                .text_sm()
                .text_color(p.danger)
                .child(format!(
                    "Permanently delete {count} entries? This cannot be undone and does not use the Recycle Bin."
                )),
        )
        .child(field)
        .child(
            div()
                .h_flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("delete-confirm-back")
                        .outline()
                        .label("Back")
                        .on_click(move |_, _, cx| {
                            back.update(cx, |shell, cx| shell.back_to_delete_preview(cx));
                        }),
                )
                .child(
                    Button::new("delete-confirm-cancel")
                        .outline()
                        .label("Cancel")
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |shell, cx| shell.close_delete(cx));
                        }),
                )
                .child(
                    Button::new("delete-permanent-confirm")
                        .primary()
                        .label("Delete permanently")
                        .disabled(!matches)
                        .on_click(move |_, _, cx| {
                            handle.update(cx, |shell, cx| shell.confirm_delete(cx));
                        }),
                ),
        )
}
