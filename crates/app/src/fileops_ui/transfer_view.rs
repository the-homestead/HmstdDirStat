//! Copy and move dialog steps: edit, preview, replace confirmation, progress and result.
//! Wording and gating come from `view_model` and `common`. This file only lays out elements.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::Input,
    progress::Progress,
    scroll::*,
    tag::Tag,
    *,
};
use gpui_kit::*;
use timberstat_fileops::{Collision, Failure, Skipped, TransferReport};

use super::common::{policy_group, skip_text};
use super::transfer::{TransferKindUi, TransferStage};
use super::{dialog_frame, view_model};
use crate::shell::{
    Shell,
    widgets::{Palette, Stat, message_block, stat_grid},
};

/// Result rows shown before the rest are counted as hidden.
const RESULT_ROWS_SHOWN: usize = 200;

pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    let verb = shell.transfer.kind.verb();
    let (title, tag, body) = match shell.transfer.stage {
        TransferStage::Closed => return None,
        TransferStage::Editing => (
            format!("{verb} to a folder"),
            Tag::primary().child(verb),
            edit_body(shell, p, handle),
        ),
        TransferStage::Planning => (
            format!("{verb}: building preview"),
            Tag::secondary().child("Planning"),
            planning_body(p, handle),
        ),
        TransferStage::Preview => (
            format!("{verb} preview"),
            kind_tag(shell.transfer.kind),
            preview_body(shell, p, handle, false),
        ),
        TransferStage::OverwriteConfirm => (
            "Replace existing item?".to_string(),
            Tag::danger().child("Replace"),
            preview_body(shell, p, handle, true),
        ),
        TransferStage::Running => (
            format!("{verb} in progress"),
            kind_tag(shell.transfer.kind),
            running_body(shell, p, handle),
        ),
        TransferStage::Result => {
            let report = shell.transfer.report.as_deref();
            let tag = match report {
                Some(report) if report.cancelled => Tag::warning().child("Cancelled"),
                Some(report) if report.is_complete() => Tag::success().child("Complete"),
                _ => Tag::warning().child("Partial"),
            };
            (format!("{verb} result"), tag, result_body(shell, p, handle))
        }
        TransferStage::Error => (
            format!("{verb} cannot start"),
            Tag::danger().child("Stopped"),
            error_body(shell, p, handle),
        ),
    };
    Some(dialog_frame(&title, tag, body, p))
}

fn kind_tag(kind: TransferKindUi) -> Tag {
    match kind {
        TransferKindUi::Copy => Tag::primary().child("Copy"),
        TransferKindUi::Move => Tag::primary().child("Move"),
    }
}

fn label(text: &str, p: Palette) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.muted_foreground)
        .child(text.to_string())
}

fn danger_text(text: &str, p: Palette) -> Div {
    div().text_xs().text_color(p.danger).child(text.to_string())
}

fn edit_body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let t = &shell.transfer;
    let mut body = div()
        .v_flex()
        .gap_3()
        .child(label("Source", p))
        .child(div().text_xs().child(t.source.clone()))
        .child(label("Destination folder (full path)", p));
    if let Some(input) = t.dest_input.as_ref() {
        body = body.child(Input::new(input).aria_label("Destination folder"));
    }
    if let Some(error) = t.dest_error.as_deref() {
        body = body.child(danger_text(error, p));
    }
    body = body
        .child(label("If the name already exists", p))
        .child(policy_group(
            "transfer-policy",
            t.collision,
            p,
            handle.clone(),
            Shell::set_transfer_collision,
        ));
    if t.collision == Collision::Rename {
        if let Some(input) = t.name_input.as_ref() {
            body = body.child(Input::new(input).aria_label("New name for the item"));
        }
        if let Some(error) = t.name_error.as_deref() {
            body = body.child(danger_text(error, p));
        }
    }
    let cancel = handle.clone();
    body.child(
        div()
            .h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("transfer-edit-cancel")
                    .outline()
                    .label("Cancel")
                    .on_click(move |_, _, cx| {
                        cancel.update(cx, |shell, cx| shell.close_transfer(cx));
                    }),
            )
            .child(
                Button::new("transfer-edit-preview")
                    .primary()
                    .label("Preview")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.preview_transfer(cx));
                    }),
            ),
    )
}

fn planning_body(p: Palette, handle: Entity<Shell>) -> Div {
    div()
        .v_flex()
        .gap_3()
        .child(message_block(
            "Building preview",
            "Reading the in-memory tree and checking the destination. No files are changed.",
            p,
        ))
        .child(
            Progress::new("transfer-plan-progress")
                .loading(true)
                .accessibility_label("Building preview"),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("transfer-plan-cancel")
                    .outline()
                    .label("Cancel")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.close_transfer(cx));
                    }),
            ),
        )
}

