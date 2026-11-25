#![cfg_attr(windows, windows_subsystem = "windows")]
//! Application entry point: logging, configuration, volumes, then the window.

mod explore;
mod fast;
mod layout;
mod open;
mod scan;
mod shell;
mod theme;
mod tiles;

use anyhow::{Context as _, Result};
use gpui_kit::*;
use hmstd_core::config::{self, AppConfig};
use hmstd_scanner::{SystemVolumes, VolumeEnumerator};

use shell::Shell;
use theme::apply_theme;

fn main() -> Result<()> {
    // Elevated Fast-scan helper: read the file table, write the result, exit. No window.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--fast-helper") {
        std::process::exit(fast::helper_main(&args[2..]));
    }

    // The window has no console, so the log file is the record of what happened.
    let log_dir = hmstd_core::logging::log_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("HmstdDirStat-logs"));
    let log_file = hmstd_core::logging::init(&log_dir)
        .map_err(|error| anyhow::anyhow!("starting the log file: {error}"))?;
    tracing::info!(path = %log_file.display(), "log file");

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
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1400.), px(900.)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("HmstdDirStat".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|_| Shell::new(volumes, log_dir.clone())),
            ) {
                tracing::error!(%error, "could not open the main window");
                cx.quit();
            }
        });

    Ok(())
}
