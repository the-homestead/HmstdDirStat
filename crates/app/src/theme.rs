//! Light and dark theme handling, including persisting the user's choice.

use gpui_kit::component::*;
use gpui_kit::*;
use hmstd_core::config::{self, ThemePreference};

/// Applies a stored preference to the live GPUI theme.
pub fn apply_theme(preference: ThemePreference, cx: &mut App) {
    match preference {
        ThemePreference::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, None, cx),
        ThemePreference::System => Theme::sync_system_appearance(None, cx),
    }
}

/// The mode name shown in the window, e.g. `dark`.
pub fn current_mode_name(cx: &App) -> &'static str {
    if cx.theme().is_dark() {
        "dark"
    } else {
        "light"
    }
}

/// Flips between light and dark, then persists the explicit choice.
pub fn toggle_theme(cx: &mut App) {
    let next = if cx.theme().is_dark() {
        ThemePreference::Light
    } else {
        ThemePreference::Dark
    };
    apply_theme(next, cx);

    match config::default_config_path() {
        Ok(path) => {
            if let Err(error) = config::store_theme_preference(&path, next) {
                tracing::error!(%error, "theme changed but could not be persisted");
            }
        }
        Err(error) => tracing::error!(%error, "theme changed but no config path is available"),
    }
}
