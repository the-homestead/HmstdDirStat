//! Result panels for the elevated, walk and Fast scans: progress while elevated, and the
//! totals once a scan has finished.

use gpui_kit::component::{alert::Alert, progress::Progress, spinner::Spinner, tag::Tag, *};
use gpui_kit::*;

use super::{render::drive_label, widgets::*};
use crate::scan::ScanSummary;
use hmstd_ntfs::{SummaryProgress, VolumeSummary};

pub(super) fn elevated_view(
    drive: &std::path::Path,
    granted: bool,
    stage: Option<SummaryProgress>,
    p: Palette,
) -> Div {
    let (title, detail) = match (granted, stage) {
        (false, _) => (
            format!("Waiting for administrator approval for {}", drive.display()),
            "Windows is asking for permission to run HmstdDirStat as administrator. Approve it to start the read.",
        ),
        (true, Some(SummaryProgress::Building)) => (
            format!("Building the folder tree for {}", drive.display()),
            "All records are read. Totals and the tree appear when this finishes.",
        ),
        _ => (
            format!("Reading the file table of {}", drive.display()),
            "The bar below shows how far through the Master File Table the read is.",
        ),
    };

    let mut page = div()
        .v_flex()
        .gap_4()
        .child(
            div()
                .h_flex()
                .gap_3()
                .items_center()
                .child(Spinner::new())
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                ),
        )
        .child(div().text_sm().text_color(p.muted_foreground).child(detail));

    page = match stage {
        Some(SummaryProgress::Reading { done, total }) => {
            let percent = if total == 0 {
                0.0
            } else {
                (done as f64 * 100.0 / total as f64).min(100.0) as f32
            };
            page.child(Progress::new("fast-progress").value(percent))
                .child(
                    div()
                        .text_sm()
                        .text_color(p.muted_foreground)
                        .child(format!(
                            "{} of {} records ({percent:.0}%)",
                            crate::scan::format_count(done),
                            crate::scan::format_count(total)
                        )),
                )
        }
        Some(SummaryProgress::Building) => page.child(Progress::new("fast-build").loading(true)),
        None => page,
    };
    page
}

pub(super) fn walked_result(summary: &ScanSummary, p: Palette) -> Div {
    let stats = [
        Stat {
            label: "Files",
            value: crate::scan::format_count(summary.files),
        },
        Stat {
            label: "Folders",
            value: crate::scan::format_count(summary.directories),
        },
        Stat {
            label: "Logical size",
            value: crate::scan::format_bytes(summary.logical_bytes),
        },
        Stat {
            label: "Physical size",
            value: bytes_or_na(summary.physical_bytes),
        },
        Stat {
            label: "Scan time",
            value: format!("{:.2} s", summary.elapsed.as_secs_f64()),
        },
        Stat {
            label: "Unreadable",
            value: crate::scan::format_count(summary.issues),
        },
    ];
    let mut page = result_page(
        &summary.root.display().to_string(),
        &format!("Walk · {}", summary.method),
        summary.complete,
        p,
    )
    .child(stat_grid(&stats, p));
    if !summary.complete {
        page = page.child(
            Alert::warning(
                "partial-result",
                "Some folders could not be read, so these totals are incomplete. The list below names them.",
            )
            .title("Partial result"),
        );
    }
    if !summary.issue_paths.is_empty() {
        let mut list = div()
            .v_flex()
            .gap_1()
            .child(section_heading("Unreadable folders", p));
        for line in &summary.issue_paths {
            list = list.child(div().text_sm().text_color(p.warning).child(line.clone()));
        }
        page = page.child(list);
    }
    page
}

pub(super) fn fast_result(summary: &VolumeSummary, p: Palette) -> Div {
    let stats = [
        Stat {
            label: "Files",
            value: crate::scan::format_count(summary.files),
        },
        Stat {
            label: "Folders",
            value: crate::scan::format_count(summary.directories),
        },
        Stat {
            label: "Logical size",
            value: crate::scan::format_bytes(summary.logical),
        },
        Stat {
            label: "Physical size",
            value: crate::scan::format_bytes(summary.physical),
        },
        Stat {
            label: "Read time",
            value: format!("{:.2} s", summary.total_secs),
        },
        Stat {
            label: "Orphans",
            value: crate::scan::format_count(summary.orphans),
        },
    ];
    result_page(
        &drive_label(&summary.volume),
        "Fast · NTFS file table",
        summary.complete,
        p,
    )
    .child(stat_grid(&stats, p))
}

/// Header line for a result: where it came from, how it was read, and whether it is complete.
pub(super) fn result_page(title: &str, method: &str, complete: bool, p: Palette) -> Div {
    let completeness = if complete {
        Tag::success().child("Complete")
    } else {
        Tag::warning().child("Partial")
    };
    div()
        .v_flex()
        .gap_4()
        .child(
            div()
                .h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(completeness),
        )
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child(method.to_string()),
        )
}
