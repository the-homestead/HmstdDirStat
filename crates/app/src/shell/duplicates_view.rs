//! The Duplicates widget body: a dense sheet of byte-verified copy groups.
//!
//! Each group lists its copies. A click selects a copy, a double click opens the folder that
//! holds it, and a right click opens the file menu, which has Show in Explorer, Copy path and the
//! delete actions that go through the preview. No copy is marked as the one to keep.

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{
    button::Button, empty::EmptyContent, progress::Progress as Bar, tag::Tag, *,
};
use gpui_kit::*;
use timberstat_core::workspace::WidgetKind;
use timberstat_duplicates::{
    DuplicateGroup, DuplicateMember, DuplicateReport, Progress, RunStatus, Stage,
};
use timberstat_filesystem::{FileTree, NodeId};

use super::{
    Shell, empty_state,
    widgets::{Palette, section_heading},
};
use crate::scan::{format_bytes, format_count};

/// Groups drawn before the list stops. The header still counts all of them.
const MAX_GROUPS: usize = 100;
/// Copies drawn per group before "and N more".
const MAX_COPIES: usize = 20;
/// Failed files named in the sheet. The rest are counted only.
const MAX_FAILURES: usize = 5;

/// The widget body for the current duplicate state.
pub(super) fn body(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Div {
    let Some(files) = shell.tree.as_deref() else {
        return empty_state::no_tree(WidgetKind::Duplicates, shell.is_scanning());
    };
    if let Some(progress) = shell.dup.progress() {
        return running(progress, p, handle);
    }
    match shell.dup.report.as_ref() {
        None => idle(handle),
        Some(report) => sheet(shell, files, report, p, handle),
    }
}

fn start_button(handle: Entity<Shell>, label: &'static str) -> Button {
    Button::new("duplicates-start")
        .outline()
        .label(label)
        .on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| shell.start_duplicates(cx));
        })
}

/// Before the first run: what the search does, and the button that starts it.
fn idle(handle: Entity<Shell>) -> Div {
    let content = EmptyContent::new().child(start_button(handle, "Find duplicates"));
    empty_state::centred(
        empty_state::empty(
            Glyph::Copy,
            "No duplicate search yet",
            "Groups files of equal size, then compares their content. Nothing is deleted.",
        )
        .content(content),
    )
}

/// While a run is going: its stage, a bar, the count, and Cancel.
fn running(progress: &Progress, p: Palette, handle: Entity<Shell>) -> Div {
    let (done, total) = (progress.done(), progress.total());
    let cancelled = progress.is_cancelled();
    let mut bar = Bar::new("duplicates-progress").accessibility_label("Searching for duplicates");
    if total > 0 {
        let fraction = (done as f64 / total as f64).clamp(0.0, 1.0) as f32;
        bar = bar.value(fraction);
    } else {
        bar = bar.loading(true);
    }
    let note = if cancelled {
        "Cancelling. Nothing is deleted."
    } else {
        "Reads only. Nothing is deleted."
    };
    div()
        .v_flex()
        .gap_3()
        .child(
            div()
                .h_flex()
                .items_center()
                .gap_2()
                .child(Tag::secondary().child(progress.stage().label()))
                .child(div().text_sm().text_color(p.muted_foreground).child(note)),
        )
        .child(bar)
        .child(
            div()
                .text_sm()
                .child(progress_text(progress.stage(), done, total)),
        )
        .child(
            div().h_flex().justify_end().child(
                Button::new("duplicates-cancel")
                    .outline()
                    .label(if cancelled { "Cancelling…" } else { "Cancel" })
                    .disabled(cancelled)
                    .on_click(move |_, _, cx| {
                        handle.update(cx, |shell, cx| shell.cancel_duplicates(cx));
                    }),
            ),
        )
}

