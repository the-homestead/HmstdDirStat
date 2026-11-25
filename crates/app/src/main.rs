//! Application entry point: logging, configuration, volumes, then the window.

mod scan;
mod shell;
mod status;
mod theme;

use anyhow::{Context as _, Result};
use gpui_kit::*;
use hmstd_core::config::{self, AppConfig};
use hmstd_scanner::{SystemVolumes, VolumeEnumerator};

use shell::Shell;
use theme::apply_theme;

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

    let volumes = SystemVolumes.volumes().unwrap_or_else(|error| {
        tracing::error!(%error, "could not list volumes");
        Vec::new()
    });

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            // Must run before any GPUI Kit component is used.
            gpui_kit::init(cx);
            apply_theme(app_config.theme, cx);

            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("HmstdDirStat".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|_| Shell::new(volumes)),
            ) {
                tracing::error!(%error, "could not open the main window");
                cx.quit();
            }
        });

    Ok(())
}
