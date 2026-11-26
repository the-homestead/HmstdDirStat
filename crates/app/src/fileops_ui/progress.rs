//! Planning and execution progress. Both jobs run on the background executor.

use gpui_kit::component::{button::Button, progress::Progress, *};
use gpui_kit::*;

use super::{Shell, view_model};
use crate::shell::widgets::{Palette, message_block};

pub(super) fn planning(p: Palette, handle: Entity<Shell>) -> Div {
    div()
        .v_flex()
        .gap_3()
        .child(message_block(
            "Building delete preview",
            "Reading only the in-memory tree. No files are being changed.",
            p,
        ))
        .child(
            Progress::new("delete-plan-progress")
                .loading(true)
                .accessibility_label("Building delete preview"),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("delete-plan-cancel")
                    .outline()
                    .label("Cancel")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.close_delete(cx));
                    }),
            ),
        )
}

pub(super) fn running(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(control) = shell.delete.control.as_ref() else {
        return div().child("Delete progress is unavailable.");
    };
    let done = control.done();
    let total = control.total();
    let cancelled = control.is_cancelled();
    let mut bar = Progress::new("delete-progress").accessibility_label("Deleting selected entries");
    if let Some(fraction) = view_model::progress_fraction(done, total) {
        bar = bar.value(fraction);
    } else {
        bar = bar.loading(true);
    }

    div()
        .v_flex()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child(if cancelled {
                    "Cancellation requested. Entries already removed stay removed."
                } else {
                    "Deleting on a background thread. The window remains responsive."
                }),
        )
        .child(bar)
        .child(
            div()
                .text_sm()
                .child(view_model::progress_text(done, total)),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("delete-running-cancel")
                    .outline()
                    .label(if cancelled { "Cancelling…" } else { "Cancel" })
                    .disabled(cancelled)
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.cancel_delete(cx));
                    }),
            ),
        )
}
