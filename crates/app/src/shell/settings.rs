//! Settings, opened as a modal dialog. gpui-kit's `WindowExt::open_dialog` mounts it through the
//! window's `Root`, which supplies the overlay, Escape to close and the close button. The panel
//! itself is gpui-kit's `Settings` component.

use gpui_kit::component::{
    WindowExt as _,
    button::Button,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    *,
};
use gpui_kit::*;
use timberstat_core::config::{self, UI_SCALE_MAX, UI_SCALE_MIN};

use super::Shell;

impl Shell {
    /// Opens the settings dialog. Its fields read the live shell state on every build, so a change
    /// shows at once.
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity();
        let log_dir = self.log_dir.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let read_shell = shell.clone();
            let write_shell = shell.clone();
            let log_dir = log_dir.clone();
            let panel = Settings::new("settings").pages(vec![
                SettingPage::new("General").default_open(true).groups(vec![
                    SettingGroup::new().title("Appearance").items(vec![
                        SettingItem::new(
                            "UI scale",
                            SettingField::number_input(
                                NumberFieldOptions {
                                    min: f64::from(UI_SCALE_MIN),
                                    max: f64::from(UI_SCALE_MAX),
                                    step: 0.05,
                                },
                                move |app: &App| f64::from(read_shell.read(app).ui_scale),
                                move |value: f64, cx: &mut App| {
                                    write_shell.update(cx, |shell, cx| {
                                        shell.set_ui_scale(value as f32, cx)
                                    });
                                },
                            ),
                        )
                        .description("Scales text and spacing. Saved to config.json."),
                        SettingItem::new(
                            "Dark theme",
                            SettingField::switch(
                                |cx: &App| cx.theme().is_dark(),
                                |_: bool, cx: &mut App| crate::theme::toggle_theme(cx),
                            ),
                        ),
                    ]),
                ]),
                SettingPage::new("Diagnostics").groups(vec![
                    SettingGroup::new().title("Logs").item(SettingItem::new(
                        "Log files",
                        SettingField::render(move |_, _, _| {
                            let log_dir = log_dir.clone();
                            Button::new("open-logs")
                                .outline()
                                .small()
                                .label("Open folder")
                                .on_click(move |_, _, _| {
                                    if let Err(error) = crate::open::open_folder(&log_dir) {
                                        tracing::error!(%error, "could not open the log folder");
                                    }
                                })
                        }),
                    )),
                ]),
            ]);
            dialog
                .title("Settings")
                .w(px(640.))
                .h(px(460.))
                .child(panel)
        });
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
