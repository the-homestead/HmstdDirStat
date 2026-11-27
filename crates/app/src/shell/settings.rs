//! The Settings panel, built on gpui-kit's `Settings` component: pages and grouped items. UI scale
//! and the theme are saved through the shell; the log folder opens from a button in the page.

use gpui_kit::component::{
    button::Button,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    *,
};
use gpui_kit::*;
use timberstat_core::config::{self, UI_SCALE_MAX, UI_SCALE_MIN};

use super::{Shell, menubar::MENU_PX, widgets::*};

/// The panel, drawn under the menu row while it is open. A click outside it closes it.
pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    if !shell.settings_open {
        return None;
    }
    let scale = f64::from(shell.ui_scale);
    let scale_shell = handle.clone();
    let log_dir = shell.log_dir.clone();

    let appearance = SettingGroup::new().title("Appearance").items(vec![
        SettingItem::new(
            "UI scale",
            SettingField::number_input(
                NumberFieldOptions {
                    min: f64::from(UI_SCALE_MIN),
                    max: f64::from(UI_SCALE_MAX),
                    step: 0.05,
                },
                move |_: &App| scale,
                move |value: f64, cx: &mut App| {
                    scale_shell.update(cx, |shell, cx| shell.set_ui_scale(value as f32, cx));
                },
            ),
        )
        .description(
            "Scales text and spacing. Saved to config.json. HMSTD_UI_SCALE overrides it for one run.",
        ),
        SettingItem::new(
            "Dark theme",
            SettingField::switch(
                |cx: &App| cx.theme().is_dark(),
                |_: bool, cx: &mut App| crate::theme::toggle_theme(cx),
            ),
        ),
    ]);
    let diagnostics = SettingGroup::new()
        .title("Diagnostics")
        .item(SettingItem::new(
            "Log files",
            SettingField::render(move |_, _, _| {
                let log_dir = log_dir.clone();
                Button::new("open-logs")
                    .outline()
                    .small()
                    .label("Open folder")
                    .tooltip(log_dir.display().to_string())
                    .on_click(move |_, _, _| {
                        if let Err(error) = crate::open::open_folder(&log_dir) {
                            tracing::error!(%error, "could not open the log folder");
                        }
                    })
            }),
        ));
    let panel = Settings::new("settings").pages(vec![
        SettingPage::new("General")
            .default_open(true)
            .groups(vec![appearance]),
        SettingPage::new("Diagnostics").groups(vec![diagnostics]),
    ]);

    // A press outside the panel closes it. There is no full-window layer, so the menu bar and the
    // toolbar stay clickable while the panel is open.
    let close = handle;
    Some(
        div()
            .absolute()
            .top(px(MENU_PX + 4.))
            .right(px(16.))
            .w(px(640.))
            .h(px(440.))
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .on_mouse_down_out(move |_, _, cx| {
                close.update(cx, |shell, cx| shell.toggle_settings(cx));
            })
            .child(panel),
    )
}

impl Shell {
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        cx.notify();
    }

    /// Applies a UI scale at once and saves it. Out-of-range values are clamped.
    pub fn set_ui_scale(&mut self, scale: f32, cx: &mut Context<Self>) {
        let scale = config::clamp_ui_scale(scale);
        self.ui_scale = scale;
        match config::default_config_path() {
            Ok(path) => {
                if let Err(error) = config::store_ui_scale(&path, scale) {
                    tracing::error!(%error, "UI scale changed but could not be saved");
                }
            }
            Err(error) => {
                tracing::error!(%error, "UI scale changed but no config path is available")
            }
        }
        cx.notify();
    }
}
