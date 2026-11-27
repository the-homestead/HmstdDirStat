//! The application menu bar. gpui-kit's `AppMenuBar` sits inside the `TitleBar`, and its menus
//! come from the shell's state: File, View, Tools and Help, declared with real actions. The
//! menus are rebuilt whenever a checkmark or a disabled state changes, and the bar reloads.

use gpui_kit::component::{GlobalState, TitleBar, menu::AppMenuBar, *};
use gpui_kit::*;
use gpui_kit::{Menu, MenuItem, OwnedMenu};
use timberstat_core::workspace::{
    WidgetKind,
    saved::{WORKSPACE_PRESETS, WorkspacePreset},
};

use super::{Shell, widgets::*};

/// Height the menu row shares with the title bar. The panels open below it.
pub(super) const MENU_PX: f32 = 34.0;

actions!(
    timberstat_menu,
    [
        StartDuplicates,
        StopScan,
        OpenLogFolder,
        ShowDiskOverview,
        ShowDeepDirectory,
        ShowDuplicateAnalysis,
        ShowFileTypeAnalysis,
        ToggleTree,
        ToggleSummary,
        ToggleExtensions,
        ToggleTreemap,
        ToggleSunburst,
        TogglePie,
        ToggleDuplicates,
        ResetLayout,
        ManageWorkspaces,
        ToggleDarkTheme,
        OpenSettings
    ]
);

/// The state the menus depend on. When it changes, the menus are rebuilt and the bar reloads.
fn signature(shell: &Shell, dark: bool) -> String {
    let shown: Vec<bool> = WidgetKind::ALL
        .into_iter()
        .map(|kind| shell.layout.is_shown(kind))
        .collect();
    format!(
        "{}|{:?}|{}|{}",
        shell.workspaces.document().active,
        shown,
        shell.is_busy(),
        dark
    )
}

fn preset_item(shell: &Shell, preset: WorkspacePreset) -> MenuItem {
    let active = shell.workspaces.document().active == preset.name();
    let item = match preset {
        WorkspacePreset::DiskOverview => MenuItem::action(preset.name(), ShowDiskOverview),
        WorkspacePreset::DeepDirectoryAnalysis => {
            MenuItem::action(preset.name(), ShowDeepDirectory)
        }
        WorkspacePreset::DuplicateAnalysis => {
            MenuItem::action(preset.name(), ShowDuplicateAnalysis)
        }
        WorkspacePreset::FileTypeAnalysis => MenuItem::action(preset.name(), ShowFileTypeAnalysis),
    };
    item.checked(active)
}

fn widget_item(shell: &Shell, kind: WidgetKind) -> MenuItem {
    let shown = shell.layout.is_shown(kind);
    let item = match kind {
        WidgetKind::Tree => MenuItem::action(kind.name(), ToggleTree),
        WidgetKind::Summary => MenuItem::action(kind.name(), ToggleSummary),
        WidgetKind::Extensions => MenuItem::action(kind.name(), ToggleExtensions),
        WidgetKind::Treemap => MenuItem::action(kind.name(), ToggleTreemap),
        WidgetKind::Sunburst => MenuItem::action(kind.name(), ToggleSunburst),
        WidgetKind::Pie => MenuItem::action(kind.name(), TogglePie),
        WidgetKind::Duplicates => MenuItem::action(kind.name(), ToggleDuplicates),
    };
    item.checked(shown)
}

fn menus(shell: &Shell, dark: bool) -> Vec<OwnedMenu> {
    let busy = shell.is_busy();
    let file = Menu {
        name: "File".into(),
        items: vec![
            MenuItem::action("Find duplicates", StartDuplicates).disabled(busy),
            MenuItem::action("Stop scan", StopScan).disabled(!busy),
        ],
        disabled: false,
    };
    let mut view_items: Vec<MenuItem> = WORKSPACE_PRESETS
        .into_iter()
        .map(|preset| preset_item(shell, preset))
        .collect();
    view_items.push(MenuItem::separator());
    view_items.extend(
        WidgetKind::ALL
            .into_iter()
            .map(|kind| widget_item(shell, kind)),
    );
    view_items.extend([
        MenuItem::separator(),
        MenuItem::action("Reset layout", ResetLayout),
        MenuItem::action("Manage workspaces…", ManageWorkspaces),
        MenuItem::separator(),
        MenuItem::action("Dark theme", ToggleDarkTheme).checked(dark),
    ]);
    let view = Menu {
        name: "View".into(),
        items: view_items,
        disabled: false,
    };
    let tools = Menu {
        name: "Tools".into(),
        items: vec![
            MenuItem::action("Command palette…   Ctrl+Shift+P", crate::OpenCommandPalette),
            MenuItem::action("Settings…", OpenSettings),
        ],
        disabled: false,
    };
    let help = Menu {
        name: "Help".into(),
        items: vec![MenuItem::action("Open log folder", OpenLogFolder)],
        disabled: false,
    };
    [file, view, tools, help]
        .into_iter()
        .map(Menu::owned)
        .collect()
}