fn preview_body(shell: &Shell, p: Palette, handle: Entity<Shell>, replacing: bool) -> Div {
    let t = &shell.transfer;
    let Some(plan) = t.plan.as_ref() else {
        return div().child("No preview is available.");
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
                label: "Logical size",
                value: text.logical,
            },
            Stat {
                label: "On-disk size",
                value: text.physical,
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

    let kind = t.kind;
    let note = match kind {
        TransferKindUi::Copy => {
            "Copied items are not added to the folder tree until the next scan."
        }
        TransferKindUi::Move => {
            "Moved items leave the folder tree now. They appear at the destination after the next scan."
        }
    };
    let mut body = div()
        .v_flex()
        .gap_3()
        .child(div().text_xs().child(format!(
            "From {}\nInto folder {}",
            t.source,
            plan.dest_dir().display()
        )))
        .child(div().text_sm().text_color(p.muted_foreground).child(note))
        .child(stats);
    if replacing {
        body = body.child(
            div()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(p.danger)
                .text_sm()
                .text_color(p.danger)
                .child(
                    "An item with this name already exists. Replacing overwrites existing files with the same name and merges into existing folders. A file and a folder never replace each other. Nothing is replaced until you press Replace.",
                ),
        );
    }
    body = body.child(
        div()
            .h(px(200.))
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .child(div().size_full().overflow_y_scrollbar().child(paths)),
    );

    let back = handle.clone();
    let run = handle;
    let run_label = match (kind, replacing) {
        (TransferKindUi::Copy, false) => "Copy",
        (TransferKindUi::Move, false) => "Move",
        (_, true) => "Replace and continue",
    };
    body.child(
        div()
            .h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("transfer-preview-back")
                    .outline()
                    .label("Back")
                    .on_click(move |_, _, cx| {
                        back.update(cx, |shell, cx| shell.back_to_transfer_edit(cx));
                    }),
            )
            .child(
                Button::new("transfer-preview-run")
                    .primary()
                    .label(run_label)
                    .on_click(move |_, _, cx| {
                        run.update(cx, |shell, cx| shell.run_transfer(cx));
                    }),
            ),
    )
}

fn running_body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(control) = shell.transfer.control.as_ref() else {
        return div().child("Progress is unavailable.");
    };
    let done = control.done();
    let total = control.total();
    let cancelled = control.is_cancelled();
    let mut bar = Progress::new("transfer-progress").accessibility_label("Transferring items");
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
                    "Cancellation requested. Items already finished stay where they are."
                } else {
                    "Working on a background thread. The window remains responsive."
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
                Button::new("transfer-running-cancel")
                    .outline()
                    .label(if cancelled { "Cancelling…" } else { "Cancel" })
                    .disabled(cancelled)
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.cancel_transfer(cx));
                    }),
            ),
        )
}

fn result_body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(report) = shell.transfer.report.as_deref() else {
        return div().child("No result is available.");
    };
    let stats = stat_grid(
        &[
            Stat {
                label: "Items done",
                value: format!("{} of {}", report.done_top_level, report.top_level_total),
            },
            Stat {
                label: "Files copied",
                value: report.files_copied.to_string(),
            },
            Stat {
                label: "Failed",
                value: report.failures.len().to_string(),
            },
            Stat {
                label: "Skipped",
                value: report.skipped.len().to_string(),
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
                .child(report.summary()),
        )
        .child(stats);
    let rows = result_rows(report);
    if !rows.is_empty() {
        let mut list = div().v_flex().gap_1().p_2();
        for (path, detail, failed) in rows.iter().take(RESULT_ROWS_SHOWN) {
            list = list.child(
                div()
                    .v_flex()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(path.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if *failed {
                                p.danger
                            } else {
                                p.muted_foreground
                            })
                            .child(detail.clone()),
                    ),
            );
        }
        if rows.len() > RESULT_ROWS_SHOWN {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(p.muted_foreground)
                    .child(format!("{} more not shown", rows.len() - RESULT_ROWS_SHOWN)),
            );
        }
        body = body.child(
            div()
                .h(px(240.))
                .w_full()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .child(div().size_full().overflow_y_scrollbar().child(list)),
        );
    }
    body.child(
        div().h_flex().justify_end().child(
            Button::new("transfer-result-close")
                .outline()
                .label("Close")
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.close_transfer(cx));
                }),
        ),
    )
}

/// Every failed and skipped path as `(path, detail, is_failure)`, failures first.
fn result_rows(report: &TransferReport) -> Vec<(String, String, bool)> {
    let failed = report.failures.iter().map(|failure: &Failure| {
        (
            failure.path.display().to_string(),
            view_model::failure_text(failure),
            true,
        )
    });
    let skipped = report
        .skipped
        .iter()
        .map(|skip: &Skipped| (skip.path.display().to_string(), skip_text(skip), false));
    failed.chain(skipped).collect()
}

fn error_body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    div()
        .v_flex()
        .gap_3()
        .child(
            div().text_sm().text_color(p.danger).child(
                shell
                    .transfer
                    .error
                    .clone()
                    .unwrap_or_else(|| "The operation could not be planned.".to_string()),
            ),
        )
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child("Nothing was changed."),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("transfer-error-close")
                    .outline()
                    .label("Close")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.close_transfer(cx));
                    }),
            ),
        )
}
