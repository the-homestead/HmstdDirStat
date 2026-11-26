//! First confirmation step: exactly what the delete plan contains, with no disk access.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    scroll::*,
    *,
};
use gpui_kit::*;
use hmstd_fileops::DeleteMode;

use super::{Shell, view_model};
use crate::shell::widgets::{Palette, Stat, stat_grid};

pub(super) fn body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(plan) = shell.delete.plan.as_ref() else {
        return div().child("No delete plan is available.");
    };
    let preview = plan.preview();
    let text = view_model::preview_text(preview);
    let stats = stat_grid(
        &[
            Stat {
                label: "Files",
                value: text.files,
            },
            Stat {
                label: "Folders",
                value: text.directories,
            },
            Stat {
                label: "Links / other",
                value: text.other,
            },
            Stat {
                label: "Logical size",
                value: text.logical,
            },
            Stat {
                label: "On-disk size",
                value: text.physical,
            },
            Stat {
                label: "Reclaimable",
                value: text.reclaimable,
            },
        ],
        p,
    );

    let mut paths = div().v_flex().gap_1().p_2();
    for path in &preview.paths_shown {
        paths = paths.child(div().text_xs().child(path.display().to_string()));
    }
    if let Some(hidden) = view_model::hidden_paths_text(preview.paths_hidden) {
        paths = paths.child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted_foreground)
                .child(hidden),
        );
    }

    let mode = shell.delete.mode;
    let confirm = handle.clone();
    let cancel = handle;
    div()
        .v_flex()
        .gap_3()
        .child(div().text_sm().text_color(p.muted_foreground).child(
            if mode == DeleteMode::Permanent {
                "Review every path. Permanent delete has a second confirmation step."
            } else {
                "Review every path. Nothing is sent to the Recycle Bin until you confirm."
            },
        ))
        .child(stats)
        .child(
            div()
                .h(px(220.))
                .w_full()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .child(div().size_full().overflow_y_scrollbar().child(paths)),
        )
        .child(
            div()
                .h_flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("delete-preview-cancel")
                        .outline()
                        .label("Cancel")
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |shell, cx| shell.close_delete(cx));
                        }),
                )
                .child(
                    Button::new("delete-preview-confirm")
                        .primary()
                        .label(if mode == DeleteMode::Permanent {
                            "Continue"
                        } else {
                            "Move to Recycle Bin"
                        })
                        .on_click(move |_, _, cx| {
                            confirm.update(cx, |shell, cx| shell.review_delete(cx));
                        }),
                ),
        )
}