/// Stage progress as text. Hashing and verifying count bytes; earlier stages count files.
fn progress_text(stage: Stage, done: u64, total: u64) -> String {
    if total == 0 {
        return format!("{} files so far", format_count(done));
    }
    let done = done.min(total);
    match stage {
        Stage::Hashing | Stage::Verifying => {
            format!("{} of {}", format_bytes(done), format_bytes(total))
        }
        _ => format!("{} of {} files", format_count(done), format_count(total)),
    }
}

/// A group with only the copies still in the tree, and the logical bytes those copies reclaim.
/// Computed at render time, so a copy deleted from the panel drops out at once.
struct Live<'a> {
    group: &'a DuplicateGroup,
    copies: Vec<&'a DuplicateMember>,
    reclaimable: u64,
}

fn live_groups<'a>(files: &FileTree, report: &'a DuplicateReport) -> Vec<Live<'a>> {
    report
        .groups
        .iter()
        .filter_map(|group| {
            let copies: Vec<&DuplicateMember> = group
                .members
                .iter()
                .filter(|member| files.is_attached(member.id))
                .collect();
            if copies.len() < 2 {
                return None;
            }
            let extra = (copies.len() - 1) as u64;
            Some(Live {
                group,
                copies,
                reclaimable: group.size.saturating_mul(extra),
            })
        })
        .collect()
}

/// The finished sheet: status line, facts, groups, and failures.
fn sheet(
    shell: &Shell,
    files: &FileTree,
    report: &DuplicateReport,
    p: Palette,
    handle: Entity<Shell>,
) -> Div {
    let live = live_groups(files, report);
    let reclaimable = live
        .iter()
        .fold(0u64, |sum, group| sum.saturating_add(group.reclaimable));
    let copies: usize = live.iter().map(|group| group.copies.len()).sum();

    let mut sheet = div()
        .v_flex()
        .gap_3()
        .child(status_line(report, live.len(), handle.clone()))
        .child(div().h(px(1.)).bg(p.border))
        .child(fact("Reclaimable (logical)", &format_bytes(reclaimable), p))
        .child(fact("Copies in groups", &format_count(copies as u64), p))
        .child(fact(
            "Hard links counted once",
            &format_count(report.hardlinks_skipped),
            p,
        ))
        .child(fact(
            "Files compared",
            &format_count(report.files_considered),
            p,
        ))
        .child(div().text_xs().text_color(p.muted_foreground).child(
            "Reclaimable is (copies − 1) × size, in logical bytes. It is not an on-disk saving.",
        ));

    match &report.status {
        RunStatus::Cancelled => {
            sheet = sheet.child(note(
                "Cancelled. No groups are shown. Run again to see results.",
                p.warning,
            ));
        }
        RunStatus::Failed(message) => {
            sheet = sheet.child(note(&format!("The search failed: {message}"), p.danger));
        }
        RunStatus::Complete => {}
    }

    if let Some(failures) = failures(report, p) {
        sheet = sheet.child(failures);
    }

    if live.is_empty() {
        if report.status == RunStatus::Complete {
            sheet = sheet.child(no_duplicates(report, handle));
        }
        return sheet;
    }

    sheet = sheet.child(section_heading("Copies", p));
    let total = live.len();
    let mut list = div().v_flex().gap_2();
    for group in live.iter().take(MAX_GROUPS) {
        list = list.child(group_card(shell, files, group, p, handle.clone()));
    }
    if total > MAX_GROUPS {
        list = list.child(note(
            &format!("Showing {MAX_GROUPS} of {total} groups. The header counts all of them."),
            p.muted_foreground,
        ));
    }
    sheet.child(list)
}

/// One line: the result tag, the group count, and Run again.
fn status_line(report: &DuplicateReport, groups: usize, handle: Entity<Shell>) -> Div {
    let tag = match &report.status {
        RunStatus::Cancelled => Tag::warning().child("Cancelled"),
        RunStatus::Failed(_) => Tag::danger().child("Failed"),
        RunStatus::Complete if groups == 0 => Tag::secondary().child("No duplicates"),
        RunStatus::Complete => {
            Tag::success().child(format!("{} groups", format_count(groups as u64)))
        }
    };
    div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(tag)
        .child(
            Button::new("duplicates-rerun")
                .outline()
                .label("Run again")
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.start_duplicates(cx));
                }),
        )
}