impl Shell {
    /// Rebuilds the menus when their state changed, creates the bar on first use, and returns it.
    pub(super) fn menu_bar(&mut self, cx: &mut Context<Self>) -> Entity<AppMenuBar> {
        let dark = cx.theme().is_dark();
        let sig = signature(self, dark);
        if self.menu_signature != sig {
            self.menu_signature = sig;
            GlobalState::global_mut(cx).set_app_menus(menus(self, dark));
            if let Some(bar) = self.menu_bar.clone() {
                bar.update(cx, |bar, cx| bar.reload(cx));
            }
        }
        match self.menu_bar.clone() {
            Some(bar) => bar,
            None => {
                let bar = AppMenuBar::new(cx);
                self.menu_bar = Some(bar.clone());
                bar
            }
        }
    }

    pub(super) fn open_workspace_panel(&mut self, cx: &mut Context<Self>) {
        self.workspace_panel = true;
        cx.notify();
    }
}

/// The title bar: gpui-kit's menu bar on the left. The TitleBar draws the window controls and
/// moves the window when its empty space is dragged.
pub(super) fn title_bar(shell: &mut Shell, cx: &mut Context<Shell>) -> TitleBar {
    let bar = shell.menu_bar(cx);
    TitleBar::new().child(div().flex().items_center().child(bar))
}

/// Action handlers for the menu bar. Each item reaches the same shell code as the toolbar.
pub(super) fn bind(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|s: &mut Shell, _: &StartDuplicates, _, c| s.start_duplicates(c)))
        .on_action(cx.listener(|s: &mut Shell, _: &StopScan, _, c| s.cancel_walk(c)))
        .on_action(cx.listener(|s: &mut Shell, _: &OpenLogFolder, _, _| {
            if let Err(error) = crate::open::open_folder(&s.log_dir) {
                tracing::error!(%error, "could not open the log folder");
            }
        }))
        .on_action(cx.listener(|s: &mut Shell, _: &ShowDiskOverview, _, c| {
            s.switch_workspace(WorkspacePreset::DiskOverview.name(), c)
        }))
        .on_action(cx.listener(|s: &mut Shell, _: &ShowDeepDirectory, _, c| {
            s.switch_workspace(WorkspacePreset::DeepDirectoryAnalysis.name(), c)
        }))
        .on_action(
            cx.listener(|s: &mut Shell, _: &ShowDuplicateAnalysis, _, c| {
                s.switch_workspace(WorkspacePreset::DuplicateAnalysis.name(), c)
            }),
        )
        .on_action(
            cx.listener(|s: &mut Shell, _: &ShowFileTypeAnalysis, _, c| {
                s.switch_workspace(WorkspacePreset::FileTypeAnalysis.name(), c)
            }),
        )
        .on_action(
            cx.listener(|s: &mut Shell, _: &ToggleTree, _, c| s.toggle_widget(WidgetKind::Tree, c)),
        )
        .on_action(cx.listener(|s: &mut Shell, _: &ToggleSummary, _, c| {
            s.toggle_widget(WidgetKind::Summary, c)
        }))
        .on_action(cx.listener(|s: &mut Shell, _: &ToggleExtensions, _, c| {
            s.toggle_widget(WidgetKind::Extensions, c)
        }))
        .on_action(cx.listener(|s: &mut Shell, _: &ToggleTreemap, _, c| {
            s.toggle_widget(WidgetKind::Treemap, c)
        }))
        .on_action(cx.listener(|s: &mut Shell, _: &ToggleSunburst, _, c| {
            s.toggle_widget(WidgetKind::Sunburst, c)
        }))
        .on_action(
            cx.listener(|s: &mut Shell, _: &TogglePie, _, c| s.toggle_widget(WidgetKind::Pie, c)),
        )
        .on_action(cx.listener(|s: &mut Shell, _: &ToggleDuplicates, _, c| {
            s.toggle_widget(WidgetKind::Duplicates, c)
        }))
        .on_action(cx.listener(|s: &mut Shell, _: &ResetLayout, _, c| s.reset_layout(c)))
        .on_action(
            cx.listener(|s: &mut Shell, _: &ManageWorkspaces, _, c| s.open_workspace_panel(c)),
        )
        .on_action(
            cx.listener(|_: &mut Shell, _: &ToggleDarkTheme, _, c| crate::theme::toggle_theme(c)),
        )
        .on_action(cx.listener(|s: &mut Shell, _: &OpenSettings, _, c| s.toggle_settings(c)))
}

/// The workspace manager panel, drawn under the title bar while it is open. A click outside it
/// closes it.
pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    if !shell.workspace_panel {
        return None;
    }
    let close = handle.clone();
    Some(
        div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                close.update(cx, |shell, cx| shell.close_workspace_panel(cx));
            })
            .child(
                div()
                    .absolute()
                    .top(px(MENU_PX + super::toolbar::BAR_PX + 4.))
                    .right(px(16.))
                    .w(px(340.))
                    .flex()
                    .flex_col()
                    .p_2()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(super::workspace_menu::section(shell, p, handle)),
            ),
    )
}
