//! Light and dark theme handling, including persisting the user's choice.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::{scroll::ScrollbarMode, *};
use gpui_kit::*;
use hmstd_core::config::{self, ThemePreference};

/// Applies a stored preference to the live GPUI theme.
pub fn apply_theme(preference: ThemePreference, cx: &mut App) {
    match preference {
        ThemePreference::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, None, cx),
        ThemePreference::System => Theme::sync_system_appearance(None, cx),
    }
    // Scrollbars stay visible, so a list that holds more than it shows says so. Through
    // `set_scrollbar_mode`, so the base theme the scrollbars read is updated too.
    Theme::set_scrollbar_mode(ScrollbarMode::Always, cx);
}

/// Flips between light and dark, then persists the explicit choice.
pub fn toggle_theme(cx: &mut App) {
    let dark = cx.theme().is_dark();
    let next = if dark {
        ThemePreference::Light
    } else {
        ThemePreference::Dark
    };
    tracing::info!(from_dark = dark, to = ?next, "theme toggle clicked");
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

/// Logs every change of light/dark mode, whoever makes it. A mode change that no click
/// asked for shows up in the log next to the click that preceded it.
pub fn log_mode_changes(cx: &mut App) {
    let last = Rc::new(Cell::new(cx.theme().is_dark()));
    cx.observe_global::<Theme>(move |cx| {
        let dark = cx.theme().is_dark();
        if last.replace(dark) != dark {
            tracing::info!(dark, "theme mode changed");
        }
    })
    .detach();
}