/// A label/value row in the sheet, with a thin rule kept by the caller.
fn fact(label: &str, value: &str, p: Palette) -> Div {
    div()
        .h_flex()
        .justify_between()
        .text_sm()
        .child(
            div()
                .text_color(p.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(value.to_string()),
        )
}

fn note(text: &str, color: Hsla) -> Div {
    div().text_sm().text_color(color).child(text.to_string())
}

fn no_duplicates(report: &DuplicateReport, handle: Entity<Shell>) -> Div {
    let content = EmptyContent::new().child(start_button(handle, "Run again"));
    div().child(
        empty_state::empty(
            Glyph::Copy,
            "No duplicates found",
            &format!(
                "Checked {} files that share a size. Nothing was changed.",
                format_count(report.files_considered)
            ),
        )
        .content(content),
    )
}

/// The files that could not be read or compared: a count, then the first few by name.
fn failures(report: &DuplicateReport, p: Palette) -> Option<Div> {
    if report.failures.is_empty() {
        return None;
    }
    let mut block = div().v_flex().gap_1().child(note(
        &format!("{} files could not be read", report.failures.len()),
        p.warning,
    ));
    for failure in report.failures.iter().take(MAX_FAILURES) {
        let name = failure
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        block = block.child(
            div()
                .text_xs()
                .text_color(p.muted_foreground)
                .truncate()
                .child(format!("{name}: {}", failure.reason.label())),
        );
    }
    Some(block)
}

/// One group: its size and reclaimable bytes on the header line, then the copies.
fn group_card(
    shell: &Shell,
    files: &FileTree,
    live: &Live<'_>,
    p: Palette,
    handle: Entity<Shell>,
) -> Div {
    let count = live.copies.len();
    let mut card = div()
        .v_flex()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .justify_between()
                .text_sm()
                .child(format!(
                    "{count} copies · {} each",
                    format_bytes(live.group.size)
                ))
                .child(
                    div()
                        .text_color(p.muted_foreground)
                        .child(format!("{} reclaimable", format_bytes(live.reclaimable))),
                ),
        );
    for member in live.copies.iter().take(MAX_COPIES) {
        card = card.child(copy_row(
            member.id,
            files,
            shell.selected,
            p,
            handle.clone(),
        ));
    }
    if count > MAX_COPIES {
        card = card.child(note(
            &format!("and {} more copies", count - MAX_COPIES),
            p.muted_foreground,
        ));
    }
    card
}

/// One copy: its name, and its folder below it. Selected copies get an outline.
fn copy_row(
    id: NodeId,
    files: &FileTree,
    selected: Option<NodeId>,
    p: Palette,
    handle: Entity<Shell>,
) -> Stateful<Div> {
    let name = files.node(id).map_or_else(String::new, |entry| {
        entry.name().to_string_lossy().into_owned()
    });
    let folder = files
        .path_of(id)
        .and_then(|path| path.parent().map(|parent| parent.display().to_string()))
        .unwrap_or_default();
    let outline = if selected == Some(id) {
        p.chart[0]
    } else {
        p.surface
    };
    let click = handle.clone();
    let menu = handle;
    div()
        .id(("duplicate-copy", id.index()))
        .w_full()
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(outline)
        .cursor_pointer()
        .hover(|style| style.bg(p.background))
        .on_click(move |event: &ClickEvent, _, cx| {
            let count = event.click_count();
            click.update(cx, |shell, cx| shell.duplicate_click(id, count, cx));
        })
        .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
            let at = event.position;
            menu.update(cx, |shell, cx| shell.open_menu(id, at, cx));
            cx.stop_propagation();
        })
        .child(div().text_sm().truncate().child(name))
        .child(
            div()
                .text_xs()
                .text_color(p.muted_foreground)
                .truncate()
                .child(folder),
        )
}
