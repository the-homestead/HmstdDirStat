//! Empty states in the shadcn style, built on gpui-component's `Empty`: an icon in a muted
//! square, a title, one line of description, and real actions where there is one to take.

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{
    button::Button,
    empty::{
        Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant,
        EmptyTitle,
    },
    *,
};
use gpui_kit::*;
use hmstd_core::workspace::WidgetKind;

use super::{Phase, Shell};

/// The header every empty state shares: icon, title, description.
fn empty(icon: Glyph, title: &str, description: &str) -> Empty {
    Empty::new().header(
        EmptyHeader::new()
            .media(
                EmptyMedia::new()
                    .with_variant(EmptyMediaVariant::Icon)
                    .child(Icon::new(icon)),
            )
            .title(EmptyTitle::new().child(title.to_string()))
            .description(EmptyDescription::new().child(description.to_string())),
    )
}

/// Centres an empty state in the widget body.
fn centred(empty: Empty) -> Div {
    div().size_full().flex().child(empty)
}

impl Shell {
    /// True while a walk or a Fast read is running.
    pub(super) fn is_scanning(&self) -> bool {
        matches!(self.phase, Phase::Walking { .. } | Phase::Elevated { .. })
    }
}

/// Summary before the first scan: one button per drive, each starting a scan.
pub(super) fn no_scan(shell: &Shell, handle: Entity<Shell>) -> Div {
    let description = if shell.fast_mode {
        "Fast NTFS read is on: Windows asks for administrator permission, then the file table is read."
    } else {
        "Pick a drive. Walk mode reads every folder and needs no administrator rights."
    };
    let mut drives = div().h_flex().flex_wrap().justify_center().gap_2();
    for (index, drive) in shell.volumes.iter().enumerate() {
        let target = drive.clone();
        let handle = handle.clone();
        drives = drives.child(
            Button::new(format!("empty-volume-{index}"))
                .outline()
                .label(format!("Scan {}", drive.display()))
                .on_click(move |_, _, cx| {
                    handle.update(cx, |shell, cx| shell.choose_volume(target.clone(), cx));
                }),
        );
    }
    let content = if shell.volumes.is_empty() {
        EmptyContent::new().child("No drives were found.")
    } else {
        EmptyContent::new().child(drives)
    };
    centred(empty(Glyph::HardDrive, "No scan yet", description).content(content))
}

/// A widget that needs the folder tree, before there is one.
pub(super) fn no_tree(kind: WidgetKind, scanning: bool) -> Div {
    let (icon, title) = match kind {
        WidgetKind::Tree => (Glyph::FolderTree, "No folder tree yet"),
        WidgetKind::Summary => (Glyph::HardDrive, "No scan yet"),
        WidgetKind::Extensions => (Glyph::Layers, "No file types yet"),
        WidgetKind::Treemap => (Glyph::LayoutGrid, "No treemap yet"),
        WidgetKind::Sunburst => (Glyph::Disc, "No sunburst yet"),
        WidgetKind::Pie => (Glyph::ChartPie, "No pie chart yet"),
    };
    let description = if scanning {
        "Scanning. This fills in when the scan finishes."
    } else {
        "Scan a drive to fill this in. Pick one in the toolbar or in Summary."
    };
    centred(empty(icon, title, description))
}

/// The workspace with every widget closed.
pub(super) fn no_widgets(handle: Entity<Shell>) -> Div {
    let reset = Button::new("empty-reset-layout")
        .outline()
        .label("Reset layout")
        .on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| shell.reset_layout(cx));
        });
    centred(
        empty(
            Glyph::LayoutDashboard,
            "No widgets shown",
            "Add one from View in the toolbar, or put every widget back.",
        )
        .content(EmptyContent::new().child(reset)),
    )
}
