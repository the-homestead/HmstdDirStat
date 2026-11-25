use anyhow::{Context as _, Result};
use gpui_kit::component::button::*;
use gpui_kit::component::label::Label;
use gpui_kit::component::*;
use gpui_kit::*;
use hmstd_core::config::{self, AppConfig, ThemePreference};

/// Root view of the Phase 1 shell. It displays the live theme and exposes one
/// working control. Nothing here scans or touches the filesystem.
struct Shell;

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = if cx.theme().is_dark() {
            "dark"
        } else {
            "light"
        };
        div()
            .v_flex()
            .gap_3()
            .size_full()
            .items_center()
            .justify_center()
            .child(Label::new("HmstdDirStat"))
            .child(Label::new(format!("Theme: {mode}")))
            .child(
                Button::new("toggle-theme")
                    .primary()
                    .label("Toggle theme")
                    .on_click(|_, _, cx| toggle_theme(cx)),
            )
    }
}

/// Applies a stored preference to the live GPUI theme.
fn apply_theme(preference: ThemePreference, cx: &mut App) {
    match preference {
        ThemePreference::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, None, cx),
        ThemePreference::System => Theme::sync_system_appearance(None, cx),
    }
}

/// Flips between light and dark, then persists the explicit choice.
fn toggle_theme(cx: &mut App) {
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

fn main() -> Result<()> {
    hmstd_core::logging::init().map_err(|error| anyhow::anyhow!(error))?;

    let config_path = config::default_config_path().context("resolving configuration path")?;
    let app_config = match config::load_from(&config_path) {
        Ok(loaded) => loaded,
        Err(error) => {
            // The file is left untouched; the run continues with defaults.
            tracing::error!(%error, path = %config_path.display(), "configuration rejected, using defaults for this run");
            AppConfig::default()
        }
    };
    tracing::info!(path = %config_path.display(), theme = ?app_config.theme, "starting HmstdDirStat");

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            // Must run before any GPUI Kit component is used.
            gpui_kit::init(cx);
            apply_theme(app_config.theme, cx);

            if let Err(error) =
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Shell))
            {
                tracing::error!(%error, "could not open the main window");
                cx.quit();
            }
        });

    Ok(())
}
