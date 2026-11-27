#![cfg_attr(windows, windows_subsystem = "windows")]
//! Application entry point: logging, configuration, volumes, then the window.

mod assets;
mod explore;
mod extensions;
mod fast;
mod layout;
mod open;
mod platform;
mod scan;
mod search;
mod shell;
mod theme;
mod tiles;

mod drive_kind;
use anyhow::{Context as _, Result};
use gpui_kit::component::TitleBar;
use gpui_kit::*;
use timberstat_core::config::{self, AppConfig};
use timberstat_scanner::{SystemVolumes, VolumeEnumerator};

use shell::Shell;
use theme::apply_theme;

actions!(timberstat, [OpenCommandPalette]);

fn main() -> Result<()> {
    // Elevated Fast-scan helper: read the file table, write the result, exit. No window.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--fast-helper") {
        std::process::exit(fast::helper_main(&args[2..]));
    }

    // The window has no console, so the log file is the record of what happened.
    let log_dir = timberstat_core::logging::log_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("TimberStat-logs"));
    let log_file = timberstat_core::logging::init(&log_dir)
        .map_err(|error| anyhow::anyhow!("starting the log file: {error}"))?;
    tracing::info!(path = %log_file.display(), "log file");

    let config_path = config::default_config_path().context("resolving configuration path")?;
    // The app was renamed from HmstdDirStat. Its saved settings are copied over once.
    if let Some(legacy) = config::legacy_config_path() {
        match config::adopt_legacy_config(&legacy, &config_path) {
            Ok(true) => {
                tracing::info!(from = %legacy.display(), "settings copied from HmstdDirStat")
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "could not copy the HmstdDirStat settings"),
        }
    }
    let app_config = match config::load_from(&config_path) {
        Ok(loaded) => loaded,
        Err(error) => {
            // The file is left untouched; the run continues with defaults.
            tracing::error!(%error, path = %config_path.display(), "configuration rejected, using defaults for this run");
            AppConfig::default()
        }
    };
    tracing::info!(path = %config_path.display(), theme = ?app_config.theme, "starting TimberStat");
    let layout = app_config.workspace.clone().unwrap_or_default();

    let volumes = SystemVolumes.volumes().unwrap_or_else(|error| {
        tracing::error!(%error, "could not list volumes");
        Vec::new()
    });

    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(move |cx| {
            // Must run before any GPUI Kit component is used.
            gpui_kit::init(cx);
            cx.bind_keys([KeyBinding::new("ctrl-shift-p", OpenCommandPalette, None)]);
            apply_theme(app_config.theme, cx);
            theme::log_mode_changes(cx);
            let ui_scale = app_config.ui_scale;
            let drive_modes = app_config.drive_modes.clone();

            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1400.), px(900.)),
                        cx,
                    ))),
                    ..TitleBar::window_options()
                },
                cx,
                |_, cx| {
                    cx.new(|_| {
                        Shell::new(
                            volumes,
                            log_dir.clone(),
                            layout.clone(),
                            ui_scale,
                            drive_modes.clone(),
                        )
                    })
                },
            ) {
                tracing::error!(%error, "could not open the main window");
                cx.quit();
            }
        });

    Ok(())
}
