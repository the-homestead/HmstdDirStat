//! Rename dialog steps: edit, check, replace confirmation, and the result.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::Input,
    progress::Progress,
    tag::Tag,
    *,
};
use gpui_kit::*;
use timberstat_fileops::{Failure, RenameOutcome};

use super::common::policy_group;
use super::rename::{RenameStage, RenameUi};
use super::{dialog_frame, view_model};
use crate::shell::{
    Shell,
    widgets::{Palette, message_block},
};

pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    let (title, tag, body) = match shell.rename.stage {
        RenameStage::Closed => return None,
        RenameStage::Editing => (
            "Rename",
            Tag::primary().child("Rename"),
            edit_body(shell, p, handle),
        ),
        RenameStage::Checking => (
            "Rename: checking name",
            Tag::secondary().child("Checking"),
            busy_body(p, "Checking the new name. No files are changed yet."),
        ),
        RenameStage::OverwriteConfirm => (
            "Replace existing item?",
            Tag::danger().child("Replace"),
            confirm_body(&shell.rename, p, handle),
        ),
        RenameStage::Running => (
            "Rename in progress",
            Tag::primary().child("Renaming"),
            busy_body(p, "Renaming on disk. The window remains responsive."),
        ),
        RenameStage::Result => (
            "Rename result",
            outcome_tag(shell.rename.outcome.as_ref()),
            result_body(&shell.rename, p, handle),
        ),
    };
    Some(dialog_frame(title, tag, body, p))
}

/// Plain wording for a rename result. A failure is never described as a rename.
pub(super) fn outcome_text(outcome: &Result<RenameOutcome, Failure>) -> String {
    match outcome {
        Ok(RenameOutcome::Renamed { to }) => format!(
            "Renamed to {}.",
            to.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        Ok(RenameOutcome::Unchanged) => "The name is unchanged. Nothing was changed.".into(),
        Ok(RenameOutcome::Skipped { existing }) => format!(
            "Not renamed: {} already exists. Skip policy left everything as it was.",
            existing.display()
        ),
        Err(failure) => format!("Not renamed: {}", view_model::failure_text(failure)),
    }
}

fn outcome_tag(outcome: Option<&Result<RenameOutcome, Failure>>) -> Tag {
    match outcome {
        Some(Ok(RenameOutcome::Renamed { .. })) => Tag::success().child("Renamed"),
        Some(Ok(_)) => Tag::warning().child("Not changed"),
        _ => Tag::danger().child("Failed"),
    }
}

fn edit_body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let r = &shell.rename;
    let mut body = div()
        .v_flex()
        .gap_3()
        .child(
            div()
                .text_xs()
                .text_color(p.muted_foreground)
                .child(format!("Current name {}", r.old_name)),
        )
        .child(
            div().text_xs().text_color(p.muted_foreground).child(
                r.path
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            ),
        );
    if let Some(input) = r.name_input.as_ref() {
        body = body.child(Input::new(input).aria_label("New name"));
    }
    if let Some(error) = r.name_error.as_deref() {
        body = body.child(
            div()
                .text_xs()
                .text_color(p.danger)
                .child(error.to_string()),
        );
    }
    body = body.child(policy_group(
        "rename-policy",
        r.collision,
        p,
        handle.clone(),
        Shell::set_rename_collision,
    ));
    let cancel = handle.clone();
    body.child(
        div()
            .h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("rename-cancel")
                    .outline()
                    .label("Cancel")
                    .on_click(move |_, _, cx| {
                        cancel.update(cx, |shell, cx| shell.close_rename(cx));
                    }),
            )
            .child(
                Button::new("rename-submit")
                    .primary()
                    .label("Rename")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.submit_rename(cx));
                    }),
            ),
    )
}

fn busy_body(p: Palette, text: &str) -> Div {
    div()
        .v_flex()
        .gap_3()
        .child(message_block("Working", text, p))
        .child(
            Progress::new("rename-progress")
                .loading(true)
                .accessibility_label("Renaming"),
        )
}

fn confirm_body(r: &RenameUi, p: Palette, handle: Entity<Shell>) -> Div {
    let target = r
        .target
        .as_deref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let back = handle.clone();
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
                    "{target} already exists. Replace overwrites a file with that name. A folder is never replaced, so the rename would fail. Nothing is replaced until you press Replace."
                )),
        )
        .child(
            div()
                .h_flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("rename-confirm-back")
                        .outline()
                        .label("Back")
                        .on_click(move |_, _, cx| {
                            back.update(cx, |shell, cx| shell.back_to_rename_edit(cx));
                        }),
                )
                .child(
                    Button::new("rename-confirm-replace")
                        .danger()
                        .label("Replace")
                        .on_click(move |_, _, cx| {
                            handle.update(cx, |shell, cx| shell.replace_rename(cx));
                        }),
                ),
        )
}

fn result_body(r: &RenameUi, p: Palette, handle: Entity<Shell>) -> Div {
    let text = match r.outcome.as_ref() {
        Some(outcome) => outcome_text(outcome),
        None => "No result is available.".to_string(),
    };
    // A failed rename reads as an error, not as a quiet result line.
    let color = if matches!(r.outcome, Some(Ok(_))) {
        p.foreground
    } else {
        p.danger
    };
    div()
        .v_flex()
        .gap_3()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(text),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("rename-result-close")
                    .outline()
                    .label("Close")
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.close_rename(cx));
                    }),
            ),
        )
}
