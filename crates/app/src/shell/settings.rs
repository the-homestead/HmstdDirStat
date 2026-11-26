//! The Settings panel: a toolbar button and a card over the window. UI scale is the setting so
//! far. It scales text and rem-based spacing, and it is saved to config.json.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    *,
};
use gpui_kit::*;
use hmstd_core::config::{self, DEFAULT_UI_SCALE, UI_SCALE_MAX, UI_SCALE_MIN};

use super::{Shell, widgets::*};

/// One step of the scale buttons.
const STEP: f32 = 0.05;

/// The toolbar button that opens and closes the panel.
pub(super) fn settings_button(handle: Entity<Shell>) -> Button {
    Button::new("settings")
        .ghost()
        .small()
        .icon(gpui_kit::component::IconName::Settings)
        .tooltip("Settings")
        .on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| shell.toggle_settings(cx));
        })
}

/// The panel, drawn over the window while it is open.
pub(super) fn overlay(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Option<Div> {
    if !shell.settings_open {
        return None;
    }
    let scale = shell.ui_scale;
    let log_dir = shell.log_dir.clone();
    let logs = div()
        .h_flex()
        .gap_2()
        .items_center()
        .child(div().text_sm().flex_1().child("Log files"))
        .child(
            Button::new("open-logs")
                .outline()
                .small()
                .label("Open folder")
                .tooltip(log_dir.display().to_string())
                .on_click(move |_, _, _| {
                    if let Err(error) = crate::open::open_folder(&log_dir) {
                        tracing::error!(%error, "could not open the log folder");
                    }
                }),
        );
    let (down, up, reset) = (handle.clone(), handle.clone(), handle);
    let row = div()
        .h_flex()
        .gap_2()
        .items_center()
        .child(div().text_sm().child("UI scale"))
        .child(
            Button::new("scale-down")
                .outline()
                .label("−")
                .disabled(scale <= UI_SCALE_MIN + 1e-3)
                .on_click(move |_, _, cx| {
                    down.update(cx, |shell, cx| {
                        shell.set_ui_scale(shell.ui_scale - STEP, cx)
                    });
                }),
        )
        .child(
            div()
                .w(px(56.))
                .text_center()
                .text_sm()
                .child(format!("{:.0}%", scale * 100.0)),
        )
        .child(
            Button::new("scale-up")
                .outline()
                .label("+")
                .disabled(scale >= UI_SCALE_MAX - 1e-3)
                .on_click(move |_, _, cx| {
                    up.update(cx, |shell, cx| {
                        shell.set_ui_scale(shell.ui_scale + STEP, cx)
                    });
                }),
        )
        .child(
            Button::new("scale-reset")
                .outline()
                .label("Reset")
                .disabled((scale - DEFAULT_UI_SCALE).abs() < 1e-3)
                .on_click(move |_, _, cx| {
                    reset.update(cx, |shell, cx| shell.set_ui_scale(DEFAULT_UI_SCALE, cx));
                }),
        );
    Some(
        div()
            .absolute()
            .top(px(super::toolbar::BAR_PX + 4.))
            .right(px(16.))
            .v_flex()
            .min_w(px(320.))
            .p_3()
            .gap_3()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            // Clicks on the panel must not reach the overlay, or it would close first.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(row)
            .child(
                div()
                    .text_xs()
                    .text_color(p.muted_foreground)
                    .child("Scales text and rem-based spacing. Saved to config.json. HMSTD_UI_SCALE overrides it for one run."),
            )
            .child(logs),
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
