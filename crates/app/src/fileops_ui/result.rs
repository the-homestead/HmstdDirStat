//! Delete report panel. Failures use a virtualized list, so a large partial run does not
//! create one live GPUI element per failure.

use gpui_kit::component::{button::Button, *};
use gpui_kit::*;
use timberstat_fileops::Failure;

use super::{Shell, view_model};
use crate::shell::widgets::{Palette, Stat, stat_grid};

pub(super) fn body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(report) = shell.delete.report.as_ref() else {
        return div().child("No delete report is available.");
    };
    let headline = view_model::result_headline(report);
    let stats = stat_grid(
        &[
            Stat {
                label: "Tree entries removed",
                value: shell.delete.detached.to_string(),
            },
            Stat {
                label: "Failures",
                value: report.failures.len().to_string(),
            },
            Stat {
                label: "Not attempted",
                value: report.not_attempted.to_string(),
            },
        ],
        p,
    );

    let mut body = div()
        .v_flex()
        .gap_3()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(headline),
        )
        .child(stats);

    if !report.failures.is_empty() {
        let failures = report.clone();
        body = body.child(
            div()
                .h(px(240.))
                .w_full()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .child(
                    uniform_list(
                        "delete-failures",
                        failures.failures.len(),
                        move |range, _window, _cx| {
                            range
                                .map(|index| failure_row(index, &failures.failures[index], p))
                                .collect::<Vec<_>>()
                        },
                    )
                    .h_full()
                    .w_full(),
                ),
        );
    }

    body.child(
        div().h_flex().justify_end().child(
            Button::new("delete-result-close")
                .outline()
                .label("Close")
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.close_delete(cx));
                }),
        ),
    )
}

fn failure_row(index: usize, failure: &Failure, p: Palette) -> Stateful<Div> {
    let path = failure.path.display().to_string();
    let detail = view_model::failure_text(failure);
    let full = format!("{path}\n{detail}");
    div()
        .id(("delete-failure", index))
        .h(px(48.))
        .w_full()
        .v_flex()
        .justify_center()
        .gap_1()
        .px_2()
        .border_b_1()
        .border_color(p.border)
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(full.clone()).build(window, cx)
        })
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(path),
        )
        .child(
            div()
                .text_xs()
                .text_color(p.danger)
                .truncate()
                .child(detail),
        )
}

pub(super) fn plan_error(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    div()
        .v_flex()
        .gap_3()
        .child(
            div().text_sm().text_color(p.danger).child(
                shell
                    .delete
                    .error
                    .clone()
                    .unwrap_or_else(|| "The delete plan could not be built.".to_string()),
            ),
        )
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child("Nothing was deleted."),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("delete-error-close")
                    .outline()
                    .label("Close")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.close_delete(cx));
                    }),
            ),
        )
}
